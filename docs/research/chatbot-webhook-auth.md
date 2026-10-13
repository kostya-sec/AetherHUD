# 调研：TG / 飞书 / 钉钉 / 微信 Bot 的 Webhook 接入与签名校验

- **调研日期**：2026-10-13
- **关联 Issue**：[kostya-sec/AetherHUD#6](https://github.com/kostya-sec/AetherHUD/issues/6)
- **调研范围**：四类聊天 Bot 向「本地 axum webhook 接收器」投递消息的接入机制、验签方式与合规风险。所有结论均以官方文档为准，链接见文末。

## 一、结论速览（可行性矩阵）

| 平台 | 能收消息？ | 能验签？ | 需要公网地址？ | 合规风险 | 对 axum 本地接收器的适配建议 |
|---|---|---|---|---|---|
| **Telegram Bot** | ✅ 官方 webhook（`setWebhook`） | ✅ `secret_token` → `X-Telegram-Bot-Api-Secret-Token` 头比对 | ✅ 需要 HTTPS（443/80/88/8443），Cloudflare Tunnel 可满足 | 低（官方机制）；注意 TG 在部分网络环境不可达 | axum 暴露一个 POST 路由 + 常量时间比较 secret 头 |
| **飞书（自建应用）** | ✅ 事件订阅 webhook；另有不需公网的**长连接模式** | ✅ 签名校验 `X-Lark-Signature`（SHA256）+ Verification Token + AES-256-CBC 解密 | webhook 模式需要（IPv4 公网地址）；长连接模式**不需要** | 低（官方机制）；群消息权限分粒度，全量群消息是敏感权限需审核 | 首选 webhook + axum：处理 url_verification challenge → 验签 → 解密 → 解析 `im.message.receive_v1` |
| **钉钉（企业内部机器人）** | ✅ HTTP 回调（outgoing）；另有不需公网的 **Stream 模式** | ✅ HTTP 头 `timestamp` + `sign`（HmacSHA256 + Base64） | HTTP 模式需要；Stream 模式**不需要** | 低（官方机制） | axum POST 路由校验 timestamp 时效（±1h）与 sign；或评估 Stream SDK（Rust 无官方 SDK，HTTP 模式更直接） |
| **微信个人号** | ❌ 无官方 API | — | — | **高**：第三方协议库（itchat 类）违反用户协议，封号风险 | 不推荐接入 |
| **企业微信·群机器人** | ❌ **只能发不能收**（webhook 仅 `cgi-bin/webhook/send`） | 不适用（无入站） | — | 低 | 不能作为消息来源；只能做通知出口 |
| **企业微信·自建应用回调** | ✅ 接收成员发给应用的消息（非群聊全量） | ✅ `msg_signature`（SHA1 排序签名）+ AES-CBC（EncodingAESKey）+ echostr 验证 URL | ✅ 需要公网 HTTPS | 低（官方机制） | axum 同时挂 GET（VerifyURL）与 POST（DecryptMsg）处理 |
| **企业微信·智能机器人（aibot）** | ✅ 支持 URL 回调与长连接接收消息（含群聊@场景） | ✅ 回调验签 + 官方 SDK（如 aibot-node-sdk） | URL 回调需要；长连接不需要 | 低（官方机制） | 若必须在企微群聊收消息，优先评估此路径而非群机器人 |

## 二、Telegram Bot

### 接入机制
- 调用 Bot API 的 [`setWebhook`](https://core.telegram.org/bots/api#setwebhook)，指定 HTTPS URL；此后 Telegram 以 HTTPS POST（JSON 序列化的 `Update` 对象）投递。webhook 与 `getUpdates` 互斥。
- 端口限制：webhook 仅支持 **443、80、88、8443** 端口；证书需被信任（自签证书需通过 `certificate` 参数上传公钥）。

### 验签方式（secret_token）
- `setWebhook` 时传 `secret_token`（1–256 字符，仅允许 `A-Z a-z 0-9 _ -`）。之后每个 webhook 请求都会携带 HTTP 头 **`X-Telegram-Bot-Api-Secret-Token`**，值为该 token。
- 接收端只需**常量时间比对**该头与本地保存的 token 即可。这是「共享密钥头比对」，不是 HMAC 签名——实现极简，Rust 侧无需引入额外 crypto 依赖（建议仍用常量时间比较或至少避免提前返回差异）。

### 与 Cloudflare Tunnel 的兼容性
- Cloudflare Tunnel（`cloudflared`，含命名隧道）为本地服务提供 443 端口、合法 CA 证书的 HTTPS 域名，**完全满足 Telegram 的端口与证书要求**，业界有大量通过 Cloudflare Tunnel 注册 Telegram webhook 的实践（如 mozilla-ai/clawbolt 的 `setup-tunnel.sh` 自动发现隧道域名并注册 webhook）。
- 注意点：
  - **Quick Tunnel（trycloudflare.com 临时域名）每次重启域名会变**，webhook URL 需重新注册；生产建议用命名隧道绑定固定域名。
  - Cloudflare 的 Bot Fight Mode / WAF 规则可能误拦 Telegram 服务器请求（n8n 社区有 webhook 无响应的案例），必要时为 webhook 路径加 WAF 例外。
  - Telegram 失败会重试（非 2xx 响应会重投），接收器应幂等。

## 三、飞书（Lark）自建应用

### 接入机制
两种事件订阅方式（开发者后台 → 事件与回调 → 订阅方式）：
1. **将事件发送至开发者服务器（webhook）**：需提供 **IPv4 公网地址**；保存配置时飞书向该地址 POST `url_verification` 请求，携带 `challenge` 字段，应用必须 **1 秒内原样返回** `{"challenge": "..."}`，否则保存失败。
2. **使用长连接接收事件**：官方 SDK 建立 WebSocket 全双工通道（服务器能访问公网即可），内置鉴权与加密，**无需公网 IP/域名/内网穿透，无需解密验签**。仅支持企业自建应用；每应用最多 50 连接；集群模式（多实例只有一个收到消息）；3 秒内处理完否则重推。

### url_verification challenge 流程
- 未配置 Encrypt Key：明文推送 `{ "challenge": "...", "token": "<Verification Token>", "type": "url_verification" }`，原样返回 challenge。
- 已配置 Encrypt Key：推送体为 `{ "encrypt": "<base64密文>" }`，需先解密再取 challenge 返回。

### Encrypt Key 解密
- 算法：**AES-256-CBC + PKCS7Padding**。
- 密钥：`key = SHA256(Encrypt Key)`；密文结构：`base64(iv(16B) + encrypted_event)`，iv 为随机 16 字节。

### 签名校验（推荐）
配置了 Encrypt Key 后使用签名校验，**无需解密即可完成来源校验**：
1. 取请求头 `X-Lark-Request-Timestamp`、`X-Lark-Request-Nonce` 与本地 `encrypt_key` 拼接，UTF-8 编码为 `b1`；
2. `b = b1 + 原始请求 body 字节`（**必须是原始 body，不能反序列化后再序列化**）；
3. `signature = hex(SHA256(b))`，与请求头 **`X-Lark-Signature`** 比对。
- 备选：Verification Token 比对（明文传输时安全性低，仅兜底）。
- Rust 实现要点：axum 中务必用 `axum::body::Bytes` 取原始 body；`sha2` crate 即可完成验签与密钥派生；AES 解密用 `aes` + `cbc` + `block-padding`。

### 消息接收权限（事件 `im.message.receive_v1`）
- 需开启「机器人能力」，并订阅「消息与群组 → 接收消息 v2.0」事件。
- 权限粒度（订阅该事件需下列任一，但决定能收到什么）：
  - 单聊：`im:message.p2p_msg` / `im:message.p2p_msg:readonly`；
  - 群内 @机器人：`im:message.group_at_msg` / `:readonly`（仅用户）或 `im:message.group_at_msg.include_bot:readonly`（含其他机器人）；
  - **群内全部消息**：`im:message.group_msg`（**敏感权限**，需企业管理员审核）/ `im:message.group_msg.include_bot:read`。
- 去重：以 `message_id` 做幂等，不要依赖 `event_id`。

## 四、钉钉（企业内部机器人）

### 真实收消息路径
1. **Stream 模式（官方推荐）**：集成 SDK 与开放平台建立 WebSocket 反向连接，建连时以 ClientId/ClientSecret 鉴权，之后消息明文推送，**零公网 IP、零加解密/签名/TLS 管理、零内网穿透**。机器人回调 topic 固定为 `/v1.0/im/bot/messages/get`。官方 SDK 覆盖 Java/Python/Go/Node 等，**无官方 Rust SDK**。
2. **HTTP 模式（outgoing 回调）**：配置公网 HTTPS 消息接收地址，用户 @机器人 或单聊机器人时钉钉 POST 消息（JSON）。这正是适配 axum 的路径。
3. 注意区分：**自定义群机器人**（webhook 加签机器人）只能**发送**消息，不能接收；能接收的是「企业内部应用的机器人能力」。

### HTTP 模式验签（加签）
- 请求头携带 `timestamp`（毫秒）与 `sign`：
  - `stringToSign = timestamp + "\n" + appSecret`（appSecret 即机器人对应企业内部应用的 AppSecret）；
  - `sign = Base64( HmacSHA256(key=appSecret, data=stringToSign) )`。
- 校验规则：**timestamp 与本地时间相差超过 1 小时即非法**（防重放）；sign 不一致即非法；两者都通过才处理。
- Rust 实现要点：`hmac` + `sha2` + `base64` crate；axum extractor 读 `HeaderMap` 中的 `timestamp`/`sign`。
- 补充：若用「事件订阅 HTTP 推送」（非机器人消息），还有 `aes_key`（AES 密钥 Base64）+ `token`（签名 token）的加密回调体系（`DingTalkEncryptor`），与机器人消息回调是两套机制。

## 五、微信 / 企业微信

### 微信个人号
- **没有任何合规的官方接入路径**。微信未向个人号开放消息收发 API；所有第三方方案（基于 Web/iPad/PC 协议的 itchat、wechaty pad 协议等）均违反《腾讯微信软件许可及服务协议》，属于灰色产业，近年风控收紧、封号风险高。AetherHUD **不应**将个人号作为消息来源。

### 企业微信·群机器人（webhook）
- 文档名为「消息推送配置说明」：群机器人只有一个 **webhook 发送地址**（`https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=...`），向其 POST JSON 即可向群发消息，支持 text/markdown/图片/图文/文件/语音/模板卡片。
- **只能发，不能收**：没有任何入站消息回调。官方开发者社区 2025-03 的回答亦确认：群聊机器人 webhook 仅用于发送，要拿群聊消息需购买「会话内容存档」。
- 注意 webhook URL 即凭证（key 泄漏 = 任何人可发垃圾消息）。

### 企业微信·自建应用回调（可收消息，可验签）
- 在自建应用配置「接收消息」URL 后：
  - **GET 验证 URL**：企业微信带 `msg_signature, timestamp, nonce, echostr` 请求，需验签并 AES 解密 echostr 后**原样明文返回**（VerifyURL）。
  - **POST 收消息**：加密 XML POST 到该 URL（DecryptMsg）。
- 验签：`msg_signature = SHA1( sort(token, timestamp, nonce, msg_encrypt) )`（字典序排序后拼接）。
- 加解密：`AESKey = Base64_Decode(EncodingAESKey + "=")`（32 字节），**AES-256-CBC，IV = AESKey 前 16 字节**，PKCS#7 填充，消息体为 XML。
- 能力边界：主要接收「成员发给应用的消息」（应用单聊场景），不是群聊全量消息。

### 企业微信·智能机器人（aibot，新路径）
- 官方提供「自建应用与智能机器人的对接」，智能机器人支持 **URL 回调模式**与**长连接模式**接收消息（含群聊中 @智能机器人的消息），官方有 aibot-node-sdk 等 SDK。若场景必须是「企微群聊收消息」，这是合规首选，优于买会话存档。

## 六、对 AetherHUD axum 接收器的落地建议

1. **统一路由形态**：`/webhook/telegram`、`/webhook/feishu`、`/webhook/dingtalk`、`/webhook/wecom` 各一条路由，各自独立验签中间件；全部先取 `Bytes` 原始 body 再验签/解密。
2. **公网入口**：Telegram/飞书 webhook/钉钉 HTTP/企微回调都需要公网 HTTPS → 用 **Cloudflare 命名隧道**（固定域名、443、合法证书）统一解决；飞书额外要求 IPv4 公网地址，隧道域名满足。
3. **免公网替代**：飞书长连接、钉钉 Stream、企微 aibot 长连接都绕开 webhook，但都要官方 SDK（无 Rust 版），与本项目「axum 本地接收」架构不符，仅作备选记录。
4. **防重放**：钉钉校验 timestamp ±1h；飞书可校验 `X-Lark-Request-Timestamp` 时效；TG 靠 secret 头 + TLS。所有平台以平台侧消息 ID（`message_id`/`msgId`/`update_id`）做幂等去重。
5. **密钥管理**：所有 token/secret（TG secret_token、飞书 Encrypt Key、钉钉 appSecret、企微 Token/EncodingAESKey）存本地 SQLite/系统 keychain，严禁入库 Git。

## 七、来源

**Telegram**
- [Bot API — setWebhook（secret_token / X-Telegram-Bot-Api-Secret-Token / 端口 443,80,88,8443）](https://core.telegram.org/bots/api#setwebhook)
- [mozilla-ai/clawbolt — setup-tunnel.sh（Cloudflare Tunnel + Telegram webhook 注册实践）](https://github.com/mozilla-ai/clawbolt/blob/main/scripts/setup-tunnel.sh)
- [n8n 社区：Cloudflare Tunnel - Telegram Webhook not responding（WAF 拦截案例）](https://community.n8n.io/t/cloudflare-tunnel-telegram-webhook-not-responding/61892)

**飞书**
- [将事件发送至开发者服务器（url_verification challenge、IPv4 公网地址要求）](https://open.feishu.cn/document/event-subscription-guide/event-subscriptions/event-subscription-configure-/choose-a-subscription-mode/send-notifications-to-developers-server?lang=zh-CN)
- [步骤三：接收事件（X-Lark-Signature 签名校验、AES-256-CBC 解密、Verification Token）](https://open.feishu.cn/document/ukTMukTMukTM/uYDNxYjL2QTM24iN0EjN/event-subscription-configure-/encrypt-key-encryption-configuration-case)
- [使用长连接接收事件](https://open.feishu.cn/document/server-docs/event-subscription-guide/event-subscription-configure-/request-url-configuration-case?lang=zh-CN)
- [接收消息事件 im.message.receive_v1（权限粒度）](https://open.feishu.cn/document/server-docs/im-v1/message/events/receive)

**钉钉**
- [机器人接收消息（Stream 模式 / HTTP 模式）](https://open.dingtalk.com/document/dingstart/robot-receive-message)（镜像：[help.dingtalk.io 版](https://help.dingtalk.io/zh/open/dingstart/robot-receive-message)）
- [接收消息协议（timestamp + sign，HmacSHA256+Base64，±1 小时时效）](https://help.dingtalk.io/zh/open/development/receive-message)
- [服务端 Stream 模式（五零接入）](https://help.dingtalk.io/zh/open/development/introduction-to-stream-mode)
- [配置 HTTP 推送（事件订阅 aes_key/token 体系）](https://help.dingtalk.io/zh/open/dingstart/configure-http-push)
- [自定义机器人安全设置（加签，仅发送场景）](https://help.dingtalk.io/zh/open/dingstart/customize-robot-security-settings)

**微信 / 企业微信**
- [群机器人「消息推送配置说明」（仅发送 webhook）](https://developer.work.weixin.qq.com/document/path/91770)
- [回调配置（自建应用接收消息）](https://developer.work.weixin.qq.com/document/path/90930)
- [加解密方案说明（msg_signature SHA1 排序签名、EncodingAESKey、AES-CBC、VerifyURL）](https://developer.work.weixin.qq.com/document/path/90968)
- [自建应用与智能机器人的对接](https://developer.work.weixin.qq.com/document/path/101521)、[智能机器人长连接](https://developer.work.weixin.qq.com/document/path/101463)、[WecomTeam/aibot-node-sdk](https://github.com/WecomTeam/aibot-node-sdk)
- [企业微信开发者社区：群聊机器人支持消息回调吗？（2025-03，确认 webhook 仅发送，收群消息需会话存档）](https://developer.work.weixin.qq.com/community/question/detail?content_id=16709787612948516025)
- [微信机器人合规风险综述（腾讯云社区，2025）](https://cloud.tencent.cn/developer/article/2730298)
