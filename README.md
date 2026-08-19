# 台球图文生成器 Tauri 版

这是一个基于 Tauri 2、Rust、原生 SVG/PNG 渲染和共享云端 API 的桌面应用。Windows 和 macOS 客户端共用同一套云服务、AI 网关和后台管理面板；客户端本地出图不依赖 Python、FastAPI、Playwright、系统 Chrome 或联网字体。

## 功能

- 50 套不同风格模板，固定输出 `1080 x 1920` PNG。
- 内置中文字体，支持标题和正文字体、粗细、颜色等自定义选项。
- 单条生成、批量生成、随机模板和自定义模板数量分配。
- 30 种语气，单条和批量模式均支持随机语气；实际语气约束会写入发送给 AI 的提示词。
- 文案容量前端提示、服务端校验和渲染自适应，减少文案超出模板承载范围的问题。
- SQLite 保存预设、设置、账号、文案库和渲染历史。
- 云服务登录会话使用 Windows Credential Manager 或 macOS Keychain；AI Provider Key 只保存在服务端。
- Windows 生成 NSIS 和 MSI 安装包，保留 macOS `.app` 和 `.dmg` 构建能力。

## 工程结构

```text
frontend/index.html          工作界面、交互和 Tauri IPC 适配
src-tauri/src/render.rs      模板、中文换行、自适应排版和 PNG 输出
src-tauri/src/commands.rs    Tauri commands、云同步和批量事件
src-tauri/src/storage.rs     SQLite、系统凭据和旧数据迁移
src-tauri/src/cloud.rs       共享云服务客户端、AI 网关和同步
src-tauri/src/tunnel.rs      Windows 启动时静默建立云服务 SSH 隧道
src-tauri/fonts/             随应用打包的中文字体
server/                      Mac 与 Windows 共用的云端 API 和管理后台
scripts/windows/             Windows 10 环境检查和发布脚本
```

## Windows 10 环境

推荐使用 Windows 10 64 位、Visual Studio 2022 Build Tools（C++ workload 和 Windows SDK）、Rust MSVC 工具链、Tauri CLI 2，以及 Microsoft Edge WebView2 Runtime。

```powershell
Set-ExecutionPolicy -Scope Process Bypass
.\scripts\windows\bootstrap-dev.ps1
.\scripts\windows\verify-dev.ps1
```

## 检查与构建

```powershell
cargo check --manifest-path src-tauri\Cargo.toml
cargo test --manifest-path src-tauri\Cargo.toml
cargo clippy --manifest-path src-tauri\Cargo.toml --all-targets -- -D warnings
cargo tauri build --bundles nsis,msi
```

Windows 构建产物位于：

```text
src-tauri\target\release\bundle\nsis\台球图文生成器_0.1.1_x64-setup.exe
src-tauri\target\release\bundle\msi\台球图文生成器_0.1.1_x64_zh-CN.msi
```

## 云服务隧道

Windows 客户端默认访问 `http://127.0.0.1:38123`。应用启动时会先做健康检查；不可用时静默启动 SSH 本地转发，再次等待云服务就绪。

安装包内包含 Windows OpenSSH 客户端，以及 `billiards-tunnel` 受限账号的专用密钥，因此目标电脑不需要额外安装 SSH。服务器端通过 `permitopen="127.0.0.1:38123"` 限制密钥用途，不能使用 root 账号，也不能获得交互式 shell。私钥和 OpenSSH 可执行文件在 `.gitignore` 中排除，不上传到 GitHub；构建发布包前必须通过安全渠道放到：

```text
src-tauri\resources\tunnel\billiards_tunnel_ed25519
src-tauri\resources\tunnel\ssh.exe
```

## 数据与迁移

Windows 默认数据库路径：

```text
%APPDATA%\com.billiards.matrix\billiards.sqlite3
```

应用首次启动会查找旧版数据目录并执行一次性迁移。旧输出目录不会移动或删除。

## 发布注意事项

Windows 有时会缓存旧版快捷方式或任务栏图标。安装新版后如仍显示旧图标，可运行：

```powershell
.\scripts\windows\fix-shortcut-icons.ps1
```

未签名安装包适合测试。正式分发前应使用代码签名证书，并在干净的 Windows 10 机器上验证安装、首次启动、升级、卸载、云服务连接和数据保留。详细验收步骤见 [`docs/WINDOWS10_RELEASE.md`](docs/WINDOWS10_RELEASE.md)。
