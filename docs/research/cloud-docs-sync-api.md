# 云端表格平台「增量同步」能力 API 对比调研

- **调研日期**：2026-10-13
- **关联 Issue**：[AetherHUD #5](https://github.com/kostya-sec/AetherHUD/issues/5)
- **结论摘要**：v1 建议支持 **飞书多维表格**（必选）+ **Google Sheets**（次选）；**腾讯文档暂不入选**（API 按调用量计费 + 仅用户授权模式，不适合后台高频轮询）。

---

## 1. 飞书多维表格（Feishu Bitable）

### 1.1 认证方式：tenant_access_token vs user_access_token

来源：[如何选择使用不同类型的 Token（官方）](https://open.feishu.cn/document/uAjLw4CM/ugTN1YjL4UTN24CO1UjN/trouble-shooting/how-to-choose-which-type-of-token-to-use.md)

| 凭证 | 身份 | 适用场景 |
|---|---|---|
| tenant_access_token | 应用自身身份 | 操作应用拥有的资源；访问用户文档需把文档授权/共享给应用 |
| user_access_token | 用户身份（OAuth 授权） | 需要操作用户数据资源，权限范围跟随用户 |

**AetherHUD 场景建议**：后台轮询同步属于常驻任务，推荐 tenant_access_token（自建应用服务端获取，无需用户反复授权）；要求把目标多维表格共享给应用或开通文档权限。user_access_token 可作为后续「个人文档」模式的备选。

### 1.2 按筛选条件列出记录

来源：[查询记录 search（官方）](https://open.feishu.cn/document/uAjLw4CM/ukTMukTMukTM/reference/bitable-v1/app-table-record/search.md)

- POST /open-apis/bitable/v1/apps/:app_token/tables/:table_id/records/search
- 请求体支持 filter（conjunction: and/or，conditions ≤ 50 个；operator 含 is / isNot / contains / isEmpty / isGreater / isLess 等）
- 单次最多 500 行，page_size 最大 500，page_token 游标分页
- **接口频率限制：20 次/秒**
- 权限（任一即可）：base:record:retrieve / bitable:app / bitable:app:readonly
- 注意：多维表格开启高级权限时，调用身份需有可管理权限，否则「调用成功但返回为空」

### 1.3 回写记录状态

来源：[更新记录 update（官方）](https://open.feishu.cn/document/uAjLw4CM/ukTMukTMukTM/reference/bitable-v1/app-table-record/update.md)

- PUT /open-apis/bitable/v1/apps/:app_token/tables/:table_id/records/:record_id
- 增量更新（仅更新传入字段；置空传 null）
- **接口频率限制：50 次/秒**
- 权限（任一即可）：base:record:update / bitable:app
- 注意：从其它数据源同步的数据表不支持增删改

### 1.4 分页与限流（QPS）规则

来源：[频控策略（官方）](https://open.feishu.cn/document/ukTMukTMukTM/uUzN04SN3QjL1cDN.md)

- 频控按 **每个 API × 每个应用 × 每个租户** 计；触发后返回 HTTP 429（部分旧接口 400），code=99991400
- 响应头 x-ogw-ratelimit-reset 给出建议等待秒数，**按此值退避重试是官方推荐做法**
- 多维表格接口暂不支持申请提频
- 查询记录 20 QPS、更新记录 50 QPS —— 对 HUD 轮询场景绰绰有余

### 1.5 自建应用所需权限范围（最小集）

| 用途 | Scope |
|---|---|
| 读记录（筛选/分页） | base:record:retrieve（或 bitable:app:readonly） |
| 回写状态 | base:record:update |
| 枚举数据表/视图 | bitable:app:readonly（含列出数据表、视图） |

### 1.6 增量能力评估

- 无记录级原生变更流/Webhook；增量靠 **filter 条件**实现：
  - 状态位方案：filter「状态 = 未同步」，处理后回写「已同步」
  - 时间戳方案：search 时 automatic_fields=true 返回 last_modified_time，配合 isGreater 过滤（日期字段 operator 受限，见筛选指南）
- 记录自带 record_id，天然适合做去重主键

---

## 2. 腾讯文档（docs.qq.com）

### 2.1 开放平台现状：API 存在但受限

来源：[腾讯文档开放平台开发文档（官方）](https://docs.qq.com/open/document/)、[开放接口总览](https://docs.qq.com/open/document/app/)

- 开放平台**真实存在**，提供文件管理 API、在线表格 API、在线智能表 API、在线文档 API、收集表 API
- **关键限制（2025-07-01 起生效）**：每个应用**限免 20,000 次 API 调用**，超出需填写意向申请表采买 API 资源包 —— 对「轮询同步」这种持续消耗调用量的场景是硬约束
- 另有面向企业的「腾讯文档 SaaS 版」开放接口（操作企业下用户文档数据）

### 2.2 认证模式

来源：[授权流程（官方）](https://docs.qq.com/open/document/app/oauth2/)

- 仅 **OAuth 2.0 授权码模式**：Access Token 30 天、Refresh Token 1 年
- 每次请求需带 Client-ID + Open-ID + Access-Token 三元组
- **没有类似飞书 tenant token 的应用级免用户授权凭证**（面向个人用户文档）；所有请求必须由后台服务发起
- 需先注册开放平台开发者、创建应用并通过审核

### 2.3 智能表读写能力边界

来源：[智能表接口索引（官方）](https://docs.qq.com/open/document/app/openapi/v2/smartsheet/)、[查询记录](https://docs.qq.com/open/document/app/openapi/v2/smartsheet/record/get_records.html)

- 记录 CRUD 齐全：查询记录 POST /openapi/smartbook/v2/files/{fileID}/sheets/{sheetID}，offset/limit 分页，响应含 hasMore/next
- 记录自带 updateTime（毫秒）与 createTime —— 理论上可做增量游标
- 更新记录接口存在，状态回写可行；scope 为 scope.smartsheet
- 在线表格（非智能表）API 仅提供操作表格/获取内容，无记录级筛选

### 2.4 结论

能力上可读可写，但：**计费模式（2 万次免费额度）+ 仅用户 OAuth + 无变更推送**，使它不适合作为 v1 后台轮询源。建议列入 backlog，待有明确用户需求或改用其 MCP 等入口时再评估。

---

## 3. Google Sheets（API v4）

### 3.1 values.get 与增量拉取模式

来源：[spreadsheets.values.get（官方）](https://developers.google.com/workspace/sheets/api/reference/rest/v4/spreadsheets.values/get)、[Track changes（Drive API，官方）](https://developers.google.com/workspace/drive/api/guides/about-changes)

- values.get 按 A1 范围**全量读取**，无 ETag、无 If-Modified-Since、无行级变更流、无筛选参数 —— **Sheets API 本身不支持增量**
- 可行的增量模式：
  1. **Drive changes.list**（pageToken 游标）检测文件级「该 Sheet 被修改过」，命中后再 values.get 全量拉取并在本地 diff（按行号或首列主键）
  2. 简化版：直接周期性 values.get 全量拉取 + 本地哈希 diff（小表场景配额完全够用）
- 批量读用 values.batchGet，一次请求算一次配额

### 3.2 配额

来源：[Usage limits（官方）](https://developers.google.com/workspace/sheets/api/limits)

| 配额 | 每项目每分钟 | 每用户每项目每分钟 |
|---|---|---|
| 读请求 | 300 | 60 |
| 写请求 | 300 | 60 |

- 配额每分钟重置；超限返回 429，官方推荐截断指数退避（1s/2s/4s… + 随机抖动，上限 32–64s）
- 无每日请求总量上限（只要在分钟配额内）
- **注意**：官方公告 2026 年晚些时候超额调用计划开始计费
- 单个请求建议 ≤ 2 MB payload；单次请求处理上限 180s

### 3.3 service account vs OAuth 取舍

来源：[Create access credentials（官方）](https://developers.google.com/workspace/guides/create-credentials)

| 方案 | 优点 | 缺点 | 适用 |
|---|---|---|---|
| Service Account | 无用户交互、token 自管理、适合常驻后台 | **访问不了普通用户的表格**，除非：① 用户把表格共享给 SA 邮箱；② Workspace 域范围委托（仅企业域） | 「共享表格给机器人邮箱」模式，最贴合 HUD 个人用户 |
| OAuth 2.0（installed app） | 可访问用户全部有权限的表格 | 需一次浏览器授权、refresh token 管理；scope 验证（spreadsheets.readonly 属敏感 scope） | 用户不想手动共享表格时 |

**建议**：v1 采用 Service Account + 「把表格共享给 SA 邮箱」模式（交互最简单、无需 OAuth 回调服务）；OAuth 作为 v2 增强。

---

## 4. v1 平台支持建议

| 平台 | v1 | 理由 |
|---|---|---|
| 飞书多维表格 | ✅ 必选 | 应用级 token、20 QPS 查询 + 50 QPS 回写、筛选分页完善、免费额度充足 |
| Google Sheets | ✅ 次选 | 配额宽松（读 300/min）、生态广；无原生增量但 Drive changes + diff 可解 |
| 腾讯文档 | ❌ 暂缓 | 2 万次/应用免费调用上限 + 仅用户 OAuth，轮询成本与接入成本均高 |

## 5. 入选平台同步器设计要点

### 5.1 FeishuSyncer（对应 src-tauri/src/ingestion/ 轮询 worker）

- **轮询间隔**：默认 60s（可配置下限 15s）；20 QPS 上限下，单表每轮 1–2 次 search 调用，远低于限额
- **增量游标**：状态位优先 —— search filter「状态 is 待处理」（由表格侧约定状态列）；兜底时间戳方案（last_modified_time > 上次游标，存 SQLite sync_state 表）
- **分页**：page_size=500，循环 page_token 至 has_more=false
- **状态回写协议**：解析成功 → PUT record_id 置「状态=已入库」+ 写入分类结果列（l1/l2）；失败 → 「状态=解析失败」；回写失败按 x-ogw-ratelimit-reset 退避重试 ≤ 3 次
- **并发控制**：令牌桶限速 5 QPS（远低于上限，多表并行时也安全）

### 5.2 GSheetsSyncer

- **轮询间隔**：默认 120s；每轮 1 次 values.get（全量小表）或先 Drive changes.list 探测（大表），均远低于 60/min/用户配额
- **增量游标**：本地快照哈希 diff（行级 SHA-1 存 SQLite）；大表用 Drive changes.list pageToken 作文件级游标
- **状态回写协议**：values.update 写回约定列（如 _aether_status），值域 pending/imported/failed；写请求计入写配额（300/min），批量回写用 values.batchUpdate
- **退避**：429 时截断指数退避（1s 起、×2、+抖动、上限 64s）

---

## 附：主要来源清单

1. [飞书 - 如何选择 Token](https://open.feishu.cn/document/uAjLw4CM/ugTN1YjL4UTN24CO1UjN/trouble-shooting/how-to-choose-which-type-of-token-to-use.md)
2. [飞书 - 查询记录 search](https://open.feishu.cn/document/uAjLw4CM/ukTMukTMukTM/reference/bitable-v1/app-table-record/search.md)
3. [飞书 - 更新记录 update](https://open.feishu.cn/document/uAjLw4CM/ukTMukTMukTM/reference/bitable-v1/app-table-record/update.md)
4. [飞书 - 频控策略](https://open.feishu.cn/document/ukTMukTMukTM/uUzN04SN3QjL1cDN.md)
5. [腾讯文档开放平台 - 首页](https://docs.qq.com/open/document/)
6. [腾讯文档 - 开放接口总览（限免 2 万次说明）](https://docs.qq.com/open/document/app/)
7. [腾讯文档 - OAuth2 授权](https://docs.qq.com/open/document/app/oauth2/)
8. [腾讯文档 - 智能表接口索引](https://docs.qq.com/open/document/app/openapi/v2/smartsheet/)
9. [腾讯文档 - 智能表查询记录](https://docs.qq.com/open/document/app/openapi/v2/smartsheet/record/get_records.html)
10. [Google Sheets - Usage limits](https://developers.google.com/workspace/sheets/api/limits)
11. [Google Sheets - values.get](https://developers.google.com/workspace/sheets/api/reference/rest/v4/spreadsheets.values/get)
12. [Google Drive - Track changes](https://developers.google.com/workspace/drive/api/guides/about-changes)
13. [Google Workspace - Create access credentials](https://developers.google.com/workspace/guides/create-credentials)
