# 移动端适配分支说明

本分支用于隔离 Android / Mobile 适配工作，避免实验性移动端改动影响 Windows 10 稳定交付线。

## 当前范围

- 前端增加移动端 viewport 与响应式布局，窄屏下改为纵向工作流。
- 移动端隐藏桌面专属的图片输出路径配置，保存按钮文案改为“保存到应用”。
- Android 生成工程放在 `src-tauri/gen/android/`，本地 Gradle 配置、构建产物和签名密钥文件由目录内 `.gitignore` 排除。
- Android 环境下云登录 token / API Key 暂走本机 SQLite 设置表兜底；Windows/macOS 仍使用系统凭据存储。

## 已验证

- Windows 桌面端 `cargo fmt --all -- --check` 通过。
- Windows 桌面端 `cargo test` 通过。
- Windows 桌面端 `cargo clippy --all-targets --all-features -- -D warnings` 通过。
- 服务端 `cargo fmt / test / clippy` 通过。
- 前端脚本语法、DOM ID 引用和按钮绑定模拟检查通过。
- Windows release NSIS / MSI 安装包可生成。

## 待验证

- Android SDK / NDK / Gradle 环境下的真实 Android 构建。
- Android 首次启动、云服务登录、AI 文案、保存图片和历史记录读取。
- 真机窄屏交互、软键盘遮挡、状态栏/安全区表现。
- Android 本地 token 兜底方案后续是否替换为平台安全存储。

## 分支策略

- `win10-adaptation`：Windows 客户端稳定交付线。
- `mobile-adaptation`：移动端 / Android 实验与适配线。
- 共用服务端接口保持向后兼容；移动端不要新增平台专属服务端接口。
