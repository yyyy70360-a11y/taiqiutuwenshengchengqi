# Windows 客户端共用后台约束

本文档给 Windows 客户端开发工程师使用。核心原则：**Mac 与 Windows 可以有不同的本地实现，但必须共用同一套服务器后台协议**。

## 1. 边界原则

- `main` 分支里的 `server/` 是唯一后台基准；Windows 支线不得维护另一套不兼容的后台。
- Windows 客户端只接入后台公开 API，不直接读取或修改服务器数据库。
- 本地渲染、安装包、WebView2、Credential Manager、路径选择、UI 交互可以按 Windows 方式实现，不要求与 macOS 一致。
- 账号、同步、AI 网关、模板容量兜底、后台管理都以服务器实现为准。
- 如果 Windows 需要新增后台字段或接口，先改 `main/server/` 并保持旧客户端兼容，再让各端接入。

## 2. 不允许做的事

- 不要把 DeepSeek、通义、Kimi、OpenAI 等供应商 API Key 写进 Windows 客户端、安装包、日志或本地配置。
- 不要在 Windows 支线删除、回滚或替换 `server/migrations/` 已上线迁移。
- 不要改动统一错误格式：`{ "error": "...", "message": "..." }`。
- 不要让客户端依赖管理员后台页面或后台 Cookie；客户端只使用 `/api/v1/*`。
- 不要把图片上传到服务器；当前图片仍由客户端本地渲染和保存。
- 不要在客户端明文长期保存 access token、refresh token 或用户密码。

## 3. 基础地址与健康检查

开发期可通过本机 SSH 隧道访问：

```text
http://127.0.0.1:38123
```

生产期必须使用 HTTPS 域名，不要让客户端直连公网明文 HTTP。

健康检查：

```http
GET /health
```

示例响应：

```json
{
  "status": "ok",
  "service": "billiards-api",
  "version": "0.1.0",
  "environment": "production"
}
```

版本检查：

```http
GET /api/v1/version
```

## 4. 认证协议

注册：

```http
POST /api/v1/auth/register
Content-Type: application/json
```

```json
{
  "email": "user@example.com",
  "password": "至少8位密码"
}
```

登录：

```http
POST /api/v1/auth/login
Content-Type: application/json
```

```json
{
  "email": "user@example.com",
  "password": "至少8位密码"
}
```

成功响应：

```json
{
  "accessToken": "...",
  "refreshToken": "...",
  "tokenType": "Bearer",
  "expiresIn": 900
}
```

约束：

- `accessToken` 有效期为 15 分钟。
- `refreshToken` 有效期为 30 天，可被服务器撤销。
- Windows 客户端应把 token 存入 Windows Credential Manager，不要存 SQLite 明文。
- 普通业务请求使用 `Authorization: Bearer <accessToken>`。
- access token 过期时调用 refresh；refresh 失败则要求用户重新登录。

刷新：

```http
POST /api/v1/auth/refresh
Content-Type: application/json
```

```json
{
  "refreshToken": "..."
}
```

退出当前设备：

```http
POST /api/v1/auth/logout
Content-Type: application/json
```

```json
{
  "refreshToken": "..."
}
```

## 5. 用户数据同步接口

所有接口都需要：

```http
Authorization: Bearer <accessToken>
```

设置：

```http
GET /api/v1/me/settings
PUT /api/v1/me/settings
```

字段：

```json
{
  "apiUrl": "",
  "apiModel": "",
  "outputDir": ""
}
```

说明：

- 这些字段是用户偏好，不是供应商 API Key。
- `apiUrl`、`apiModel` 保留兼容旧本地设置；AI Provider 真实配置以服务器后台为准。

账号人设：

```http
GET /api/v1/me/accounts
PUT /api/v1/me/accounts
```

单条结构：

```json
{
  "id": "可选，云端ID",
  "name": "账号名称",
  "region": "斗门",
  "level": "2档",
  "persona": "热情约球型",
  "tone": "口语化",
  "status": "养号中"
}
```

文案库：

```http
GET /api/v1/me/copy-library
POST /api/v1/me/copy-library
```

单条结构：

```json
{
  "id": "可选，云端ID",
  "title": "标题",
  "body": "正文",
  "tags": "#话题"
}
```

同步策略：

- 第一版采用手动同步 + 最后写入者优先。
- 下载云端数据前必须提醒用户：会替换本机账号和文案库，但不删除本地图片。
- 上传本机数据前必须提醒用户：会覆盖云端同类数据。

## 6. AI 网关接口

单条文案：

```http
POST /api/v1/ai/generate-copy
Authorization: Bearer <accessToken>
Content-Type: application/json
```

```json
{
  "prompt": "用户提示词",
  "template": "magazine"
}
```

批量文案：

```http
POST /api/v1/ai/generate-batch-copy
Authorization: Bearer <accessToken>
Content-Type: application/json
```

```json
{
  "prompt": "用户提示词",
  "count": 10,
  "template": "magazine"
}
```

响应单条结构：

```json
{
  "id": null,
  "title": "标题",
  "body": "正文",
  "tags": "#话题1 #话题2"
}
```

约束：

- `prompt` 不能为空，最长 20000 字符。
- `count` 服务端会限制到 `1-100`。
- `template` 是可选字段；旧客户端不传时服务器按 `magazine` 处理。
- 客户端可以在提示词里写模板容量，但不能只依赖客户端；服务器会再次追加容量硬约束并做最终裁剪。
- AI Provider Key 只在服务器后台配置；客户端永远拿不到。

## 7. 模板容量标准

服务器当前识别这些模板 ID：

| 模板 ID | 标题上限 | 正文上限 | 正文行数 | 话题数量 | 单个话题 |
| --- | ---: | ---: | ---: | ---: | ---: |
| `magazine` | 30 | 96 | 6 | 3 | 12 |
| `magazine_pro` | 30 | 112 | 7 | 3 | 12 |
| `fresh` | 30 | 112 | 7 | 3 | 12 |
| `minimal` | 30 | 136 | 8 | 3 | 12 |
| `poster` | 30 | 144 | 8 | 3 | 12 |
| `journal` | 30 | 112 | 7 | 3 | 12 |

Windows 客户端可自行决定如何展示容量提示，但提交 AI 请求时应传当前模板 ID。批量混合模板时，建议传本次实际使用模板里正文容量最小的模板 ID。

## 8. 本地实现可自由发挥的范围

Windows 可以自行实现：

- Windows Credential Manager 存储登录会话。
- 本地 SQLite 路径、迁移路径和输出目录选择。
- WebView2 安装检测、NSIS / MSI 安装包和 Windows 代码签名。
- 本地图像渲染器、字体 fallback、打开输出目录方式。
- 前端 UI 布局、容量提示样式、批量进度展示。

只要不改变服务器 API 契约，本地实现不需要和 macOS 一模一样。

## 9. 变更流程

涉及后台时按这个顺序：

1. 先在 `main/server/` 修改接口、迁移和测试。
2. 保持旧客户端兼容；新增字段优先做可选字段。
3. 更新本文档和 `server/README.md`。
4. 服务端测试和 clippy 通过后再让 Windows 支线接入。
5. Windows 支线只同步必要的客户端调用，不回滚服务器实现。

## 10. Windows 端最低验收

- 能配置服务器地址并登录。
- access token 过期后能自动 refresh。
- refresh 失效后能明确提示重新登录。
- 能上传 / 下载账号和文案库，且不会删除本地图片。
- 单条和批量 AI 文案请求经过服务器完成。
- `template` 字段按当前模板传递；混合模板按最保守模板传递。
- 客户端日志、SQLite、安装包和错误提示中都没有供应商 API Key、access token、refresh token 或密码。
