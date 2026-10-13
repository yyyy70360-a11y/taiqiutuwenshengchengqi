# Windows 安装与构建

适用于 Windows 10/11 x64，版本 0.1.4，包含本地直连 AI（URL、模型、API Key）和云端服务两种模式。

## 使用

1. 运行 `台球图文生成器_0.1.4_x64-setup.exe`，按提示完成安装。
2. 打开应用，进入设置，选择本地直连，填写兼容 OpenAI Chat Completions 的 URL、模型和 API Key。
3. 测试连接并保存，即可生成文案和海报。
4. 在单条或批量生成页的题材中选择「比赛找搭子」分组，修改比赛条件，再生成提示词与文案。内置 12 个角度，默认示例为限1–3档、免报名费、一天打完。
5. 配色默认跟随模板，修改颜色会切到自定义。0.1.4 修复话题越界、浅色文字对比、表情缺字和字体字重，详见 [渲染修复说明](RENDER_FIXES.md)。

安装程序在当前用户目录安装，无需为应用安装申请管理员权限。缺少 WebView2 时，安装程序会联网下载并安装运行时。已安装 WebView2 后，本地海报渲染不需要联网；AI 请求需要能访问配置的接口。

API Key 和云端会话保存在 Windows 凭据管理器。图片、设置和 SQLite 数据保存在应用的数据目录或用户选择的输出目录。安装包未做 Windows 代码签名。

下载包同时包含 `billiards_matrix.exe` 和 `SHA256SUMS.txt`。推荐使用安装程序；直接运行应用 EXE 需要预先安装 WebView2。

## Windows 本机构建

安装 Rust stable MSVC、Visual Studio C++ Build Tools、Windows SDK 和 Tauri CLI 2.12.2。项目提供 `scripts/windows/bootstrap-dev.ps1` 与 `verify-dev.ps1`。

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo tauri build --ci -- --locked
```

Windows 平台自动合并 `src-tauri/tauri.windows.conf.json`，生成中文 NSIS 安装程序：

```text
src-tauri/target/release/bundle/nsis/台球图文生成器_0.1.4_x64-setup.exe
```

GitHub Actions 工作流 `Windows EXE` 在 Windows 上执行客户端测试、Clippy、release 构建，上传安装程序、应用 EXE、使用说明与 SHA-256。下载构建记录里的 `billiards-windows-x64` 文件即可。构建与自动测试不替代 Windows 桌面的安装、启动与交互验收。
