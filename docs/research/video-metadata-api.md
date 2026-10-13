# Bilibili 与 YouTube 视频元数据抓取可行路径调研

> 调研日期：2026-10-13
> 目标：为 AetherHUD 的视频元数据抓取（标题、简介、首条热门/置顶评论）确定接入方案、凭证需求、配额天花板与降级策略。

## 一、Bilibili 公开接口现状

### 1.1 视频详情接口 `x/web-interface/view`

- 端点：
  - `https://api.bilibili.com/x/web-interface/view`（经典端点）
  - `https://api.bilibili.com/x/web-interface/wbi/view`（WBI 签名变体）
- 请求方式 GET，参数 `aid`/`bvid` 二选一。
- 认证方式：官方（社区整理的接口文档）标注为 Cookie(SESSDATA)，但备注明确「**限制游客访问的视频需要登录**」——即普通公开视频匿名（无 Cookie）也可取到 `title`、`desc`（简介）、`owner`、`stat` 等字段。受限视频（如仅登录可见、大会员抢先）返回 `code = -403`（权限不足）。
- 返回值关键码：`0` 成功；`-403` 权限不足；`-404` 无视频；`62002` 稿件不可见；`62004` 审核中。
- 来源：[bilibili-API-collect（Dreamkaka 镜像）docs/video/info.md](https://github.com/Dreamkaka/bilibili-API-collect/blob/master/docs/video/info.md)

### 1.2 WBI 签名机制（当前要求）

- 自 2023 年 3 月起，B 站 Web 端部分接口采用 WBI 签名：请求需附加 `w_rid`（签名）与 `wts`（秒级时间戳）两个 Query 参数，作为 Web 端风控手段。
- 签名流程（至今有效）：
  1. 从 nav 接口 `https://api.bilibili.com/x/web-interface/nav` 取 `wbi_img.img_url`/`sub_url`（**匿名请求也返回该字段**，故匿名即可实现签名）；或从 `bili_ticket` 接口获取。
  2. 取文件名得 `img_key`/`sub_key`，按固定 64 长度映射表 `MIXIN_KEY_ENC_TAB` 重排、截取前 32 位得 `mixin_key`。
  3. 参数加 `wts` 后按键名升序 URL 编码拼接 `mixin_key`，MD5 得 `w_rid`（中文参数编码字母需大写、空格为 `%20`）。
  4. `img_key`/`sub_key` **每日更替**，必须做缓存 + 刷新（建议 TTL 12–24h，失败即刷新）。
- 若 WBI 接口缺/错签名，返回 `{"code":0,"data":{"v_voucher":"voucher_..."}}`，可据此检测触发风控。
- 实务结论：经典 `view` 端点匿名不带签名目前仍普遍可用；但越来越多查询接口迁移到 WBI，**建议直接实现 WBI 签名并优先调用 `/wbi/view`**，签名实现成本低（纯 MD5，无凭证依赖），一次实现长期受益。
- 来源：[bilibili-API-collect docs/misc/sign/wbi.md](https://github.com/Dreamkaka/bilibili-API-collect/blob/master/docs/misc/sign/wbi.md)

### 1.3 评论接口 `x/v2/reply` 的可用性与风控

- 端点：`https://api.bilibili.com/x/v2/reply`（GET），关键参数：`type=1`（视频评论区）、`oid=aid`、`sort`（0 时间 / 1 点赞 / 2 回复数）、`ps`(≤20)、`pn`、`nohot`。
- **认证方式标注为 Cookie(SESSDATA) 或 APP**——匿名在多数情况下仍可读取，但属于风控高发接口，匿名 + 高频触发拦截的概率显著高于 `view`。
- 响应 `data` 中直接提供两类关键结构：
  - `hots`：热评列表（热门评论）；
  - `upper`：**置顶评论对象**（B 站置顶可通过此字段明确识别，这是相对 YouTube 的优势）。
- 风控特征：
  - HTTP 层：**412**（Precondition Failed）是 B 站 WAF 对可疑请求的通用拦截，诱因包括缺少 `User-Agent`/`Referer`、缺少 `buvid3` Cookie、请求频率过高、数据中心 IP。2026 年内仍有大量「BilibiliAPI 调用频繁返回 412」的报告。
  - JSON 层：`code = -352`（风控校验失败）、`-101`（未登录）等。
- 缓解措施（按收益排序）：
  1. 带真实浏览器 `User-Agent` + `Referer: https://www.bilibili.com`；
  2. 启动时先调用 `https://api.bilibili.com/x/frontend/finger/spi` 获取并携带 `buvid3` Cookie（匿名指纹，无需登录）；
  3. 限速（建议 ≤1 req/2s 每目标，指数退避重试）；
  4. 可选：用户提供 SESSDATA 换取稳定性（但涉及个人凭证，AetherHUD 不应默认要求）。
- 来源：[bilibili-API-collect docs/comment/list.md](https://github.com/Dreamkaka/bilibili-API-collect/blob/master/docs/comment/list.md)；412 风控讨论见 [CSDN 问答（2026-03）](https://ask.csdn.net/questions/9405843)

### 1.4 B 站推荐接入方案

| 项目 | 方案 |
|---|---|
| 标题/简介 | `GET /x/web-interface/wbi/view?bvid=` + WBI 签名；匿名即可，失败回退无签名经典端点 |
| 置顶/热门评论 | `GET /x/v2/reply?type=1&oid={aid}&ps=1`，读 `data.upper`（置顶）或 `data.hots[0]`（首条热评） |
| 凭证 | 无需登录凭证；建议持 `buvid3` 匿名指纹 Cookie；WBI key 每日缓存刷新 |
| 配额天花板 | 无官方配额；以风控为实际天花板，保守限速（每分钟 ≤20–30 次） |
| 降级链 | wbi/view 签名失败 → 无签名 view → 412/-352 时退避重试 → 仅保留 URL/标题（来自 `<title>` 或用户手输） |

## 二、YouTube Data API v3

### 2.1 配额单位成本（官方配额表，页面最近更新 2026-10-08）

| 方法 | 单位成本 |
|---|---|
| `videos.list` | **1** 单位/次 |
| `commentThreads.list` | **1** 单位/次 |
| `comments.list` | 1 单位/次 |
| `search.list` | 独立配额桶，默认 100 次/天（每次 1 单位） |

- 默认配额：启用 YouTube Data API 的项目为 **10,000 单位/天**（所有非 search/insert 端点合计），太平洋时间午夜重置。
- 单视频完整元数据（1 次 `videos.list` + 1 次 `commentThreads.list`）= **2 单位**，默认配额下理论上限 **~5,000 视频/天**——对个人 HUD 工具而言配额充裕。
- 凭证：API Key（Server key）即可读公开数据，无需 OAuth。
- 来源：[Quota Calculator — YouTube Data API](https://developers.google.com/youtube/v3/determine_quota_cost)

### 2.2 「置顶评论」能否通过 API 识别

- **不能**。`commentThread` 与 `comment` 资源的官方字段清单（`snippet.topLevelComment`、`likeCount`、`publishedAt` 等）中**不存在任何 pinned 标记**；`commentThreads.list` 的返回顺序也不保证置顶评论排在首位。
- 长期社区验证：StackOverflow 多次确认 API 不暴露置顶状态；仅有的弱代理信号是 `isHearted`（UP 主点红心，红心与置顶常同时出现但不等价）。
- 可行近似：`commentThreads.list?part=snippet&videoId=&order=relevance&maxResults=1` 取相关性排序首条，作为「热门/代表性评论」；若需真正置顶只能抓网页端（违反 ToS，不建议）。
- 来源：[CommentThreads — YouTube Data API](https://developers.google.com/youtube/v3/docs/commentThreads)；[Comments — YouTube Data API](https://developers.google.com/youtube/v3/docs/comments)；[StackOverflow 相关讨论](https://stackoverflow.com/posts/76002180/revisions)

### 2.3 YouTube 推荐接入方案

| 项目 | 方案 |
|---|---|
| 标题/简介 | `videos.list?part=snippet&id={videoId}`（1 单位） |
| 热门评论 | `commentThreads.list?part=snippet&videoId=&order=relevance&maxResults=1`（1 单位） |
| 置顶评论 | API 无官方支持；用 relevance 首条近似，标注 `commentKind: "top" | "unavailable"` |
| 凭证 | Google Cloud 项目 + API Key（免费） |
| 配额天花板 | 10,000 单位/天 ≈ 5,000 视频/天；可申请提额 |
| 降级链 | API Key 缺失/配额耗尽(403 quotaExceeded) → 网页 og: 标签（oEmbed `https://www.youtube.com/oembed?url=...&format=json` 免费无配额，仅标题/作者）→ 仅 URL/标题 |

## 三、两平台对比与统一降级策略

| 维度 | Bilibili | YouTube |
|---|---|---|
| 官方性 | 全部为非官方逆向接口，随时可能变动 | 官方 API，稳定 |
| 凭证 | 无需凭证（建议 buvid3 指纹 + WBI 签名） | 必须 API Key |
| 置顶评论 | ✅ `data.upper` 官方字段可识别 | ❌ 无官方支持，仅能近似 |
| 配额 | 无配额，风控限速为天花板 | 10,000 单位/天，2 单位/视频 |
| 主要失败模式 | 412 / -352 风控、-403 受限视频 | 403 quotaExceeded、commentsDisabled |

**统一降级策略（建议实现于 `parser/`）**：
1. 首选各平台 API 路径，超时 ≤4s（符合 AGENTS.md Fail-safe 约定）；
2. B 站：签名端点 → 无签名端点 → 退避重试（412 时指数退避，最多 2 次）；
3. YouTube：Data API → oEmbed（至少保住标题）；
4. 均失败：仅记录 URL + 可获取到的页面标题，`metadata_status = "partial"`，不阻塞入库与分类流程；
5. 评论为可选增强字段，评论抓取失败不影响标题/简介的保存。

## 来源汇总

- [bilibili-API-collect 视频信息接口](https://github.com/Dreamkaka/bilibili-API-collect/blob/master/docs/video/info.md)
- [bilibili-API-collect WBI 签名](https://github.com/Dreamkaka/bilibili-API-collect/blob/master/docs/misc/sign/wbi.md)
- [bilibili-API-collect 评论区明细](https://github.com/Dreamkaka/bilibili-API-collect/blob/master/docs/comment/list.md)
- [YouTube Data API 配额计算器（最近更新 2026-10-08）](https://developers.google.com/youtube/v3/determine_quota_cost)
- [YouTube Data API CommentThreads](https://developers.google.com/youtube/v3/docs/commentThreads)
- [YouTube Data API Comments](https://developers.google.com/youtube/v3/docs/comments)
- [StackOverflow：置顶评论 API 支持讨论](https://stackoverflow.com/posts/76002180/revisions)
- [CSDN：B 站接口 412 风控（2026-03）](https://ask.csdn.net/questions/9405843)
