# Contributing

<!--
    File: Contribution guide.
    Intent: Give contributors the shortest path to useful, tested changes.
-->

Thanks for helping improve `electron-detector`.

## Development Setup

Requirements:

- Windows
- Rust stable toolchain
- PowerShell

Run checks before opening a pull request:

```powershell
cargo fmt --all --check
cargo test -- --nocapture
cargo clippy --all-targets -- -D warnings
```

## Bug Reports

Good bug reports include:

- the command you ran;
- whether you ran as administrator;
- text or JSON output;
- the app path that should have been detected;
- whether the app is running or only installed.

For installed-app misses, useful paths include:

```text
<app root>\resources\app.asar
<app root>\resources\app\package.json
<app root>\resources\electron.exe
```

## Pull Requests

Keep changes small.

For detection changes:

- add a regression test with the relevant path shape;
- prefer structural signals over app-name whitelists;
- avoid full-disk indexing features unless the cache format and performance impact are clear.

Commit messages should use Conventional Commit format. Chinese descriptions are fine, for example:

```text
fix(detect): 识别 unpacked Electron 布局
```
