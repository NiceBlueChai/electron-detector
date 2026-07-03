//! Refreshes the installed app cache from Windows NTFS MFT and USN records.

use crate::cache::CacheData;

/// @brief Builds a fresh installed app cache from NTFS MFT and USN records.
#[cfg(windows)]
pub fn refresh_cache() -> Result<CacheData, String> {
    windows_refresh::refresh_cache()
}

/// @brief Returns an error on platforms that cannot read NTFS journals.
#[cfg(not(windows))]
pub fn refresh_cache() -> Result<CacheData, String> {
    Err("NTFS refresh is only available on Windows".to_string())
}

/// @brief Formats the permission error shown when a volume requires elevation.
pub fn refresh_required_message(root: &str) -> String {
    format!("administrator permission is required to refresh NTFS index for {root}")
}

#[cfg(windows)]
mod windows_refresh {
    use super::refresh_required_message;
    use crate::cache::{CacheData, VolumeState};
    use crate::detect::{candidate_kind, installed_apps_from_candidates};
    use std::collections::{BTreeMap, BTreeSet};
    use std::ffi::c_void;
    use std::fs::{File, OpenOptions};
    use std::mem::size_of;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use std::time::{SystemTime, UNIX_EPOCH};
    use windows::core::{Error, HRESULT, PCWSTR};
    use windows::Win32::Foundation::{
        ERROR_ACCESS_DENIED, ERROR_HANDLE_EOF, ERROR_NO_MORE_FILES, HANDLE,
    };
    use windows::Win32::Storage::FileSystem::{
        GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW, FILE_ATTRIBUTE_DIRECTORY,
        FILE_GENERIC_READ, FILE_SHARE_DELETE, FILE_SHARE_MODE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    use windows::Win32::System::IO::DeviceIoControl;

    const DRIVE_FIXED: u32 = 3;
    const FSCTL_ENUM_USN_DATA: u32 = 0x0009_00b3;
    const FSCTL_QUERY_USN_JOURNAL: u32 = 0x0009_00f4;
    const USN_RECORD_V2_SIZE: usize = 60;
    const ENUM_BUFFER_SIZE: usize = 1024 * 1024;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct MftEnumDataV0 {
        start_file_reference_number: u64,
        low_usn: i64,
        high_usn: i64,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct UsnJournalDataV0 {
        journal_id: u64,
        first_usn: i64,
        next_usn: i64,
        lowest_valid_usn: i64,
        max_usn: i64,
        maximum_size: u64,
        allocation_delta: u64,
    }

    struct NtfsVolume {
        root: String,
        device: String,
    }

    struct Entry {
        parent: u64,
        name: String,
    }

    struct VolumeHandle(File);

    pub fn refresh_cache() -> Result<CacheData, String> {
        let mut candidates = BTreeSet::new();
        let mut volumes = Vec::new();

        for volume in ntfs_volumes()? {
            let handle = open_volume(&volume)?;
            let journal = query_usn_journal(handle.raw(), &volume)?;
            let volume_candidates = enumerate_candidates(handle.raw(), &volume, journal.next_usn)?;

            candidates.extend(volume_candidates);
            volumes.push(VolumeState {
                root: volume.root,
                journal_id: journal.journal_id,
                next_usn: journal.next_usn,
            });
        }

        let candidates: Vec<_> = candidates.into_iter().collect();

        Ok(CacheData {
            built_at_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|err| format!("system clock is before Unix epoch: {err}"))?
                .as_secs(),
            installed_apps: installed_apps_from_candidates(candidates.clone()),
            candidates,
            volumes,
        })
    }

    fn ntfs_volumes() -> Result<Vec<NtfsVolume>, String> {
        let mask = unsafe { GetLogicalDrives() };
        if mask == 0 {
            return Err("failed to list logical drives".to_string());
        }

        let mut volumes = Vec::new();
        for index in 0..26 {
            if mask & (1 << index) == 0 {
                continue;
            }

            let letter = (b'A' + index as u8) as char;
            let root = format!("{letter}:\\");
            if unsafe { GetDriveTypeW(PCWSTR(to_wide(&root).as_ptr())) } != DRIVE_FIXED {
                continue;
            }

            if file_system_name(&root)?.eq_ignore_ascii_case("NTFS") {
                volumes.push(NtfsVolume {
                    root,
                    device: format!(r"\\.\{letter}:"),
                });
            }
        }

        Ok(volumes)
    }

    fn file_system_name(root: &str) -> Result<String, String> {
        let mut file_system = [0u16; 32];
        unsafe {
            GetVolumeInformationW(
                PCWSTR(to_wide(root).as_ptr()),
                None,
                None,
                None,
                None,
                Some(&mut file_system),
            )
            .map_err(|err| format!("failed to read volume information for {root}: {err}"))?;
        }

        Ok(from_wide_z(&file_system))
    }

    fn open_volume(volume: &NtfsVolume) -> Result<VolumeHandle, String> {
        let share = FILE_SHARE_MODE(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0);
        match OpenOptions::new()
            .read(true)
            .access_mode(FILE_GENERIC_READ.0)
            .share_mode(share.0)
            .open(&volume.device)
        {
            Ok(file) => Ok(VolumeHandle(file)),
            Err(err) if err.raw_os_error() == Some(ERROR_ACCESS_DENIED.0 as i32) => {
                Err(refresh_required_message(&volume.root))
            }
            Err(err) => Err(format!("failed to open NTFS volume {}: {err}", volume.root)),
        }
    }

    impl VolumeHandle {
        fn raw(&self) -> HANDLE {
            HANDLE(self.0.as_raw_handle())
        }
    }

    fn query_usn_journal(handle: HANDLE, volume: &NtfsVolume) -> Result<UsnJournalDataV0, String> {
        let mut journal = UsnJournalDataV0::default();
        let mut returned = 0;

        unsafe {
            DeviceIoControl(
                handle,
                FSCTL_QUERY_USN_JOURNAL,
                None,
                0,
                Some((&mut journal as *mut UsnJournalDataV0).cast::<c_void>()),
                size_of::<UsnJournalDataV0>() as u32,
                Some(&mut returned),
                None,
            )
            .map_err(|err| fsctl_error(volume, err, "query USN journal"))?;
        }

        Ok(journal)
    }

    fn enumerate_candidates(
        handle: HANDLE,
        volume: &NtfsVolume,
        high_usn: i64,
    ) -> Result<BTreeSet<String>, String> {
        let mut query = MftEnumDataV0 {
            start_file_reference_number: 0,
            low_usn: 0,
            high_usn,
        };
        let mut buffer = vec![0u8; ENUM_BUFFER_SIZE];
        let mut entries = BTreeMap::new();
        let mut interesting = BTreeSet::new();
        let mut root_exes = BTreeSet::new();

        loop {
            let mut returned = 0;
            let result = unsafe {
                DeviceIoControl(
                    handle,
                    FSCTL_ENUM_USN_DATA,
                    Some((&query as *const MftEnumDataV0).cast::<c_void>()),
                    size_of::<MftEnumDataV0>() as u32,
                    Some(buffer.as_mut_ptr().cast::<c_void>()),
                    buffer.len() as u32,
                    Some(&mut returned),
                    None,
                )
            };

            match result {
                Ok(()) => {
                    if returned as usize <= size_of::<u64>() {
                        break;
                    }

                    query.start_file_reference_number = read_u64(&buffer[..size_of::<u64>()])?;
                    parse_records(
                        &buffer[..returned as usize],
                        &mut entries,
                        &mut interesting,
                        &mut root_exes,
                    )?;
                }
                Err(err) if is_expected_enum_end(&err) => break,
                Err(err) => return Err(fsctl_error(volume, err, "enumerate NTFS records")),
            }
        }

        let mut candidates = BTreeSet::new();
        for reference in interesting {
            if let Some(path) = resolve_path(&volume.root, &entries, reference) {
                if candidate_kind(&path).is_some() {
                    candidates.insert(path);
                }
            }
        }
        let app_roots: BTreeSet<_> = installed_apps_from_candidates(candidates.iter().cloned())
            .into_iter()
            .map(|app| app.path.to_ascii_lowercase())
            .collect();

        for reference in root_exes {
            if let Some(path) = resolve_path(&volume.root, &entries, reference) {
                if exe_parent(&path)
                    .is_some_and(|root| app_roots.contains(&root.to_ascii_lowercase()))
                {
                    candidates.insert(path);
                }
            }
        }

        Ok(candidates)
    }

    fn parse_records(
        buffer: &[u8],
        entries: &mut BTreeMap<u64, Entry>,
        interesting: &mut BTreeSet<u64>,
        root_exes: &mut BTreeSet<u64>,
    ) -> Result<(), String> {
        let mut offset = size_of::<u64>();

        while offset + USN_RECORD_V2_SIZE <= buffer.len() {
            let record = &buffer[offset..];
            let record_length = read_u32(record)? as usize;
            if record_length == 0 {
                break;
            }
            if offset + record_length > buffer.len() || record_length < USN_RECORD_V2_SIZE {
                return Err("received malformed USN record".to_string());
            }

            let major_version = read_u16(&record[4..])?;
            if major_version == 2 {
                parse_record_v2(&record[..record_length], entries, interesting, root_exes)?;
            }

            offset += record_length;
        }

        Ok(())
    }

    fn parse_record_v2(
        record: &[u8],
        entries: &mut BTreeMap<u64, Entry>,
        interesting: &mut BTreeSet<u64>,
        root_exes: &mut BTreeSet<u64>,
    ) -> Result<(), String> {
        let reference = read_u64(&record[8..])?;
        let parent = read_u64(&record[16..])?;
        let attributes = read_u32(&record[52..])?;
        let name_length = read_u16(&record[56..])? as usize;
        let name_offset = read_u16(&record[58..])? as usize;

        if name_offset < USN_RECORD_V2_SIZE
            || name_offset + name_length > record.len()
            || !name_length.is_multiple_of(2)
        {
            return Err("received malformed USN record name".to_string());
        }

        let name = utf16_name(&record[name_offset..name_offset + name_length])?;
        let is_directory = attributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0;
        let is_interesting_file = name.eq_ignore_ascii_case("app.asar")
            || name.eq_ignore_ascii_case("electron.exe")
            || name.eq_ignore_ascii_case("package.json");
        let is_root_exe =
            exe_stem(&name).is_some_and(|stem| !stem.eq_ignore_ascii_case("electron"));

        if is_directory || is_interesting_file || is_root_exe {
            entries.insert(reference, Entry { parent, name });
        }
        if is_interesting_file {
            interesting.insert(reference);
        }
        if is_root_exe {
            root_exes.insert(reference);
        }

        Ok(())
    }

    fn exe_parent(path: &str) -> Option<&str> {
        let separator_index = path
            .char_indices()
            .rev()
            .find(|(_, character)| matches!(character, '\\' | '/'))?
            .0;
        let file_name = &path[separator_index + 1..];
        let stem = exe_stem(file_name)?;
        if stem.eq_ignore_ascii_case("electron") {
            return None;
        }

        let root = path[..separator_index].trim_end_matches(['\\', '/']);
        if root.is_empty() {
            None
        } else {
            Some(root)
        }
    }

    fn exe_stem(file_name: &str) -> Option<&str> {
        let suffix_start = file_name.len().checked_sub(4)?;
        let suffix = file_name.get(suffix_start..)?;
        if suffix.eq_ignore_ascii_case(".exe") {
            file_name.get(..suffix_start)
        } else {
            None
        }
    }

    fn resolve_path(root: &str, entries: &BTreeMap<u64, Entry>, reference: u64) -> Option<String> {
        let mut current = reference;
        let mut names = Vec::new();
        let mut seen = BTreeSet::new();

        while seen.insert(current) {
            let entry = entries.get(&current)?;
            if entry.name != "." {
                names.push(entry.name.as_str());
            }
            if entry.parent == current || entry.parent == 0 || !entries.contains_key(&entry.parent)
            {
                break;
            }
            current = entry.parent;
        }

        names.reverse();
        if names.is_empty() {
            None
        } else {
            Some(format!("{root}{}", names.join("\\")))
        }
    }

    fn utf16_name(bytes: &[u8]) -> Result<String, String> {
        let mut wide = Vec::with_capacity(bytes.len() / 2);
        for chunk in bytes.chunks_exact(2) {
            wide.push(u16::from_le_bytes([chunk[0], chunk[1]]));
        }

        String::from_utf16(&wide).map_err(|err| format!("failed to decode USN record name: {err}"))
    }

    fn read_u16(bytes: &[u8]) -> Result<u16, String> {
        let bytes = bytes
            .get(..2)
            .ok_or_else(|| "received truncated USN record".to_string())?;

        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn read_u32(bytes: &[u8]) -> Result<u32, String> {
        let bytes = bytes
            .get(..4)
            .ok_or_else(|| "received truncated USN record".to_string())?;

        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_u64(bytes: &[u8]) -> Result<u64, String> {
        let bytes = bytes
            .get(..8)
            .ok_or_else(|| "received truncated USN record".to_string())?;

        Ok(u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn to_wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn from_wide_z(wide: &[u16]) -> String {
        let end = wide
            .iter()
            .position(|code| *code == 0)
            .unwrap_or(wide.len());

        String::from_utf16_lossy(&wide[..end])
    }

    fn is_expected_enum_end(err: &Error) -> bool {
        is_win32_error(err, ERROR_HANDLE_EOF.0) || is_win32_error(err, ERROR_NO_MORE_FILES.0)
    }

    fn fsctl_error(volume: &NtfsVolume, err: Error, operation: &str) -> String {
        if is_win32_error(&err, ERROR_ACCESS_DENIED.0) {
            refresh_required_message(&volume.root)
        } else {
            format!("failed to {operation} for {}: {err}", volume.root)
        }
    }

    fn is_win32_error(err: &Error, code: u32) -> bool {
        err.code() == HRESULT::from_win32(code)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn maps_fsctl_access_denied_to_refresh_required_message() {
            let volume = NtfsVolume {
                root: r"C:\".to_string(),
                device: r"\\.\C:".to_string(),
            };
            let err = Error::from_hresult(HRESULT::from_win32(ERROR_ACCESS_DENIED.0));

            assert_eq!(
                fsctl_error(&volume, err, "query USN journal"),
                r"administrator permission is required to refresh NTFS index for C:\"
            );
        }

        #[test]
        fn rejects_record_names_inside_v2_header() {
            let mut record = vec![0u8; USN_RECORD_V2_SIZE + 2];
            let record_len = record.len() as u32;
            record[0..4].copy_from_slice(&record_len.to_le_bytes());
            record[4..6].copy_from_slice(&2u16.to_le_bytes());
            record[56..58].copy_from_slice(&2u16.to_le_bytes());
            record[58..60].copy_from_slice(&58u16.to_le_bytes());

            let err = parse_record_v2(
                &record,
                &mut BTreeMap::new(),
                &mut BTreeSet::new(),
                &mut BTreeSet::new(),
            )
            .unwrap_err();

            assert_eq!(err, "received malformed USN record name");
        }

        #[test]
        fn keeps_resources_directory_only_for_path_reconstruction() {
            let record = test_record_v2(10, 5, FILE_ATTRIBUTE_DIRECTORY.0, "resources");
            let mut entries = BTreeMap::new();
            let mut interesting = BTreeSet::new();
            let mut root_exes = BTreeSet::new();

            parse_record_v2(&record, &mut entries, &mut interesting, &mut root_exes).unwrap();

            assert!(entries.contains_key(&10));
            assert!(interesting.is_empty());
        }

        #[test]
        fn records_non_electron_exe_for_app_name_lookup() {
            let record = test_record_v2(10, 5, 0, "ATLaserTrackClient.exe");
            let mut entries = BTreeMap::new();
            let mut interesting = BTreeSet::new();
            let mut root_exes = BTreeSet::new();

            parse_record_v2(&record, &mut entries, &mut interesting, &mut root_exes).unwrap();

            assert!(entries.contains_key(&10));
            assert!(interesting.is_empty());
            assert!(root_exes.contains(&10));
        }

        fn test_record_v2(reference: u64, parent: u64, attributes: u32, name: &str) -> Vec<u8> {
            let name_bytes: Vec<_> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
            let record_len = USN_RECORD_V2_SIZE + name_bytes.len();
            let mut record = vec![0u8; record_len];

            record[0..4].copy_from_slice(&(record_len as u32).to_le_bytes());
            record[4..6].copy_from_slice(&2u16.to_le_bytes());
            record[8..16].copy_from_slice(&reference.to_le_bytes());
            record[16..24].copy_from_slice(&parent.to_le_bytes());
            record[52..56].copy_from_slice(&attributes.to_le_bytes());
            record[56..58].copy_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            record[58..60].copy_from_slice(&(USN_RECORD_V2_SIZE as u16).to_le_bytes());
            record[USN_RECORD_V2_SIZE..].copy_from_slice(&name_bytes);
            record
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_refresh_permission_message() {
        assert_eq!(
            refresh_required_message(r"C:\"),
            r"administrator permission is required to refresh NTFS index for C:\"
        );
    }
}
