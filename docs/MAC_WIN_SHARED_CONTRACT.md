# Mac / Win 共享契约

本文档约束 Mac 与 Win 客户端共用的云服务、账号和模板协议。平台壳层、本地文件路径、安装器、隧道和图标资源不属于共享契约。

## 2026-08-20 同步范围

- Mac 与 Win 当前共享产品版本为 `0.1.2`。
- 两端必须同时支持注册申请审核、登录状态码、refresh token 续期、启动会话校验、50 个模板 ID、30 种语气、随机模板和自定义数量分配。
- 两端必须向同一个 AI 网关传递模板 ID，并遵守同一份标题、正文和话题容量规则。
- 本地渲染实现可以不同，但离线渲染、`1080 x 1920` 输出、长文本自适应和不覆盖文件名属于共同验收标准。
- Windows SSH 隧道、`ssh.exe` 回收、WebView2、NSIS/MSI、`.ps1`、Windows Credential Manager 和 Windows 图标仅属于 Windows 壳层，Mac 不实现这些代码。
- Mac 使用 Keychain、`.app/.dmg` 和现有 macOS 本地云服务连接配置，不改动 Windows 支线文件。

## 账号

- POST /api/v1/auth/register-application 接受 email、password、confirmPassword。
- 密码长度为 8 至 256 个字符，必须同时包含字母和数字。
- 注册返回 HTTP 202、status: pending，不直接发放 access token。
- 待审核账号登录返回 403 / application_pending；拒绝账号返回 403 / application_rejected，客户端允许立即重新提交。
- access token 有效期短，refresh token 保存在系统凭据存储中。启动和收到 401 时先刷新，刷新失败清理本地会话并要求重新登录。

## AI 请求

- 单条请求：{ prompt, template }。
- 批量请求：{ prompt, count, template }。
- template 使用下方 50 个 ID；旧客户端不传时服务端按 magazine 容量兼容。

## 模板 ID

magazine、magazine_pro、fresh、minimal、poster、journal、neon_club、chalkboard、retro_ticket、cyber_grid、cream_note、arena_score、sunset_gradient、ink_stamp、glass_card、tactical_blue、midnight_lux、candy_pop、forest_match、steel_gray、royal_gold、ocean_wave、lava_motion、pearl_lite、street_snap、comic_burst、vaporwave、newspaper、coffee_receipt、scoreboard_green、purple_stage、ice_blue、red_warning、kraft_label、mint_mono、black_gold、gradient_ring、billiard_felt、tournament_bracket、soft_shadow、bold_blocks、pink_soda、desert_sand、matrix_code、club_vip、clean_blue、orange_zine、silver_card、green_laser、classic_serif。

## 模板容量

- magazine: 标题 30、正文 96 / 6 行。
- minimal: 标题 30、正文 136 / 8 行。
- poster: 标题 30、正文 144 / 8 行。
- 其他模板：标题 30、正文 112 / 7 行。
- 所有模板最多 3 个话题，每个话题最多 12 字。

客户端必须展示容量状态，服务端必须按模板收口，渲染层必须先缩字号和行距，最后才加省略号。
