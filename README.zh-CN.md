# electron-detector

<!--
    File: Chinese project README.
    Intent: Explain what the CLI does, how to build it, and how to interpret its output.
-->

[English](README.md) | 简体中文

`electron-detector` 是一个 Windows 命令行工具，用来检测当前电脑上正在运行和已经安装的 Electron 应用。

它会输出两类结果：

- 正在运行的 Electron 应用：从 Windows 进程信息和 Electron 打包结构判断；
- 已安装的 Electron 应用：从本地 NTFS 扫描缓存判断。

刷新已安装应用缓存时，会直接读取 Windows NTFS 元数据。

## 状态

项目仍处于早期阶段，目前仅支持 Windows。检测逻辑优先使用明确的 Electron 文件结构信号，而不是应用名白名单，
但特殊打包方式的应用仍可能漏检。

## 安装

从源码构建：

```powershell
cargo build --release
```

运行构建后的程序：

```powershell
.\target\release\electron-detector.exe
```

## 使用

显示正在运行的应用和缓存中的已安装应用：

```powershell
electron-detector
```

在文本输出中显示完整路径：

```powershell
electron-detector --paths
```

输出 JSON：

```powershell
electron-detector --json
```

刷新已安装应用缓存：

```powershell
electron-detector --refresh
```

显示帮助或版本：

```powershell
electron-detector --help
electron-detector --version
```

`--refresh` 可能需要管理员权限，因为 NTFS MFT 和 USN Journal 访问可能被系统限制。如果命令提示需要管理员权限，
请在提权终端中运行：

```powershell
Start-Process .\target\release\electron-detector.exe -ArgumentList "--refresh" -Verb RunAs
```

## 输出

默认文本输出不显示路径：

```text
Running Electron apps: 2
- Code
- Codex

Installed Electron apps: 7
- Figma
- Code
- Obsidian
```

调试时可以使用 `--paths`：

```text
- Code (C:\Users\you\AppData\Local\Programs\Microsoft VS Code)
```

JSON 输出始终包含路径：

```json
{
    "running": [],
    "installed": [],
    "warnings": []
}
```

## 检测规则概要

检测器查找 Electron 打包结构，而不是匹配应用名称。

强安装信号包括：

- `resources\app.asar`
- `resources\app\package.json`
- `resources\electron.exe`

运行中应用的信号包括：

- 可执行文件旁边存在 `resources\app.asar`；
- 命令行包含 `--app-path=...\resources\app`；
- 命令行或路径中包含真实的 `app.asar` 路径片段；
- 进程身份符合 Electron runtime。

更多细节和已知限制见 [docs/DETECTION.zh-CN.md](docs/DETECTION.zh-CN.md)。

## 缓存

已安装应用结果缓存位置：

```text
%LOCALAPPDATA%\electron-detector\cache.json
```

安装或卸载应用后，运行 `electron-detector --refresh` 更新缓存。

## 开发

运行检查：

```powershell
cargo fmt --all --check
cargo test -- --nocapture
cargo clippy --all-targets -- -D warnings
```

## 许可证

MIT。见 [LICENSE](LICENSE)。

