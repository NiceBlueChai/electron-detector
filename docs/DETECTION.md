# Detection Notes

<!--
    File: English detection documentation.
    Intent: Describe the Electron detection rules and the limits of the current implementation.
-->

English | [简体中文](DETECTION.zh-CN.md)

## Goals

The detector should identify Electron apps by their runtime layout, not by a hand-maintained app-name whitelist.

The current implementation separates two questions:

- Is this an Electron app?
- What display name should we show?

Application names are used for display only. They are not the primary proof that a program is Electron.

## Installed Apps

Installed apps are found from NTFS scan candidates stored in the cache. The detector groups candidate paths into app
roots.

Recognized app layouts:

```text
<root>\resources\app.asar
<root>\resources\app\package.json
<root>\resources\electron.exe
```

Common examples:

```text
C:\Program Files\Obsidian\resources\app.asar
C:\Users\you\AppData\Local\Programs\Microsoft VS Code\<version>\resources\app\package.json
D:\project\dist\win-unpacked\resources\app.asar
```

Generic packaging directories such as `app`, `app-1.2.3`, hash-like version directories, and `win-unpacked` are cleaned
up for display when there is enough context.

## Running Apps

Running apps are detected from Windows process metadata:

- executable path;
- process name;
- command line.

The detector intentionally ignores generic tool hosts such as `cmd.exe`, `powershell.exe`, `node.exe`, and Cargo/Rust
compiler processes even if their command line mentions Electron.

Recognized running signals:

- executable sits next to `resources\app.asar`;
- command line contains `--app-path=...\resources\app`;
- command line contains a real `app.asar` component;
- process identity is an Electron runtime process.

Uninstallers next to Electron app files are ignored.

## NTFS Refresh

`--refresh` scans fixed NTFS volumes with Windows filesystem control APIs:

- `FSCTL_QUERY_USN_JOURNAL`
- `FSCTL_ENUM_USN_DATA`

The cache stores only Electron-relevant candidates. It is not a general-purpose filename index.

The cache also stores volume USN state for future freshness checks. Automatic incremental USN updates are intentionally
deferred in this version; normal runs read the cache until the user runs `--refresh`.

## Known Limits

- Non-NTFS volumes are skipped.
- Portable apps on removable or unsupported filesystems may be missed.
- Apps with custom Electron layouts may be missed until their layout is added as a structural rule.
- `--refresh` may require administrator permission.
- Text output hides paths unless `--paths` is used.
