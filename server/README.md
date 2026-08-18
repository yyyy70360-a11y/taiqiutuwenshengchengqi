# 台球图文生成器云端 API

这是 macOS 云端化原型的服务端。图片仍由客户端本地渲染，服务端负责账号、配置同步、文案库和 AI 网关。

## 本地运行

```bash
cp server/.env.example server/.env
cargo run --manifest-path server/Cargo.toml
curl http://127.0.0.1:38123/health
```

默认只监听 `127.0.0.1:38123`，不直接暴露公网。生产环境由 Caddy 终止 HTTPS，再反向代理到该端口。

## 当前接口

- `GET /health`
- `GET /api/v1/version`
- `POST /api/v1/auth/register`
- `POST /api/v1/auth/login`
- `POST /api/v1/auth/refresh`
- `POST /api/v1/auth/logout`
- `GET/PUT /api/v1/me/settings`
- `GET/PUT /api/v1/me/accounts`
- `GET/POST /api/v1/me/copy-library`
- `POST /api/v1/ai/generate-copy`
- `POST /api/v1/ai/generate-batch-copy`

用户数据接口和 AI 接口使用 `Authorization: Bearer <accessToken>`。access token 有效期 15 分钟，refresh token 有效期 30 天且只在数据库保存 SHA-256 哈希。密码使用 Argon2id 哈希。

## 服务器部署原则

- 以独立 `billiards` 用户运行，不使用 root 启动服务。
- 真实配置放在 `/etc/billiards-api/server.env`，权限设为 `600`。
- API Key、数据库密码和 Token 不进入 Git、日志或客户端。
- PostgreSQL 不开放公网端口，只允许本机 API 服务访问。
- Caddy 配置域名后自动申请 HTTPS 证书。

详见 `docs/MACOS_CLOUD_PROTOTYPE_CHECKLIST.md`。
