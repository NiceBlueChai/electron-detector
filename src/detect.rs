//! Classifies pure Electron install path candidates without filesystem access.

use std::collections::BTreeMap;

/// @brief Identifies the Electron signal found in a candidate path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateKind {
    /// @brief A bundled Electron app archive named app.asar.
    AppAsar,
    /// @brief An unpacked Electron app package file under resources/app.
    AppPackageJson,
    /// @brief An Electron executable named electron.exe.
    ElectronExe,
    /// @brief A path containing a resources directory component.
    ResourcesDir,
}

/// @brief Describes an installed Electron app inferred from candidate paths.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ElectronApp {
    /// @brief Display name inferred from the app root directory name.
    pub name: String,
    /// @brief App root path inferred from the components before resources.
    pub path: String,
    /// @brief Detection source labels that contributed this app.
    pub sources: Vec<String>,
}

/// @brief Classifies a path string as an Electron candidate, without touching the filesystem.
pub fn candidate_kind(path: &str) -> Option<CandidateKind> {
    let parts = path_components(path);
    let last = parts.last()?;

    if is_resources_app_package(&parts) {
        Some(CandidateKind::AppPackageJson)
    } else if last.eq_ignore_ascii_case("app.asar") {
        Some(CandidateKind::AppAsar)
    } else if last.eq_ignore_ascii_case("electron.exe") {
        Some(CandidateKind::ElectronExe)
    } else if parts
        .iter()
        .any(|part| part.eq_ignore_ascii_case("resources"))
    {
        Some(CandidateKind::ResourcesDir)
    } else {
        None
    }
}

/// @brief Groups Electron candidate paths into deduplicated installed apps.
pub fn installed_apps_from_candidates<I>(paths: I) -> Vec<ElectronApp>
where
    I: IntoIterator<Item = String>,
{
    let mut apps = BTreeMap::new();
    let mut root_exes = BTreeMap::new();

    for path in paths {
        if let Some((root, name)) = root_exe_name(&path) {
            root_exes
                .entry(root.to_ascii_lowercase())
                .or_insert_with(|| (root.to_string(), name.to_string()));
        }

        match candidate_kind(&path) {
            Some(
                CandidateKind::AppAsar | CandidateKind::AppPackageJson | CandidateKind::ElectronExe,
            ) => {}
            Some(CandidateKind::ResourcesDir) | None => continue,
        }

        let Some((root, name)) = app_root_before_resources(&path) else {
            continue;
        };

        apps.entry(root.to_ascii_lowercase())
            .or_insert(ElectronApp {
                name: name.to_string(),
                path: root.to_string(),
                sources: vec!["ntfs".to_string()],
            });
    }

    for (root, app) in &mut apps {
        if is_generic_root_name(&app.name) {
            if let Some((_, name)) = root_exes.get(root) {
                app.name = name.clone();
            }
        }
        if should_use_parent_root_name(&app.name) {
            if let Some((parent_root, name)) = parent_root_exe(root, &root_exes) {
                app.name = name.to_string();
                app.path = parent_root.to_string();
            } else if let Some((parent_root, name)) = parent_root_name(&app.path) {
                app.name = name.to_string();
                app.path = parent_root.to_string();
            }
        }
    }

    let mut deduped = BTreeMap::new();
    for app in apps.into_values() {
        deduped.entry(app.path.to_ascii_lowercase()).or_insert(app);
    }

    deduped.into_values().collect()
}

fn path_components(path: &str) -> Vec<&str> {
    component_spans(path)
        .into_iter()
        .map(|(part, _, _)| part)
        .collect()
}

fn app_root_before_resources(path: &str) -> Option<(&str, &str)> {
    let spans = component_spans(path);
    let resources_index = spans
        .iter()
        .position(|(part, _, _)| part.eq_ignore_ascii_case("resources"))?;
    if resources_index == 0 {
        return None;
    }

    let (_, resources_start, _) = spans[resources_index];
    let root = path[..resources_start].trim_end_matches(is_path_separator);

    if root.is_empty() {
        None
    } else {
        Some((root, spans[resources_index - 1].0))
    }
}

fn is_resources_app_package(parts: &[&str]) -> bool {
    let [.., resources, app, package] = parts else {
        return false;
    };

    resources.eq_ignore_ascii_case("resources")
        && app.eq_ignore_ascii_case("app")
        && package.eq_ignore_ascii_case("package.json")
}

fn root_exe_name(path: &str) -> Option<(&str, &str)> {
    let spans = component_spans(path);
    let (file_name, file_start, _) = *spans.last()?;
    let stem = exe_stem(file_name)?;
    if stem.eq_ignore_ascii_case("electron") || spans.len() < 2 {
        return None;
    }

    let root = path[..file_start].trim_end_matches(is_path_separator);
    if root.is_empty() {
        None
    } else {
        Some((root, stem))
    }
}

fn parent_root_exe<'a>(
    root_key: &str,
    root_exes: &'a BTreeMap<String, (String, String)>,
) -> Option<(&'a str, &'a str)> {
    let separator = root_key.rfind(['\\', '/'])?;
    let parent = &root_key[..separator];
    let (root, name) = root_exes.get(parent)?;

    Some((root, name))
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

fn is_generic_root_name(name: &str) -> bool {
    name == "app" || name.eq_ignore_ascii_case("win-unpacked") || should_use_parent_root_name(name)
}

fn should_use_parent_root_name(name: &str) -> bool {
    name == "app"
        || name
            .strip_prefix("app-")
            .is_some_and(|suffix| suffix.chars().next().is_some_and(|ch| ch.is_ascii_digit()))
        || is_hexish_name(name)
}

fn is_hexish_name(name: &str) -> bool {
    name.len() >= 8 && name.chars().all(|character| character.is_ascii_hexdigit())
}

fn parent_root_name(path: &str) -> Option<(&str, &str)> {
    let separator = path.rfind(['\\', '/'])?;
    let parent = path[..separator].trim_end_matches(is_path_separator);
    let name = parent.rsplit(['\\', '/']).next()?;

    if parent.is_empty() || name.is_empty() {
        None
    } else {
        Some((parent, name))
    }
}

fn component_spans(path: &str) -> Vec<(&str, usize, usize)> {
    let mut spans = Vec::new();
    let mut start = None;

    for (index, character) in path.char_indices() {
        if is_path_separator(character) {
            if let Some(component_start) = start.take() {
                spans.push((&path[component_start..index], component_start, index));
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }

    if let Some(component_start) = start {
        spans.push((&path[component_start..], component_start, path.len()));
    }

    spans
}

fn is_path_separator(character: char) -> bool {
    matches!(character, '\\' | '/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_candidate_paths() {
        assert_eq!(
            candidate_kind(r"C:\Users\me\AppData\Local\Programs\Code\resources\app.asar"),
            Some(CandidateKind::AppAsar)
        );
        assert_eq!(
            candidate_kind(
                r"C:\Users\me\AppData\Local\Programs\Microsoft VS Code\4fe60c8b1c\resources\app\package.json"
            ),
            Some(CandidateKind::AppPackageJson)
        );
        assert_eq!(
            candidate_kind(r"C:\Tools\Electron\electron.exe"),
            Some(CandidateKind::ElectronExe)
        );
        assert_eq!(
            candidate_kind(r"C:\Users\me\AppData\Local\Programs\Code\resources"),
            Some(CandidateKind::ResourcesDir)
        );
        assert_eq!(candidate_kind(r"C:\Windows\notepad.exe"), None);
    }

    #[test]
    fn groups_candidates_by_app_root() {
        let apps = installed_apps_from_candidates(vec![
            r"C:\Users\me\AppData\Local\Programs\Code\resources\app.asar".to_string(),
            r"C:\Users\me\AppData\Local\Programs\Code\resources\electron.exe".to_string(),
            r"C:\Users\me\AppData\Local\Programs\Slack\resources\app.asar".to_string(),
        ]);

        assert_eq!(
            apps,
            vec![
                ElectronApp {
                    name: "Code".to_string(),
                    path: r"C:\Users\me\AppData\Local\Programs\Code".to_string(),
                    sources: vec!["ntfs".to_string()],
                },
                ElectronApp {
                    name: "Slack".to_string(),
                    path: r"C:\Users\me\AppData\Local\Programs\Slack".to_string(),
                    sources: vec!["ntfs".to_string()],
                },
            ]
        );
    }

    #[test]
    fn ignores_bare_resources_directory() {
        let apps = installed_apps_from_candidates(vec![r"C:\Random\resources".to_string()]);

        assert!(apps.is_empty());
    }

    #[test]
    fn groups_electron_exe_under_resources_by_app_root() {
        let apps = installed_apps_from_candidates(vec![
            r"C:\Users\me\AppData\Local\Programs\Cursor\resources\electron.exe".to_string(),
        ]);

        assert_eq!(
            apps,
            vec![ElectronApp {
                name: "Cursor".to_string(),
                path: r"C:\Users\me\AppData\Local\Programs\Cursor".to_string(),
                sources: vec!["ntfs".to_string()],
            }]
        );
    }

    #[test]
    fn uses_root_exe_name_for_generic_win_unpacked_root() {
        let apps = installed_apps_from_candidates(vec![
            r"D:\projects\UI\ele\dist\win-unpacked\resources\app.asar".to_string(),
            r"D:\projects\UI\ele\dist\win-unpacked\ATLaserTrackClient.exe".to_string(),
        ]);

        assert_eq!(
            apps,
            vec![ElectronApp {
                name: "ATLaserTrackClient".to_string(),
                path: r"D:\projects\UI\ele\dist\win-unpacked".to_string(),
                sources: vec!["ntfs".to_string()],
            }]
        );
    }

    #[test]
    fn detects_unpacked_vscode_layout_without_app_asar() {
        let apps = installed_apps_from_candidates(vec![
            r"C:\Users\me\AppData\Local\Programs\Microsoft VS Code\4fe60c8b1c\resources\app\package.json"
                .to_string(),
        ]);

        assert_eq!(
            apps,
            vec![ElectronApp {
                name: "Microsoft VS Code".to_string(),
                path: r"C:\Users\me\AppData\Local\Programs\Microsoft VS Code".to_string(),
                sources: vec!["ntfs".to_string()],
            }]
        );
    }

    #[test]
    fn uses_root_exe_name_for_generic_app_root() {
        let apps = installed_apps_from_candidates(vec![
            r"C:\Program Files\WindowsApps\OpenAI.Codex_1.0_x64__id\app\resources\app.asar"
                .to_string(),
            r"C:\Program Files\WindowsApps\OpenAI.Codex_1.0_x64__id\app\Codex.exe".to_string(),
        ]);

        assert_eq!(
            apps,
            vec![ElectronApp {
                name: "Codex".to_string(),
                path: r"C:\Program Files\WindowsApps\OpenAI.Codex_1.0_x64__id\app".to_string(),
                sources: vec!["ntfs".to_string()],
            }]
        );
    }

    #[test]
    fn detects_unpacked_vscode_layout_with_nested_package_candidates() {
        let apps = installed_apps_from_candidates(vec![
            r"C:\Users\me\AppData\Local\Programs\Microsoft VS Code\4fe60c8b1c\resources\app\node_modules\foo\package.json"
                .to_string(),
            r"C:\Users\me\AppData\Local\Programs\Microsoft VS Code\4fe60c8b1c\resources\app\package.json"
                .to_string(),
            r"C:\Users\me\AppData\Local\Programs\Microsoft VS Code\Code.exe".to_string(),
        ]);

        assert_eq!(
            apps,
            vec![ElectronApp {
                name: "Code".to_string(),
                path: r"C:\Users\me\AppData\Local\Programs\Microsoft VS Code".to_string(),
                sources: vec!["ntfs".to_string()],
            }]
        );
    }

    #[test]
    fn deduplicates_versioned_app_dirs_by_parent_path() {
        let apps = installed_apps_from_candidates(vec![
            r"C:\Users\me\AppData\Local\Figma\app-1.0.0\resources\app.asar".to_string(),
            r"C:\Users\me\AppData\Local\Figma\app-2.0.0\resources\app.asar".to_string(),
        ]);

        assert_eq!(
            apps,
            vec![ElectronApp {
                name: "Figma".to_string(),
                path: r"C:\Users\me\AppData\Local\Figma".to_string(),
                sources: vec!["ntfs".to_string()],
            }]
        );
    }

    #[test]
    fn preserves_absolute_and_unc_root_prefixes() {
        let apps = installed_apps_from_candidates(vec![
            "/Applications/Foo.app/Contents/resources/app.asar".to_string(),
            r"\\server\share\App\resources\app.asar".to_string(),
        ]);

        let paths: Vec<_> = apps.into_iter().map(|app| app.path).collect();

        assert_eq!(
            paths,
            ["/Applications/Foo.app/Contents", r"\\server\share\App",]
        );
    }
}
