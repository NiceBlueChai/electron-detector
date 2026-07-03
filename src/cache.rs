//! Tests and stores JSON cache data for detector scan results.

use crate::detect::ElectronApp;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// @brief Serialized detector cache payload saved on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheData {
    /// @brief Unix timestamp when this cache was built.
    pub built_at_unix: u64,
    /// @brief Raw Electron candidate paths discovered during the scan.
    pub candidates: Vec<String>,
    /// @brief Installed Electron apps inferred from candidate paths.
    pub installed_apps: Vec<ElectronApp>,
    /// @brief NTFS volume journal states used to resume later scans.
    pub volumes: Vec<VolumeState>,
}

/// @brief Serialized NTFS volume journal state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeState {
    /// @brief Volume root path.
    pub root: String,
    /// @brief NTFS USN journal identifier.
    pub journal_id: u64,
    /// @brief Next USN value to continue scanning from.
    pub next_usn: i64,
}

/// @brief Returns the default per-user cache file path.
pub fn default_cache_path() -> Result<PathBuf, String> {
    let local_app_data =
        std::env::var_os("LOCALAPPDATA").ok_or_else(|| "LOCALAPPDATA is not set".to_string())?;

    Ok(PathBuf::from(local_app_data)
        .join("electron-detector")
        .join("cache.json"))
}

/// @brief Loads detector cache data from a JSON file.
pub fn load_cache(path: &Path) -> Result<CacheData, String> {
    let text = fs::read_to_string(path)
        .map_err(|err| format!("failed to read cache {}: {err}", path.display()))?;

    serde_json::from_str(&text)
        .map_err(|err| format!("failed to parse cache {}: {err}", path.display()))
}

/// @brief Saves detector cache data as pretty JSON.
pub fn save_cache(path: &Path, cache: &CacheData) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create cache directory {}: {err}",
                parent.display()
            )
        })?;
    }

    let text = serde_json::to_string_pretty(cache)
        .map_err(|err| format!("failed to encode cache: {err}"))?;

    fs::write(path, text).map_err(|err| format!("failed to write cache {}: {err}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_and_loads_cache_data() {
        let cache = CacheData {
            built_at_unix: 123,
            candidates: vec![r"C:\Apps\Code\resources\app.asar".to_string()],
            installed_apps: vec![ElectronApp {
                name: "Code".to_string(),
                path: r"C:\Apps\Code".to_string(),
                sources: vec!["ntfs".to_string()],
            }],
            volumes: vec![VolumeState {
                root: r"C:\".to_string(),
                journal_id: 456,
                next_usn: 789,
            }],
        };
        let path = std::env::temp_dir().join(format!(
            "electron-detector-cache-{}.json",
            std::process::id()
        ));

        save_cache(&path, &cache).unwrap();
        let loaded = load_cache(&path).unwrap();
        std::fs::remove_file(&path).unwrap();

        assert_eq!(loaded, cache);
    }
}
