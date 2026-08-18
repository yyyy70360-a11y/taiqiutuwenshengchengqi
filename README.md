# 台球图文生成器 Tauri 版

这是一个基于 Tauri 2、Rust 和原生 SVG/PNG 渲染的桌面应用。当前代码已按 Windows 10 优先适配，同时保留原有 macOS 行为。应用不需要 Python、FastAPI、Playwright、系统 Chrome、localhost 服务或联网字体。

## 功能

- 6 套 SVG 模板，固定输出 `1080 x 1920` PNG。
- 内置 Noto Sans/Serif CJK 字体，不依赖系统字体。
- 单条预览、单条保存、AI 文案、批量文案和最多 100 张批量渲染。
- 批量渲染支持统一模板，或按数量分配多套模板。
- 批量进度、失败和完成事件。
- SQLite 保存预设、设置、账号、文案库和渲染历史元数据。
- API Key 使用操作系统的系统凭据存储：Windows 10 为 Windows Credential Manager，macOS 为 Keychain。SQLite 和普通配置文件只记录“已配置”状态。
- 旧版 `settings.json`、`accounts.json` 和 JSON 文案库一次性迁移，旧输出目录不移动、不删除。
- Windows 生成 NSIS 和 MSI 安装包；原有 macOS `.app` 和 `.dmg` 目标仍保留。

## 工程结构

```text
frontend/index.html          三栏工作界面与 Tauri IPC 适配
src-tauri/src/render.rs      SVG 模板、中文换行与 PNG 输出
src-tauri/src/commands.rs    Tauri commands、跨平台目录打开与批量事件
src-tauri/src/storage.rs     SQLite、系统凭据与旧数据迁移
src-tauri/src/ai.rs          OpenAI 兼容 API 与文案解析
src-tauri/fonts/             随应用打包的中文字体
scripts/windows/             Windows 10 开发环境检查与引导
```

## Windows 10 环境

推荐使用 Windows 10 64 位、Visual Studio 2022 Build Tools（C++ workload 和 Windows 10/11 SDK）、Rust MSVC 工具链、Tauri CLI 2.11.4，以及 Microsoft Edge WebView2 Runtime。

管理员 PowerShell 中执行一次：

```powershell
Set-ExecutionPolicy -Scope Process Bypass
.\scripts\windows\bootstrap-dev.ps1
```

检查环境：

```powershell
.\scripts\windows\verify-dev.ps1
```

## 检查与构建

```powershell
cargo check --manifest-path src-tauri\Cargo.toml
cargo test --manifest-path src-tauri\Cargo.toml
cargo clippy --manifest-path src-tauri\Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri\Cargo.toml renders_hundred_images_without_external_dependencies -- --ignored
cargo tauri build --bundles nsis,msi
```

Windows 构建产物位于：

```text
src-tauri\target\release\bundle\nsis\台球图文生成器_0.1.0_x64-setup.exe
src-tauri\target\release\bundle\msi\台球图文生成器_0.1.0_x64_en-US.msi
```

## 数据与迁移

Windows 10 默认数据库路径：

```text
%APPDATA%\com.billiards.matrix\billiards.sqlite3
```

API Key 使用服务名 `com.billiards.matrix`、账号 `api_key` 写入 Windows Credential Manager。应用启动和本地出图不需要读取凭据，只有状态检查、保存 API Key 和 AI 调用时才访问系统凭据存储。

首次启动会查找当前目录、可执行文件附近的旧版目录，以及 `%USERPROFILE%\Documents\台球图文生成器` 和 `%USERPROFILE%\Documents\台球矩阵搭建\workflow`。迁移完成后写入一次性标记，不会重复覆盖新设置。

## 发布注意事项

Windows 安装包默认使用当前用户安装模式，不要求管理员权限；NSIS 安装器会按配置下载 WebView2 Bootstrapper。目标机器没有 WebView2 时需要联网完成运行时安装，企业离线环境应预装 WebView2 Runtime。

未签名的 Windows 安装包适合本机测试。对外分发前应使用组织代码签名证书签名，并在一台干净的 Windows 10 机器上验证安装、首次启动、升级、卸载和数据保留。

详细验收步骤见 [`docs/WINDOWS10_RELEASE.md`](docs/WINDOWS10_RELEASE.md)。
