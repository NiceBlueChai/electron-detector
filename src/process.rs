//! Detects running Electron apps from Windows process metadata.

use crate::detect::ElectronApp;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

/// @brief Windows process metadata used to detect running Electron apps.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct ProcessInfo {
    /// @brief Process image name reported by Windows.
    #[serde(rename = "Name")]
    pub name: String,
    /// @brief Full executable path when Windows exposes it.
    #[serde(
        rename = "ExecutablePath",
        default,
        deserialize_with = "empty_string_if_null"
    )]
    pub executable_path: String,
    /// @brief Full command line when Windows exposes it.
    #[serde(
        rename = "CommandLine",
        default,
        deserialize_with = "empty_string_if_null"
    )]
    pub command_line: String,
}

/// @brief Queries Windows for running processes and returns detected Electron apps.
pub fn running_apps() -> Result<Vec<ElectronApp>, String> {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); \
             $OutputEncoding = [Console]::OutputEncoding; \
             Get-CimInstance Win32_Process | Select-Object Name,ExecutablePath,CommandLine | ConvertTo-Json",
        ])
        .output()
        .map_err(|err| format!("failed to spawn PowerShell process query: {err}"))?;

    if !output.status.success() {
        return Err(format_powershell_query_error(
            &output.status.to_string(),
            &output.stderr,
            &output.stdout,
        ));
    }

    let processes = processes_from_powershell_stdout(&output.stdout)?;

    Ok(apps_from_processes(processes))
}

fn processes_from_powershell_stdout(stdout: &[u8]) -> Result<Vec<ProcessInfo>, String> {
    let text = String::from_utf8_lossy(stdout);
    let processes = serde_json::from_str::<Vec<ProcessInfo>>(&text)
        .or_else(|_| serde_json::from_str::<ProcessInfo>(&text).map(|process| vec![process]))
        .map_err(|err| format!("failed to parse PowerShell process query JSON: {err}"))?;

    Ok(processes)
}

/// @brief Detects Electron apps from process metadata without querying the OS.
pub fn apps_from_processes(processes: Vec<ProcessInfo>) -> Vec<ElectronApp> {
    let mut apps = BTreeMap::new();

    for process in processes {
        if !is_electron_process(&process) {
            continue;
        }

        let name = display_name(&process);
        if name.is_empty() {
            continue;
        }

        apps.entry(dedupe_key(&process)).or_insert(ElectronApp {
            name,
            path: process.executable_path,
            sources: vec!["running".to_string()],
        });
    }

    apps.into_values().collect()
}

fn is_electron_process(process: &ProcessInfo) -> bool {
    let process_name = process.name.to_ascii_lowercase();
    if is_ignored_host_process(&process_name) {
        return false;
    }
    if file_stem(&process.executable_path)
        .or_else(|| file_stem(&process.name))
        .is_some_and(|stem| stem.to_ascii_lowercase().starts_with("uninstall"))
    {
        return false;
    }

    has_electron_runtime_identity(process)
        || has_adjacent_app_asar(&process.executable_path)
        || contains_app_asar_component(&process.command_line)
        || contains_app_path_argument(&process.command_line)
        || process.command_line.contains("prod=Electron")
}

fn is_ignored_host_process(process_name: &str) -> bool {
    matches!(
        process_name,
        "cmd.exe"
            | "powershell.exe"
            | "pwsh.exe"
            | "node.exe"
            | "cargo.exe"
            | "rustc.exe"
            | "conhost.exe"
    )
}

fn dedupe_key(process: &ProcessInfo) -> String {
    let path = process.executable_path.trim();
    if path.is_empty() {
        display_name(process).to_ascii_lowercase()
    } else {
        path.to_ascii_lowercase()
    }
}

fn has_electron_runtime_identity(process: &ProcessInfo) -> bool {
    [&process.name, &process.executable_path]
        .into_iter()
        .flat_map(|text| text.split(['\\', '/', '"', '\'', ' ', '\t']))
        .any(|component| {
            component.eq_ignore_ascii_case("electron")
                || component.eq_ignore_ascii_case("electron.exe")
        })
}

fn has_adjacent_app_asar(executable_path: &str) -> bool {
    let Some(root) = Path::new(executable_path).parent() else {
        return false;
    };

    root.join("resources").join("app.asar").is_file()
}

fn contains_app_asar_component(text: &str) -> bool {
    text.split(['\\', '/', '"', '\'', ' ', '\t'])
        .any(|component| component.eq_ignore_ascii_case("app.asar"))
}

fn contains_app_path_argument(text: &str) -> bool {
    text.to_ascii_lowercase().contains("--app-path")
        && text
            .split(['\\', '/', '"', '\'', ' ', '\t', '='])
            .collect::<Vec<_>>()
            .windows(2)
            .any(|window| {
                window[0].eq_ignore_ascii_case("resources") && window[1].eq_ignore_ascii_case("app")
            })
}

fn format_powershell_query_error(status: &str, stderr: &[u8], stdout: &[u8]) -> String {
    let stderr_text = String::from_utf8_lossy(stderr);
    let stdout_text = String::from_utf8_lossy(stdout);
    let detail = stderr_text.trim();
    let detail = if detail.is_empty() {
        stdout_text.trim()
    } else {
        detail
    };

    if detail.is_empty() {
        format!("PowerShell process query failed ({status})")
    } else {
        format!("PowerShell process query failed ({status}): {detail}")
    }
}

fn display_name(process: &ProcessInfo) -> String {
    file_stem(&process.executable_path)
        .or_else(|| file_stem(&process.name))
        .unwrap_or_else(|| process.name.trim())
        .to_string()
}

fn file_stem(path: &str) -> Option<&str> {
    let file_name = path
        .rsplit(['\\', '/'])
        .next()
        .filter(|file_name| !file_name.is_empty())?;

    Some(
        file_name
            .rsplit_once('.')
            .map_or(file_name, |(stem, _)| stem),
    )
}

fn empty_string_if_null<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_running_electron_apps_from_process_metadata() {
        let apps = apps_from_processes(vec![
            ProcessInfo {
                name: "AnyName.exe".to_string(),
                executable_path: r"C:\Users\me\AppData\Local\Programs\AnyName\AnyName.exe"
                    .to_string(),
                command_line: r#""AnyName.exe" --type=renderer C:\AnyName\resources\app.asar"#
                    .to_string(),
            },
            ProcessInfo {
                name: "notepad.exe".to_string(),
                executable_path: r"C:\Windows\notepad.exe".to_string(),
                command_line: "notepad.exe".to_string(),
            },
        ]);

        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].name, "AnyName");
        assert_eq!(apps[0].sources, ["running"]);
    }

    #[test]
    fn detects_running_electron_app_from_adjacent_app_asar() {
        let root = std::env::temp_dir().join(format!(
            "electron-detector-process-test-{}",
            std::process::id()
        ));
        let resources = root.join("resources");
        std::fs::create_dir_all(&resources).unwrap();
        std::fs::write(resources.join("app.asar"), b"").unwrap();
        let executable_path = root.join("CustomProduct.exe").to_string_lossy().to_string();

        let apps = apps_from_processes(vec![ProcessInfo {
            name: "CustomProduct.exe".to_string(),
            executable_path,
            command_line: "CustomProduct.exe".to_string(),
        }]);
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].name, "CustomProduct");
    }

    #[test]
    fn detects_running_unpacked_electron_app_from_app_path() {
        let apps = apps_from_processes(vec![ProcessInfo {
            name: "Anything.exe".to_string(),
            executable_path: r"C:\Users\me\AppData\Local\Programs\Anything\Anything.exe"
                .to_string(),
            command_line:
                r#""Anything.exe" --app-path="C:\Users\me\AppData\Local\Programs\Anything\resources\app""#
                    .to_string(),
        }]);

        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].name, "Anything");
    }

    #[test]
    fn ignores_non_electron_process_paths_under_resources_app() {
        let apps = apps_from_processes(vec![ProcessInfo {
            name: "BackgroundDownload.exe".to_string(),
            executable_path: r"C:\Program Files (x86)\Microsoft Visual Studio\Installer\resources\app\ServiceHub\Services\Microsoft.VisualStudio.Setup.Service\BackgroundDownload.exe"
                .to_string(),
            command_line: r#""C:\Program Files (x86)\Microsoft Visual Studio\Installer\resources\app\ServiceHub\Services\Microsoft.VisualStudio.Setup.Service\BackgroundDownload.exe""#
                .to_string(),
        }]);

        assert!(apps.is_empty());
    }

    #[test]
    fn ignores_uninstallers_next_to_electron_app_files() {
        let root = std::env::temp_dir().join(format!(
            "electron-detector-uninstaller-test-{}",
            std::process::id()
        ));
        let resources = root.join("resources");
        std::fs::create_dir_all(&resources).unwrap();
        std::fs::write(resources.join("app.asar"), b"").unwrap();
        let executable_path = root
            .join("Uninstall Foxglove.exe")
            .to_string_lossy()
            .to_string();

        let apps = apps_from_processes(vec![ProcessInfo {
            name: "Uninstall Foxglove.exe".to_string(),
            executable_path,
            command_line: "Uninstall Foxglove.exe".to_string(),
        }]);
        let _ = std::fs::remove_dir_all(&root);

        assert!(apps.is_empty());
    }

    #[test]
    fn ignores_project_paths_that_only_contain_electron_in_their_name() {
        let apps = apps_from_processes(vec![ProcessInfo {
            name: "electron-detector.exe".to_string(),
            executable_path:
                r"C:\Users\me\repo\electron-detector\target\debug\electron-detector.exe".to_string(),
            command_line: r"C:\Users\me\repo\electron-detector\target\debug\electron-detector.exe"
                .to_string(),
        }]);

        assert!(apps.is_empty());
    }

    #[test]
    fn ignores_shell_and_tool_hosts_that_mention_electron() {
        let apps = apps_from_processes(vec![
            ProcessInfo {
                name: "cmd.exe".to_string(),
                executable_path: r"C:\Windows\System32\cmd.exe".to_string(),
                command_line: r"cmd.exe /c electron .".to_string(),
            },
            ProcessInfo {
                name: "node.exe".to_string(),
                executable_path: r"C:\Program Files\nodejs\node.exe".to_string(),
                command_line: r"node.exe C:\tools\electron\cli.js C:\App\resources\app.asar"
                    .to_string(),
            },
        ]);

        assert!(apps.is_empty());
    }

    #[test]
    fn deduplicates_running_apps_by_executable_path_when_available() {
        let apps = apps_from_processes(vec![
            ProcessInfo {
                name: "electron.exe".to_string(),
                executable_path: r"C:\Apps\Alpha\electron.exe".to_string(),
                command_line: r"C:\Apps\Alpha\electron.exe".to_string(),
            },
            ProcessInfo {
                name: "electron.exe".to_string(),
                executable_path: r"C:\Apps\Beta\electron.exe".to_string(),
                command_line: r"C:\Apps\Beta\electron.exe".to_string(),
            },
        ]);

        assert_eq!(apps.len(), 2);
    }

    #[test]
    fn formats_failed_powershell_query_with_status_and_output() {
        assert_eq!(
            format_powershell_query_error("exit code: 1", b" access denied \r\n", b"ignored"),
            "PowerShell process query failed (exit code: 1): access denied"
        );
        assert_eq!(
            format_powershell_query_error("exit code: 2", b" \r\n", b" fallback \n"),
            "PowerShell process query failed (exit code: 2): fallback"
        );
    }

    #[test]
    fn parses_process_json_with_non_utf8_bytes_in_values() {
        let mut stdout =
            br#"[{"Name":"app.exe","ExecutablePath":"C:\\App\\app.exe","CommandLine":"bad "#
                .to_vec();
        stdout.push(0xFF);
        stdout.extend_from_slice(br#""}]"#);

        let processes = processes_from_powershell_stdout(&stdout).unwrap();

        assert_eq!(processes.len(), 1);
        assert_eq!(processes[0].name, "app.exe");
    }
}
