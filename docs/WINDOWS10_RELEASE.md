# Windows 10 构建与验收

## 构建前提

- Windows 10 64 位，已安装最新可用的系统更新。
- Visual Studio 2022 Build Tools 的 C++ workload 和 Windows SDK。
- Rust stable MSVC 工具链与 Tauri CLI 2.11.4。
- Microsoft Edge WebView2 Runtime；安装器默认使用 Bootstrapper，在缺少运行时时需要联网安装。

## 本地构建

```powershell
cargo check --manifest-path src-tauri\Cargo.toml
cargo test --manifest-path src-tauri\Cargo.toml
cargo clippy --manifest-path src-tauri\Cargo.toml --all-targets -- -D warnings
cargo check --manifest-path server\Cargo.toml
cargo test --manifest-path server\Cargo.toml
cargo clippy --manifest-path server\Cargo.toml --all-targets -- -D warnings
cargo tauri build --bundles nsis,msi
# 或使用 npm 脚本：
npm run build:windows
```

当前产物（0.1.2）：

```text
src-tauri\target\release\bundle\nsis\台球图文生成器_0.1.2_x64-setup.exe
src-tauri\target\release\bundle\msi\台球图文生成器_0.1.2_x64_zh-CN.msi
```

## 验收清单

- 全新用户目录启动后创建 `%APPDATA%\com.billiards.matrix\billiards.sqlite3` 和默认预设。
- 预览、单条保存、批量文案和 100 张离线批量渲染均完成，文件名不覆盖。
- Win 客户端与 macOS 客户端使用同一云服务地址、同一 AI 网关和同一 `/admin` 后台管理面板。
- 云服务登录、退出、上传本机数据、下载云端数据均可用；云端不可用时本地预览和出图仍可用。
- 点击“打开文件夹”后由 Windows Explorer 打开输出目录。
- 云服务 access/refresh token 保存在 Windows Credential Manager；SQLite、普通配置文件和应用日志均不包含明文 token 或服务端 AI Key。
- 从旧版 `settings.json`、`accounts.json` 和 JSON 文案库启动，数据迁移完成且旧输出图片未移动或删除。
- 在没有开发工具的干净 Windows 10 机器上安装 NSIS 或 MSI，首次启动、同路径升级、卸载均成功。
- 安装后桌面、开始菜单和任务栏图标均显示 `src-tauri\icons\icon.ico` 对应的台球图标；如系统缓存旧图标，运行 `.\scripts\windows\fix-shortcut-icons.ps1` 后重新验收。
- 卸载应用后用户数据库、输出图片和系统凭据仍保留，除非用户明确要求清理数据。

## 当前支线验收证据

- 支线：`codex/registration-splash-ui`
- 发布源码提交：`ffb009b`（后续仅有证据文档提交）
- NSIS SHA256：`6B096D32828987618F2160A04DC9DF5F088FD5A943AB7FECEE290E84610EDD95`
- MSI SHA256：`12AEBB80BA7E15BA3243EEE8FC2D73439A574A892C6B0D57526E926CC4380AA3`
- 开发版 UI 自检已通过：6 组菜单、50 模板、31 语气、9:16 画布、中央区域 hidden、左右区域 auto。
- 当前机器进程审计已通过：应用只保留 WebView2 与内置 `ssh.exe`，无 CMD 子进程。
- 最新 release 关闭窗口回归已通过：`ssh.exe` 从 1 个回收到 0 个。
- 安装后可运行 `powershell -ExecutionPolicy Bypass -File .\scripts\windows\verify-release.ps1` 做只读发布检查。
- 隧道可用性可运行 `powershell -ExecutionPolicy Bypass -File .\scripts\windows\verify-tunnel.ps1`，脚本会验证受限 SSH 转发、服务端健康响应和进程回收。
- 干净 Windows 10 安装、同路径升级、卸载仍需在独立测试机执行并记录结果。

## 发布

未签名安装包只用于本机测试。对外发布前应使用组织代码签名证书签名，并在干净的 Windows 10 环境完成安装和升级回归。企业离线环境应预装 WebView2 Runtime，或改用包含运行时的离线安装策略。
