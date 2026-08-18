# macOS 云端客户端接入说明

## 当前实现

- 图像预览、保存、批量渲染和历史记录仍完全在本机完成。
- 登录会话保存在 macOS Keychain：`cloud_access_token` 和 `cloud_refresh_token`。
- 服务器地址、云账号邮箱和同步时间保存在本地 SQLite；密码不会保存。
- `generate_copy` 和 `generate_batch_copy` 通过 Rust 云客户端调用服务器 AI 网关。
- 客户端不再读取或发送供应商 API Key。供应商 Key 只配置在服务器环境变量中。
- 上传和下载是手动操作。下载会在界面确认后替换本机账号和文案库，不会删除图片历史和输出目录。

## 无域名联调

服务端当前只监听服务器本机回环地址。开发机可通过 SSH 本地隧道访问：

```bash
ssh -N -o IdentitiesOnly=yes -i /path/to/taiqiutuwen.pem \
  -L 38123:127.0.0.1:38123 root@115.191.33.129
```

在客户端设置页填写：

```text
http://127.0.0.1:38123
```

客户端只允许回环地址使用明文 HTTP；远程地址必须使用 HTTPS。隧道关闭后，本地渲染仍可用，云端功能会显示网络错误。

## 有域名后的切换

为现有 Nginx/Caddy 增加 HTTPS 反向代理后，把客户端地址改为 `https://你的域名`。不要把数据库端口或 API 服务端口直接暴露到公网，也不要把真实 API Key 写入客户端配置、Git 或日志。

## 构建

```bash
cargo tauri build --debug --bundles app
```

当前已验证可生成未签名 `.app`。生产域名和服务端 AI Key 配置完成后，再进行 release 构建和 macOS 首次启动回归。

## 当前待办

- 配置真实域名和 HTTPS 反向代理。
- 在 `/etc/billiards-api/server.env` 配置服务器 AI Provider Key。
- 通过客户端完成一次注册、登录、上传、下载和 AI 文案联调。
- 增加正式 release 包、升级保留数据和无网络回归记录。
