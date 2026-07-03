# 检测说明

<!--
    File: Chinese detection documentation.
    Intent: Describe the Electron detection rules and the limits of the current implementation.
-->

[English](DETECTION.md) | 简体中文

## 目标

检测器应该通过运行时结构识别 Electron 应用，而不是维护应用名称白名单。

当前实现把两个问题分开处理：

- 这个程序是不是 Electron 应用？
- 应该显示什么名称？

应用名称只用于展示，不作为判断 Electron 应用的主要证据。

## 已安装应用

已安装应用来自缓存中的 NTFS 扫描候选路径。检测器会把候选路径归并到应用根目录。

已识别的应用结构：

```text
<root>\resources\app.asar
<root>\resources\app\package.json
<root>\resources\electron.exe
```

常见示例：

```text
C:\Program Files\Obsidian\resources\app.asar
C:\Users\you\AppData\Local\Programs\Microsoft VS Code\<version>\resources\app\package.json
D:\project\dist\win-unpacked\resources\app.asar
```

展示名称会在上下文足够时清理通用打包目录，例如 `app`、`app-1.2.3`、哈希版本目录和 `win-unpacked`。

## 运行中应用

运行中应用通过 Windows 进程信息检测：

- 可执行文件路径；
- 进程名称；
- 命令行。

即使命令行提到 Electron，检测器也会忽略 `cmd.exe`、`powershell.exe`、`node.exe`、Cargo/Rust 编译器这类通用工具宿主。

已识别的运行中信号：

- 可执行文件旁边存在 `resources\app.asar`；
- 命令行包含 `--app-path=...\resources\app`；
- 命令行包含真实的 `app.asar` 路径片段；
- 进程身份符合 Electron runtime。

Electron 应用文件旁边的卸载程序会被忽略。

## NTFS 刷新

`--refresh` 会通过 Windows 文件系统控制 API 扫描固定 NTFS 卷：

- `FSCTL_QUERY_USN_JOURNAL`
- `FSCTL_ENUM_USN_DATA`

缓存只保存和 Electron 检测相关的候选路径，不是通用文件名索引。

缓存也保存卷的 USN 状态，供后续新鲜度检查使用。自动增量 USN 更新在当前版本中暂缓；普通运行会读取缓存，
直到用户再次执行 `--refresh`。

## 已知限制

- 非 NTFS 卷会被跳过。
- 可移动磁盘或不支持文件系统上的便携应用可能漏检。
- 自定义 Electron 打包结构可能漏检，直到对应结构规则被加入。
- `--refresh` 可能需要管理员权限。
- 文本输出默认隐藏路径，除非使用 `--paths`。

