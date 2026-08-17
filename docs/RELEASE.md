# macOS 自用构建与验收

## 当前状态

开发和未签名 release 打包使用 Xcode 16.4，目标架构为 Intel `x86_64`。本应用仅供本机自用，不上架 App Store；Developer ID 签名、Apple 公证和 stapling 不属于自用验收的前置条件。

2026-08-15 已完成的本机验收：

- `cargo test`：`16 passed, 2 ignored`；被忽略项包含手动模板视觉检查和已单独完整执行的 100 张离线渲染，压力测试 `100/100` 通过，最终耗时 `226.49s`。
- `cargo clippy --all-targets -- -D warnings` 通过。
- release `.app` 与 `.dmg` 生成成功，DMG 完整性校验通过。
- 主可执行文件是 `x86_64`，动态链接仅指向 macOS 系统框架。
- release 冷启动窗口为 `1300 x 860`，进程存活且没有 TCP 监听。
- 预览、保存与批量渲染不占用 Tauri 主线程，批量进度、失败和完成事件可在任务进行中投递。
- SQLite 升级启动前后原有表数据保留，一次性迁移标记已通过 Tauri IPC 写入。
- SQLite 与旧 `settings.json` 不包含 API Key，Keychain 条目存在。
- DMG 已在本机隔离目录完成挂载、安装、首次启动、同路径覆盖升级和移除；数据库哈希与表记录数未变。
- 前端按钮已在缺少可选事件接口的条件下逐项复测，初始化不中断。
- 冷启动日志不包含 Keychain 读取；钥匙串只在用户检查状态、保存 Key 或调用 AI 时按需访问。
- 6 套模板的水平网格和卡片外框已做回归检查，`1080 x 1920` 对齐样图已逐张目检。

## 可选的对外分发条件

以下内容仅在未来把安装包发给其他人时需要，本机自用可以跳过：

1. Apple Developer Program 有效成员资格。
2. 钥匙串中安装有效的 `Developer ID Application` 证书及私钥。
3. 配置 App Store Connect API Key，或提供 Apple ID、Team ID 和 app-specific password。
4. 确认 bundle identifier `com.billiards.matrix` 属于对应团队。

## 发布构建

无证书的本机 release 验收包：

```bash
cargo tauri build --no-sign
```

产物：

```text
src-tauri/target/release/bundle/macos/台球图文生成器.app
src-tauri/target/release/bundle/dmg/台球图文生成器_0.1.0_x64.dmg
```

当前自用 DMG SHA-256：

```text
52a5d0488655bfbb24c107d9b52cc9d9ee54ccdffdc938a30cf94231e0a290cf
```

未来如需对外分发，安装 Developer ID Application 证书后再执行：

```bash
cargo tauri build
codesign --verify --deep --strict --verbose=2 \
  src-tauri/target/release/bundle/macos/台球图文生成器.app
xcrun notarytool submit \
  src-tauri/target/release/bundle/dmg/台球图文生成器_0.1.0_x64.dmg \
  --keychain-profile billiards-notary --wait
xcrun stapler staple \
  src-tauri/target/release/bundle/dmg/台球图文生成器_0.1.0_x64.dmg
spctl --assess --type open --context context:primary-signature -vv \
  src-tauri/target/release/bundle/dmg/台球图文生成器_0.1.0_x64.dmg
```

不要把证书、API 私钥、Apple ID 密码或 Keychain profile 内容写入仓库。

## 发布验收清单

- 在断网状态下完成 6 模板预览和保存。
- 确认进程树中没有 Python、Chrome 或 localhost 服务。
- 生成 100 张，确认文件名唯一、进度到达完成且失败数正确。
- 全新用户目录首次启动，确认数据库和默认预设创建成功。
- 使用旧版数据启动，确认设置、账号、文案库迁移且旧输出未被移动。
- 升级安装前后比较数据库路径与内容，确认用户数据保留。
- 检查 SQLite、应用日志和错误提示均不包含 API Key。
- 在一台未安装开发工具的受支持 Mac 上安装 DMG、首次启动和卸载。
- 对外分发时使用 `codesign`、`notarytool`、`stapler` 和 `spctl` 验证签名包。

当前未签名构建可以作为本机自用完成包，但不能据此宣称已完成第三方机器上的分发验收。无开发工具的独立 Mac 验证留到确有对外分发需求时再做。

## 升级与卸载

应用升级不得更改 identifier `com.billiards.matrix`，否则系统会使用新的数据目录和 Keychain 命名空间。拖动应用到废纸篓只删除程序，不会删除 `~/Library/Application Support/com.billiards.matrix` 或 Keychain 中的 API Key；彻底卸载时应由用户明确决定是否移除这些数据。
