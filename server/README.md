# 台球图文生成器云端 API

这是 macOS 云端化原型的服务端骨架。第一阶段只提供健康检查和版本接口，后续按施工清单增加账号、同步和 AI 网关。

## 本地运行

```bash
cargo run --manifest-path server/Cargo.toml
curl http://127.0.0.1:38123/health
```

默认只监听 `127.0.0.1:38123`，不直接暴露公网。生产环境由 Caddy 终止 HTTPS，再反向代理到该端口。

## 当前接口

- `GET /health`
- `GET /api/v1/version`

## 服务器部署原则

- 以独立 `billiards` 用户运行，不使用 root 启动服务。
- 真实配置放在 `/etc/billiards-api/server.env`，权限设为 `600`。
- API Key、数据库密码和 Token 不进入 Git、日志或客户端。
- PostgreSQL 不开放公网端口，只允许本机 API 服务访问。
- Caddy 配置域名后自动申请 HTTPS 证书。

详见 `docs/MACOS_CLOUD_PROTOTYPE_CHECKLIST.md`。
