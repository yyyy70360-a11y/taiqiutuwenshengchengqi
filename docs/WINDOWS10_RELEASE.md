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
cargo tauri build --bundles nsis,msi
```

产物：

```text
src-tauri\target\release\bundle\nsis\台球图文生成器_0.1.0_x64-setup.exe
src-tauri\target\release\bundle\msi\台球图文生成器_0.1.0_x64_en-US.msi
```

## 验收清单

- 全新用户目录启动后创建 `%APPDATA%\com.billiards.matrix\billiards.sqlite3` 和默认预设。
- 预览、单条保存、批量文案和 100 张离线批量渲染均完成，文件名不覆盖。
- 点击“打开文件夹”后由 Windows Explorer 打开输出目录。
- 保存 API Key 后，SQLite、普通配置文件和应用日志均不包含明文 Key；Windows Credential Manager 中存在服务 `com.billiards.matrix`、账号 `api_key`。
- 从旧版 `settings.json`、`accounts.json` 和 JSON 文案库启动，数据迁移完成且旧输出图片未移动或删除。
- 在没有开发工具的干净 Windows 10 机器上安装 NSIS 或 MSI，首次启动、同路径升级、卸载均成功。
- 卸载应用后用户数据库、输出图片和系统凭据仍保留，除非用户明确要求清理数据。

## 发布

未签名安装包只用于本机测试。对外发布前应使用组织代码签名证书签名，并在干净的 Windows 10 环境完成安装和升级回归。企业离线环境应预装 WebView2 Runtime，或改用包含运行时的离线安装策略。
