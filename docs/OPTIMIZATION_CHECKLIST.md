# 本轮优化施工清单

- [x] 统一版本到 0.1.2，服务端已部署并验证 /health。
- [x] SSH 子进程已托管，重连和退出都会回收。
- [x] UI 自检等待模板和预设加载，不再依赖固定延迟。
- [x] Rust 检查、测试、clippy 与 NSIS/MSI 构建已通过。
- [x] 内置 SSH 转发端到端返回服务端 0.1.2 健康响应，验证脚本确认隧道进程回收。
- [x] 已安装 release 关闭窗口回归：客户端退出后内置 ssh.exe 从 1 个回收到 0 个。
- [x] NSIS 同路径升级、静默卸载和重新安装回归通过；卸载后程序目录清理，用户数据库哈希保持不变。
- [x] 补充 NSIS 卸载 hook，清理 fix-shortcut-icons.ps1 生成的根目录 app.ico。
- [x] 增加 Mac 只读验收脚本，统一检查 bundle、版本、Keychain、SQLite 和 DMG 哈希。

## 仍需独立设备验收

- [ ] 干净 Windows 10 安装、升级、卸载、图标和 WebView2。
- [ ] macOS 注册、共享云服务、升级和数据保留。

## 发布动作

- [x] 已提交并推送当前支线，远端与本地无 ahead/behind。
