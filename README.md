# 台球图文生成器 Tauri 重写版

这是 `workflow/app.py` 与 `workflow/index.html` 的 macOS 优先重写。正式应用使用 Tauri 2 和 Rust，不需要 Python、FastAPI、Playwright、系统 Chrome、localhost 服务或联网字体。

## 已实现

- 6 套 SVG 模板输出固定 `1080 x 1920` PNG。
- `resvg + usvg + tiny-skia + fontdb` 原生渲染。
- 内置 Noto Sans/Serif CJK 字体，不读取系统字体。
- 单条预览、单条保存、AI 文案、批量文案和最多 100 张批量渲染。
- 批量进度、失败和完成事件。
- SQLite 预设、设置、账号、文案库和渲染历史元数据。
- macOS Keychain 保存 API Key，界面和数据库只记录“已配置”状态。
- 旧版 `settings.json`、`accounts.json` 和可用的 JSON 文案库一次性迁移。
- 旧输出目录原样保留；不导入旧版 base64 预览历史。
- `.app` 与 `.dmg` 打包。

## 工程结构

```text
frontend/index.html          三栏工作界面与 Tauri IPC 适配
src-tauri/src/render.rs      SVG 模板、中文换行与 PNG 输出
src-tauri/src/commands.rs    Tauri commands、文件与批量事件
src-tauri/src/storage.rs     SQLite、Keychain 与旧数据迁移
src-tauri/src/ai.rs          OpenAI 兼容 API 与文案解析
src-tauri/fonts/             随应用打包的中文字体
```

## 环境基线

- macOS 15.7.5
- Xcode 16.4：`/Applications/Xcode.app`
- Apple SDK 15.5
- Rust stable 1.97.1，目标 `x86_64-apple-darwin`
- Tauri CLI 2.11.4

```bash
rustc --version --verbose
cargo --version
rustup show active-toolchain
cargo tauri --version
xcode-select -p
xcodebuild -version
```

## 检查与构建

```bash
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml renders_hundred_images_without_external_dependencies -- --ignored
cargo tauri build --no-sign
```

构建产物位于：

```text
src-tauri/target/release/bundle/macos/台球图文生成器.app
src-tauri/target/release/bundle/dmg/台球图文生成器_0.1.0_x64.dmg
```

## 2026-08-15 验收结果

- 普通测试：`16 passed, 2 ignored`；被忽略项包含手动模板视觉检查和已单独完整执行的 100 张离线渲染，压力测试 `100/100` 通过，最终耗时 `226.49s`。
- `cargo clippy --all-targets -- -D warnings` 通过。
- release 为 Intel `x86_64`；`.app` 约 57 MB，`.dmg` 约 39 MB。
- `hdiutil verify` 通过；DMG SHA-256 为 `52a5d0488655bfbb24c107d9b52cc9d9ee54ccdffdc938a30cf94231e0a290cf`。
- release 冷启动后主窗口尺寸为 `1300 x 860`，无 TCP 监听，SQLite 升级前后原有数据保留。
- 前端已通过 Tauri IPC 写入一次性迁移标记，证明生产 CSP 没有阻断 IPC。
- 预览与批量渲染在阻塞任务线程中执行，批量进度、失败与完成事件不被渲染占用主线程。
- 前端缺少可选事件接口时不再中断初始化；单条、批量、设置页按钮已逐项复测，控件不再依赖 WKWebView 隐式全局变量。
- 普通启动只读 SQLite，不自动访问 Keychain；Key 状态检查、Key 保存和 AI 调用均在后台按需访问，避免阻塞界面。
- 6 套模板改用明确的水平布局网格；卡片型模板共用 `60,220,960,1480` 外框，标题、正文、分隔线和话题按模板内边距对齐，长文本与话题不会越过边框。
- DMG 已完成本机隔离目录的挂载、安装、首次启动、同路径覆盖升级和移除；整个过程中 SQLite 哈希与表记录数保持不变。
- 旧 JSON 文案库以单一 SQLite 事务迁移，中途失败会全部回滚，不会留下半迁移状态。
- 二进制动态依赖仅为 macOS 系统框架，源码不包含 localhost、Playwright、FastAPI、ChromeDriver 或联网字体路径。

## 数据与迁移

SQLite 数据库位于：

```text
~/Library/Application Support/com.billiards.matrix/billiards.sqlite3
```

API Key 使用 Keychain service `com.billiards.matrix`、account `api_key`。数据库和普通配置文件不保存密钥。

首次启动会查找旧版开发目录及 `~/Documents/台球图文生成器`，只补充尚未存在的数据。迁移完成或确认无旧数据后写入版本标记，后续启动不会重复扫描或覆盖新设置。只有旧配置真正包含 API Key 时才在迁移中访问 Keychain。旧输出图片不移动、不删除。

## 自用说明

当前包用于本机自用，不上架 App Store，也不要求 Developer ID 签名或 Apple 公证。未签名二进制每次重编后，macOS 仍可能在真正读取 API Key 时重新询问 Keychain 授权；应用启动和本地出图不依赖该授权。只有将应用发给其他人时，才建议额外完成 Developer ID 签名与公证。详见 [RELEASE.md](docs/RELEASE.md)。
