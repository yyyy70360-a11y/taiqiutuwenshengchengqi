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
- 通过客户端完成一次注册申请、后台批准、登录、上传、下载和 AI 文案联调。
- 增加正式 release 包、升级保留数据和无网络回归记录。

## 注册申请与审核同步

Windows 和 macOS 共用同一套账号库与管理后台。macOS 客户端不要继续把“注册”理解为立即创建账号，应改为以下流程：

1. 登录页提供“注册申请”入口，进入独立申请视图。
2. 申请视图提交邮箱、密码和确认密码到 POST /api/v1/auth/register-application。
3. 服务端返回 HTTP 202 后，客户端显示“注册申请已提交，请等待管理员审核。批准后可直接使用该邮箱和密码登录。”
4. 管理员在 /admin/registration-applications 批准后，服务端才会创建正式账号。
5. 用户返回登录页，用原申请邮箱和密码调用 POST /api/v1/auth/login。

请求字段使用 camelCase：

    {
      "email": "name@example.com",
      "password": "Example2026",
      "confirmPassword": "Example2026"
    }

密码必须至少 8 个字符，并同时包含字母和数字。申请成功响应为 HTTP 202，响应体包含 `status: "pending"` 和 `message`；申请成功不会返回 access token 或 refresh token。

客户端需要按服务端 error 字段处理状态：

| error | 客户端提示 |
| --- | --- |
| application_pending | 申请正在审核，请等待批准 |
| application_rejected | 申请未通过，可重新提交 |
| account_exists | 邮箱已注册，直接登录 |
| too_many_requests | 提交过于频繁，稍后再试 |
| invalid_credentials | 邮箱或密码不正确 |
| account_disabled | 账号已停用，联系管理员 |

兼容说明：旧的 POST /api/v1/auth/register 暂时保留，但行为已经改为提交申请并返回 HTTP 202，不再返回登录令牌。新版 macOS 客户端必须使用 register-application，不能依赖旧接口的响应结构。
