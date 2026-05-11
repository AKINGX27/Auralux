# Auralux

[English](README.md)

Auralux 是一个轻量、本地优先的音乐播放器与音频转换项目，目标覆盖桌面端、Android、本地网页、远程网页控制台和 CLI。

项目的核心思路是：一个 Rust 核心、一套共享 GUI、多个很薄的平台壳。扫描、播放、解码、转码等重计算都在用户自己的设备上运行。网页端可以部署到云端，但只负责控制本地算力，不接收用户的音频文件。

## 当前状态

Auralux 目前是早期实现骨架，不是完整成品播放器。

本仓库已经包含：

- Rust workspace：核心库、本地 daemon、CLI。
- SQLite 音乐库 schema，并启用 FTS5 搜索。
- 本地文件夹扫描器，支持标签读取，并可用 `ffprobe` 补充格式信息。
- FFmpeg/mpv 能力检测。
- 单并发 FFmpeg 转换队列。
- 基础 mpv 播放控制抽象。
- Axum daemon，提供 REST 和 WebSocket API。
- Svelte/Vite GUI，包含响应式玻璃风格界面。
- Tauri 2 桌面/Android 壳骨架。
- Android APK 支持通过系统文件选择器导入音乐文件，声明/请求音频媒体权限，并内嵌启动本地 daemon。
- Cloudflare Worker relay 骨架，用于远程网页控制台的加密配对信令。
- CI、文档、GPL 许可证和 NOTICE 文件。

仍待完善：

- Android 原生播放/转换插件：libmpv、FFmpeg codec pack、MediaSession、音频焦点、通知栏控制和 SAF 目录授权。
- GUI 中的持久播放队列和播放列表编辑。
- 远程网页与本地 daemon 之间的端到端配对加密。
- 使用真实和 fake FFmpeg/mpv 的更完整集成测试。

## 目标

- 尽可能轻量：不使用 Electron，减少运行时层级，使用本地 SQLite，codec pack 可选。
- 格式支持以当前 FFmpeg/mpv 工具链的实际能力为准。
- 在本地网页、桌面和 Android 上尽量复用同一套 GUI。
- 音频文件始终留在本地，即使网页 UI 部署在远程。
- 提供真正可用的 CLI，覆盖音乐库管理、转换和 daemon 工作流。
- 项目保持 GPL-3.0-or-later，官方构建不启用 FFmpeg `nonfree` 组件。

## 架构

```text
apps/gui              Svelte/Vite 共享 GUI
apps/tauri            Tauri 2 桌面与 Android 壳
crates/auralux-core   SQLite、扫描、元数据、转换、播放、共享类型
crates/auraluxd       本地 REST/WebSocket daemon
crates/auralux-cli    复用同一核心的 CLI
workers/web-relay     Cloudflare Worker 静态托管与 WebSocket relay 骨架
docs                  架构、命令、Android、远程网页说明
```

运行模型：

- daemon 默认绑定 `127.0.0.1:4147`。
- GUI 通过 REST 调用 `/api`，通过 `/api/events` WebSocket 接收状态和进度事件。
- 桌面播放优先使用用户系统中的 `mpv`。
- 转换优先使用用户系统中的 `ffmpeg`。
- 格式能力会在 GUI 设置页以 capability matrix 显示。
- Cloudflare Workers 只做静态托管和加密信令中继，不处理媒体文件。

更多说明见 [docs/architecture.md](docs/architecture.md)、[docs/remote-web.md](docs/remote-web.md)、[docs/android.md](docs/android.md)。

## 环境要求

开发环境：

- Rust stable，包含 `cargo`、`rustfmt`、`clippy`。
- Node.js 20+ 和 npm。
- 可选：桌面或 Android 构建所需的 Tauri 平台依赖。
- 可选：Cloudflare Worker 开发所需的 Wrangler。

运行环境：

- `ffmpeg` 和 `ffprobe`：用于扫描增强和音频转换。
- `mpv`：用于桌面播放。

环境变量：

```bash
AURALUX_BIND=127.0.0.1:4147
AURALUX_DATA_DIR=/path/to/data
AURALUX_FFMPEG=/path/to/ffmpeg
AURALUX_FFPROBE=/path/to/ffprobe
AURALUX_MPV=/path/to/mpv
AURALUX_GUI_DIST=apps/gui/dist
```

参考 [.env.example](.env.example)。

## 快速开始

安装前端依赖：

```bash
npm install
```

运行 Rust 测试：

```bash
cargo test --workspace
```

启动本地 daemon：

```bash
cargo run -p auraluxd -- --bind 127.0.0.1:4147
```

另开一个终端，启动 GUI 开发服务器：

```bash
npm run dev
```

打开：

```text
http://127.0.0.1:5173
```

扫描音乐目录：

```bash
cargo run -p auralux-cli -- scan ~/Music
```

转换音频：

```bash
cargo run -p auralux-cli -- convert ~/Music/in.flac ~/Music/Converted --format opus
```

## CLI

CLI 二进制名为 `auralux`。

```bash
auralux serve --bind 127.0.0.1:4147
auralux scan ~/Music
auralux search "artist or title"
auralux convert ~/Music/in.flac ~/Music/Converted --format opus
auralux jobs
auralux config
```

当前子命令：

- `serve`：启动本地 daemon。
- `scan`：扫描一个或多个本地目录。
- `search`：搜索 SQLite FTS 索引。
- `play`：daemon 播放加载入口，目前是占位实现。
- `pause`：daemon 播放控制入口，目前是占位实现。
- `queue`：持久播放队列的预留入口。
- `convert`：执行前台 FFmpeg 转换。
- `jobs`：打印最近转换任务。
- `config`：打印数据库路径和 codec 能力检测结果。

见 [docs/commands.md](docs/commands.md)。

## API

daemon 的 API 挂载在 `/api`：

- `GET /api/health`
- `GET /api/events`
- `POST /api/library/scan`
- `GET /api/library/tracks`
- `GET /api/playback/state`
- `POST /api/playback/load`
- `POST /api/playback/command`
- `POST /api/conversions`
- `GET /api/jobs`
- `GET /api/jobs/:id`

扫描请求示例：

```bash
curl -X POST http://127.0.0.1:4147/api/library/scan \
  -H 'content-type: application/json' \
  -d '{"roots":["/home/me/Music"],"force":false}'
```

转换请求示例：

```bash
curl -X POST http://127.0.0.1:4147/api/conversions \
  -H 'content-type: application/json' \
  -d '{
    "source_path": "/home/me/Music/in.flac",
    "output_dir": "/home/me/Music/Converted",
    "preset": { "format": "opus", "quality": "160k" },
    "overwrite": false
  }'
```

## GUI

GUI 位于 [apps/gui](apps/gui)，当前包含：

- 音乐库搜索和曲目列表。
- 文件夹扫描入口。
- 播放栏。
- 转换任务面板。
- codec capability matrix。
- 桌面/移动响应式布局。

开发命令：

```bash
npm --workspace apps/gui run dev
npm --workspace apps/gui run check
npm --workspace apps/gui run build
```

## Tauri

Tauri 壳位于 [apps/tauri](apps/tauri)。

```bash
npm --workspace apps/tauri run dev
npm --workspace apps/tauri run build
```

Windows 桌面版可用下面的脚本构建：

```bash
npm run build:desktop:windows
```

便携版 EXE 会输出到 `release/auralux-windows-x86_64`，其中包含 `windows-register-file-associations.ps1`，可为当前 Windows 用户注册常见音频扩展名。常见音频扩展名也已经写入 Tauri bundle 配置，安装器构建会自动注册这些关联。若在 Windows 主机或带 NSIS 的环境中需要生成安装器，可运行：

```bash
AURALUX_WINDOWS_BUNDLE=1 npm run build:desktop:windows
```

Windows 壳会把通过扩展名关联打开的音频文件转发给共享 GUI；本地 daemon 运行时，这些文件会按路径导入当前 playlist。

Android 命令已预留：

```bash
npm --workspace apps/tauri run android:dev
npm --workspace apps/tauri run android:build
npm run build:android:apk
```

Android APK 会请求音频媒体权限，启动内嵌本地 daemon，并通过 Android 系统文件选择器把用户选择的音乐导入当前 playlist。完整 Android 原生后台播放和 codec pack 转换能力仍待完善，见 [docs/android.md](docs/android.md)。

当前 release CI 会发布 CLI/daemon 二进制和 web 资源包。桌面 Tauri 安装包会在平台依赖和签名配置完成后加入。

## Cloudflare Worker

Worker 骨架位于 [workers/web-relay](workers/web-relay)。它用于托管构建后的 GUI，并通过 Durable Objects 暴露 `/relay/:room` WebSocket 房间。

```bash
npm --workspace workers/web-relay run dev
npm --workspace workers/web-relay run build
npm --workspace workers/web-relay run test
```

Worker relay 只负责控制面信令。浏览器和本地 daemon 应在消息进入 Worker 之前完成端到端加密。

## 测试

推荐检查：

```bash
cargo generate-lockfile
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run check
npm run build
npm test
```

本仓库骨架是在缺少 `cargo`、`node`、`npm` 的环境中生成的。安装工具链后需要在本地重新运行上述命令。

这是应用型项目，生成 `Cargo.lock` 后应提交到仓库，以保证 release 构建可复现。

## 打包说明

- 官方构建保持 GPL-3.0-or-later。
- 官方 FFmpeg 构建不得启用 `nonfree` 组件。
- 桌面基础包优先使用系统 `ffmpeg`、`ffprobe`、`mpv`；可选 codec pack 可单独分发。
- Android 应使用 ABI split/AAB 来分发 native codec 库。
- 可选 codec pack 必须附带对应许可证、源码和构建信息。

## 许可证

Auralux 使用 GPL-3.0-or-later。见 [LICENSE](LICENSE) 和 [NOTICE](NOTICE)。
