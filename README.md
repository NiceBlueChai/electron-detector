# electron-detector

<!--
    File: English project README.
    Intent: Explain what the CLI does, how to build it, and how to interpret its output.
-->

English | [简体中文](README.zh-CN.md)

`electron-detector` is a Windows CLI that detects Electron applications on the current machine.

It reports two groups:

- running Electron apps, detected from Windows process metadata and Electron layout signals;
- installed Electron apps, detected from a cached NTFS scan.

Installed-app refresh uses Windows NTFS metadata directly.

## Status

This project is early-stage Windows-only software. The detector favors concrete Electron layout signals over app-name
whitelists, but it can still miss unusual packaging layouts.

## Install

Build from source:

```powershell
cargo build --release
```

Run the release binary:

```powershell
.\target\release\electron-detector.exe
```

## Usage

Show running apps and cached installed apps:

```powershell
electron-detector
```

Show full paths in text output:

```powershell
electron-detector --paths
```

Output JSON:

```powershell
electron-detector --json
```

Refresh the installed-app cache:

```powershell
electron-detector --refresh
```

Print help or version:

```powershell
electron-detector --help
electron-detector --version
```

`--refresh` may require administrator permission because NTFS MFT and USN Journal access can be restricted. If the
command prints an administrator-permission error, run it from an elevated terminal:

```powershell
Start-Process .\target\release\electron-detector.exe -ArgumentList "--refresh" -Verb RunAs
```

## Output

Default text output hides paths:

```text
Running Electron apps: 2
- Code
- Codex

Installed Electron apps: 7
- Figma
- Code
- Obsidian
```

Use `--paths` when debugging:

```text
- Code (C:\Users\you\AppData\Local\Programs\Microsoft VS Code)
```

JSON output always includes paths:

```json
{
    "running": [],
    "installed": [],
    "warnings": []
}
```

## Detection Summary

The detector looks for Electron packaging layouts, not app names.

Strong installed-app signals:

- `resources\app.asar`
- `resources\app\package.json`
- `resources\electron.exe`

Running-app signals include:

- an executable next to `resources\app.asar`;
- command lines with `--app-path=...\resources\app`;
- command lines or paths with a real `app.asar` component;
- Electron runtime process identity.

See [docs/DETECTION.md](docs/DETECTION.md) for details and known limits.

## Cache

Installed-app results are cached at:

```text
%LOCALAPPDATA%\electron-detector\cache.json
```

Run `electron-detector --refresh` after installing or removing apps.

## Development

Run checks:

```powershell
cargo fmt --all --check
cargo test -- --nocapture
cargo clippy --all-targets -- -D warnings
```

## License

MIT. See [LICENSE](LICENSE).

