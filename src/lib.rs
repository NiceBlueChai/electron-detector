//! Provides the library entry points for the Electron detector.

pub mod cache;
pub mod cli;
pub mod detect;
pub mod ntfs;
pub mod process;

use cli::CliArgs;
use detect::{installed_apps_from_candidates, ElectronApp};

/// @brief Full detector report printed by the CLI.
#[derive(Debug, serde::Serialize)]
pub struct Report {
    /// @brief Electron apps detected from currently running processes.
    pub running: Vec<ElectronApp>,
    /// @brief Electron apps loaded from the installed app cache.
    pub installed: Vec<ElectronApp>,
    /// @brief Non-fatal detector failures surfaced to users.
    pub warnings: Vec<String>,
}

/// @brief Runs the Electron detector command with parsed CLI arguments.
pub fn run<I, S>(args: I) -> Result<String, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args = CliArgs::parse_from(args)?;
    if args.help {
        return Ok(help_text());
    }
    if args.version {
        return Ok(format!("electron-detector {}\n", env!("CARGO_PKG_VERSION")));
    }

    let mut warnings = Vec::new();

    let running = process::running_apps().unwrap_or_else(|err| {
        warnings.push(err);
        Vec::new()
    });

    let installed = if args.refresh {
        let refreshed_cache = ntfs::refresh_cache()?;
        let path = cache::default_cache_path()?;
        cache::save_cache(&path, &refreshed_cache)?;
        refreshed_cache.installed_apps
    } else {
        match cache::default_cache_path().and_then(|path| cache::load_cache(&path)) {
            Ok(cache) => installed_apps_from_candidates(cache.candidates),
            Err(err) => {
                warnings.push(format!("{err}; run electron-detector --refresh"));
                Vec::new()
            }
        }
    };

    let report = Report {
        running,
        installed,
        warnings,
    };

    if args.json {
        return serde_json::to_string_pretty(&report)
            .map_err(|err| format!("failed to encode report JSON: {err}"));
    }

    Ok(format_text(&report, args.paths))
}

fn help_text() -> String {
    r#"electron-detector

Usage:
    electron-detector [OPTIONS]

Options:
    --refresh       Refresh the installed app cache
    --json          Print JSON output
    --paths         Show full paths in text output
    -h, --help      Print help
    -V, --version   Print version
"#
    .to_string()
}

fn format_text(report: &Report, show_paths: bool) -> String {
    let mut output = format!("Running Electron apps: {}\n", report.running.len());
    for app in &report.running {
        output.push_str(&format_app_line(app, show_paths));
    }

    output.push('\n');
    output.push_str(&format!(
        "Installed Electron apps: {}\n",
        report.installed.len()
    ));
    for app in &report.installed {
        output.push_str(&format_app_line(app, show_paths));
    }

    for warning in &report.warnings {
        output.push_str(&format!("warning: {warning}\n"));
    }

    output
}

fn format_app_line(app: &ElectronApp, show_paths: bool) -> String {
    if show_paths {
        format!("- {} ({})\n", app.name, app.path)
    } else {
        format!("- {}\n", app.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_report() -> Report {
        Report {
            running: vec![ElectronApp {
                name: "Code".to_string(),
                path: r"C:\Users\me\AppData\Local\Programs\Microsoft VS Code\Code.exe".to_string(),
                sources: vec!["running".to_string()],
            }],
            installed: vec![ElectronApp {
                name: "Program".to_string(),
                path: r"C:\Program Files (x86)\Thunder Network\Thunder\Program".to_string(),
                sources: vec!["ntfs".to_string()],
            }],
            warnings: Vec::new(),
        }
    }

    #[test]
    fn text_output_hides_app_paths_by_default() {
        let report = sample_report();

        let output = format_text(&report, false);

        assert!(output.contains("- Code\n"));
        assert!(output.contains("- Program\n"));
        assert!(!output.contains(r"C:\Users\me\AppData\Local\Programs\Microsoft VS Code"));
        assert!(!output.contains(r"C:\Program Files (x86)\Thunder Network\Thunder\Program"));
    }

    #[test]
    fn text_output_includes_app_paths_when_requested() {
        let report = sample_report();

        let output = format_text(&report, true);

        assert!(output
            .contains(r"- Code (C:\Users\me\AppData\Local\Programs\Microsoft VS Code\Code.exe)"));
        assert!(
            output.contains(r"- Program (C:\Program Files (x86)\Thunder Network\Thunder\Program)")
        );
    }

    #[test]
    fn help_output_lists_supported_options() {
        let output = run(["electron-detector", "--help"]).unwrap();

        assert!(output.contains("-h, --help"));
        assert!(output.contains("-V, --version"));
        assert!(output.contains("--refresh"));
    }

    #[test]
    fn version_output_uses_package_version() {
        let output = run(["electron-detector", "-V"]).unwrap();

        assert_eq!(
            output,
            format!("electron-detector {}\n", env!("CARGO_PKG_VERSION"))
        );
    }
}
