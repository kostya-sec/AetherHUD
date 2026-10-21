# API 凭证与密钥存放（API Keys & Keychain Spec v1）

> 状态：**进行中**（2026-10-21，等用户开通并验证）
> 来源票：[任务：开通 Jina / YouTube Data API / 飞书自建应用凭证](https://github.com/kostya-sec/AetherHUD/issues/15)
> 依赖决议：#5（飞书多维表格同步）、#2（YouTube Data API）、#3（Jina Reader）、#10（settings 表与 key_ref）、#18（评测需要真实 provider）
> 一句话：v1 要三个外部凭证，全部存进 **OS Keychain**（Windows 凭据管理器 / macOS 钥匙串），数据库和配置文件里只存"指针"，永不明文落盘。

## 1. 需要什么

| # | 服务 | 字段 | v1 是否必填 | 用途 | Keychain 条目（account） |
|---|---|---|---|---|---|
| 1 | 飞书开放平台（自建应用） | App ID | ✅ 必填 | 多维表格增量同步（#5） | `feishu.app_id` |
| 2 | 飞书开放平台（自建应用） | App Secret | ✅ 必填 | 换 `tenant_access_token` | `feishu.app_secret` |
| 3 | 飞书多维表格 | Base app_token + table_id | ✅ 必填 | 同步源定位（可非秘密） | `feishu.app_token` / settings |
| 4 | YouTube Data API v3 | API Key | ✅ 必填 | 视频元数据（#2） | `youtube.api_key` |
| 5 | Jina Reader | API Key | ⬜ 可选 | 网页正文抓取 L2 路径（#3） | `jina.api_key` |
| 6 | 分类 Provider（OpenAI 兼容） | API Key | ⬜ 本票不含 | 分类引擎 + 评测（#14/#21） | `classifier.api_key` |

- 第 5 项按 #3 决策是**用户可选**：无 key 走 20 RPM 匿名额度，个人单机够用；有 key 500 RPM + 可观测余量。所以 Jina 走不通不阻塞 v1，但票里既然要求就一并开通。
- 第 6 项不在本票范围（分类 provider 型号与计费在 PRD v2 / #19 讨论），条目名先占位。

## 2. 存放规范

### 2.1 Keychain 二元组

应用内统一用 `service` + `account` 两段式（与 Rust `keyring` crate 的 API 形状一致）：

| 项 | 值 |
|---|---|
| service | `aetherhud` |
| account | 上表第 5 列 |
| Windows 物理 target | `aetherhud.<account>`（Generic Credential；实际拼接以 keyring 后端为准） |
| macOS | 登录钥匙串，service/account 同上 |

- **数据库只存逻辑引用**，不存明文：`settings.classifier.api_key_ref`（#10 §settings）之类的字段存 `keychain://aetherhud/youtube.api_key` 这种 URI。
- 未配置的凭证在设置页显示「未配置」，对应功能降级而不是崩溃：
  - 无 YouTube key → 视频元数据退到 oEmbed（标题保底）
  - 无 Jina key → 走匿名 20 RPM 或纯 L1 直连
  - 无飞书凭证 → 该同步源不显示/不轮询
- 严禁：写进 `.env`、写进 SQLite、写进日志、写进 panic 信息。日志里密钥一律 mask 成前 4 后 4。
- 开发期临时文件：`.local/keys.env`（已在 .local/ 下，gitignore 覆盖），验证完可删；它不是 v1 的运行时方案。

### 2.2 用户怎么把凭证放进去

v1 设置页「凭证管理」分区（PRD v2 待写 UI 细节）负责写入；应用首次写入后，在设置页可看到「已存于系统钥匙串」状态，不显示明文。开发阶段用脚本验证（见 §4），暂不写入系统存储。

## 3. 怎么拿

### 3.1 飞书自建应用（最麻烦的一项）

前置：需要一个飞书账号 + 一个"企业/团队"。没有团队也能自己建一个免费团队，**自己就是管理员，应用权限免审**（官方文档原话：应用测试阶段可自行创建新企业，在新企业中创建应用、添加权限，实现权限免审）。

1. 打开 <https://open.feishu.cn/app>，用飞书账号登录；没有团队先到 <https://www.feishu.cn/hc/zh-CN/articles/360043741453> 建一个。
2. 「创建企业自建应用」，名称随意（如 `AetherHUD`），描述随意。
3. 左侧「权限管理」→ 搜索并添加：
   - `bitable:app`（查看、评论、编辑和管理多维表格）—— 一把梭，读写都覆盖；
   - 若目标表浏览器地址是 `/wiki/` 形态，再加 `wiki:wiki:readonly`（用来把 wiki 节点解析成 app_token）。
   - 想走最小权限就换成 `base:record:retrieve` + `base:record:update` + `bitable:app:readonly`（#5 原方案）。
4. 左侧「版本管理与发布」→ 创建版本 → 申请发布（自己审批，通常秒过）。**权限改动必须重新发版才生效。**
5. 左侧「凭证与基础信息」→ 复制 **App ID** 和 **App Secret**。
6. **把应用加进目标多维表格**（不做这步必然报 91403 Forbidden）：
   - 打开目标 Base → 右上角「协作」/「分享」→ 添加协作者 → 搜应用名或 App ID → 给「可编辑」权限。
   - 如果 Base 开了高级权限，还要在「高级权限」里给应用「可管理」。
7. 复制该表浏览器地址：形如 `https://xxx.feishu.cn/base/bascnXXXX?table=tblXXXX&view=vewXXXX`。里面 `bascnXXXX` 就是 app_token，`table=tblXXXX` 就是 table_id。

### 3.2 YouTube Data API v3

1. 打开 <https://console.cloud.google.com/>，新建项目（如 `aetherhud`）。
2. 「API 和服务」→「库」→ 搜 `YouTube Data API v3` → 启用。
3. 「API 和服务」→「凭证」→ 创建凭证 → **API 密钥**。
4. 建议给密钥加「API 限制 = YouTube Data API v3」，防止被别处拿去刷额度。
5. 免费配额：默认 **10,000 单位/天**（美国太平洋时间 0 点重置）；`videos.list` 1 单位/次、`commentThreads.list` 1 单位/次，所以约 5,000 视频/天；`search.list` 另有每天 100 次独立桶（HUD 不用它做搜索）。

已知坑：新项目如果提示结算/验证，先停手告诉我，别绑卡。

### 3.3 Jina Reader

1. 打开 <https://jina.ai/>，用 GitHub / Google 登录。
2. 打开 <https://jina.ai/api-dashboard> 复制 API Key。
3. 免费额度（#3 调研，2026-10-13）：无 key 20 RPM；免费 key 500 RPM；免费 key 附带的 token 配额限**非商用**（CC-BY-NC），个人自用无碍。
4. 平均延迟 7.9s（headless 渲染），热 URL 0.9–2.2s —— 所以 HUD 里必须异步，不能堵 UI。

已知坑：本机（2026-10-21）实测 `jina.ai` / `r.jina.ai` 直连 DNS 污染（解析到 31.13.x.x / 199.59.x.x），走代理地址 127.0.0.1:10809 能建隧道但 TLS 无响应。**换一个代理节点再验证**；这不影响 v1（默认 L1 直连抓取，Jina 只是降级路径）。

## 4. 怎么验证

脚本：`scripts/setup/verify-keys.mjs`（零依赖，Node 18+）。

```
# 1. 把凭证填进 .local/keys.env（模板见 §4.1）
# 2. 跑验证
node scripts/setup/verify-keys.mjs
# 需要代理时（Node 24 支持环境变量代理）：
$env:HTTPS_PROXY="http://127.0.0.1:10809"; $env:NODE_USE_ENV_PROXY="1"; node scripts/setup/verify-keys.mjs
```

脚本做的事与判定：

| 服务 | 请求 | 通过条件 | 能证明什么 |
|---|---|---|---|
| Jina | `GET https://r.jina.ai/https://example.com` | 200 + 返回正文 | key 有效 + 网络可达（读到 `x-ratelimit-*` 头就顺手记下配额） |
| YouTube | `videos.list` 查 `jNQXAC9IVRw` | 200 + 有 items | key 有效 + API 已启用 + 配额未耗尽 |
| 飞书 | token → 列数据表 → 列字段 → search 读 5 条 | 每步 `code == 0` | app_id/secret 有效 + 应用已加入 Base + 读权限生效 |

产物：控制台摘要（英文，避免 Windows 控制台乱码）+ `.local/keys-report.json`（实测结果，**不含密钥明文**，可直接贴到 issue）。退出码 0 = 全过。

### 4.1 `.local/keys.env` 模板

```
JINA_API_KEY=
YOUTUBE_API_KEY=
FEISHU_APP_ID=
FEISHU_APP_SECRET=
FEISHU_BASE_URL=
```

## 5. 配额速查

| 服务 | 限额 | 来源 |
|---|---|---|
| Jina Reader | 匿名 20 RPM / 免费 key 500 RPM；免费 token 非商用 | #3 调研（2026-10-13） |
| YouTube Data API v3 | 10,000 单位/天；`videos.list` 1 单位；`search.list` 100 次/天 | 官方配额表（2026-10-15 更新，本机实测 2026-10-21） |
| 飞书多维表格 | search 20 QPS；记录更新 50 QPS（按 API×应用×租户） | #5 调研（2026-10-13） |

## 6. 已知风险

| 风险 | 影响 | 应对 |
|---|---|---|
| 飞书个人版没有团队 | 无法进开发者后台 | 自建免费团队，自己当管理员（官方支持） |
| 飞书应用没加进 Base | 全部接口 91403 | checklist §3.1 第 6 步 |
| 飞书改了权限没发版 | 权限不生效 | 每次改权限都重新「创建版本 → 发布」 |
| Jina 域名在用户网络不可达 | L2 抓取路径不可用 | v1 默认 L1 直连；网络可用时再开 Jina |
| YouTube key 泄露被刷额度 | 当天配额废掉 | 密钥加 API 限制；Keychain 存储 |
| Google Cloud 要求绑卡 | 可能阻塞开通 | 先停手，报告后再决定 |
