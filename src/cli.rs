//! Parses command line flags for the Electron detector CLI.

/// Command line flags accepted by the Electron detector.
#[derive(Debug, Eq, PartialEq)]
pub struct CliArgs {
    /// Forces a fresh scan instead of using cached detector state.
    pub refresh: bool,
    /// Prints detector output as JSON when supported.
    pub json: bool,
    /// Includes full app paths in text output.
    pub paths: bool,
    /// Prints usage text without running detection.
    pub help: bool,
    /// Prints package version without running detection.
    pub version: bool,
}

impl CliArgs {
    /// Parses supported command line flags from an argument iterator.
    pub fn parse_from<I, S>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut cli_args = Self {
            refresh: false,
            json: false,
            paths: false,
            help: false,
            version: false,
        };

        for arg in args.into_iter().skip(1) {
            match arg.as_ref() {
                "-h" | "--help" => cli_args.help = true,
                "-V" | "--version" => cli_args.version = true,
                "--refresh" => cli_args.refresh = true,
                "--json" => cli_args.json = true,
                "--paths" => cli_args.paths = true,
                other => return Err(format!("unknown argument: {other}")),
            }
        }

        Ok(cli_args)
    }
}

#[cfg(test)]
mod tests {
    use super::CliArgs;

    #[test]
    fn parses_supported_flags() {
        let args = CliArgs::parse_from([
            "electron-detector",
            "--refresh",
            "--json",
            "--paths",
            "--help",
            "--version",
        ])
        .unwrap();

        assert!(args.refresh);
        assert!(args.json);
        assert!(args.paths);
        assert!(args.help);
        assert!(args.version);
    }

    #[test]
    fn parses_short_help_and_version_flags() {
        let args = CliArgs::parse_from(["electron-detector", "-h", "-V"]).unwrap();

        assert!(args.help);
        assert!(args.version);
    }

    #[test]
    fn rejects_unknown_flags() {
        let err = CliArgs::parse_from(["electron-detector", "--deep"]).unwrap_err();

        assert_eq!(err, "unknown argument: --deep");
    }
}
