# API 凭证与密钥存放（API Keys & Keychain Spec v1）

> 状态：**已确认**（2026-10-21 全部实测完毕：飞书 ✅ / 百炼 ✅ / Jina ✅（需代理）/ YouTube ⏸ 跳过）
> 来源票：[任务：开通 Jina / YouTube Data API / 飞书自建应用凭证](https://github.com/kostya-sec/AetherHUD/issues/15)
> 依赖决议：#5（飞书多维表格同步）、#2（YouTube Data API）、#3（Jina Reader）、#10（settings 表与 key_ref）、#18（评测需要真实 provider）
> 一句话：v1 要三个外部凭证，全部存进 **OS Keychain**（Windows 凭据管理器 / macOS 钥匙串），数据库和配置文件里只存"指针"，永不明文落盘。

## 1. 需要什么

| # | 服务 | 字段 | v1 是否必填 | 用途 | Keychain 条目（account） |
|---|---|---|---|---|---|
| 1 | 飞书开放平台（自建应用） | App ID | ✅ 必填 | 多维表格增量同步（#5） | `feishu.app_id` |
| 2 | 飞书开放平台（自建应用） | App Secret | ✅ 必填 | 换 `tenant_access_token` | `feishu.app_secret` |
| 3 | 飞书多维表格 | Base app_token + table_id | ✅ 必填 | 同步源定位（可非秘密） | `feishu.app_token` / settings |
| 4 | YouTube Data API v3 | API Key | ⏸ 暂不配（2026-10-21 决定） | 视频元数据（#2）；降级为 oEmbed 兜底 | `youtube.api_key` |
| 5 | Jina Reader | API Key | ✅ 已验证（需代理） | 网页正文抓取 L2 路径（#3） | `jina.api_key` |
| 6 | 阿里百炼 DashScope（OpenAI 兼容） | API Key | ✅ 已验证 | 分类引擎 + 评测（#14/#21） | `classifier.api_key` |

- 第 4 项 **暂不配**：用户不为视频元数据单独开 Google Cloud 账号。降级方案见 §3.2。条目名先占位，以后要补直接填同一个位置。
- 第 5 项按 #3 决策是**用户可选**：无 key 走 20 RPM 匿名额度，个人单机够用；有 key 500 RPM + 可观测余量。Jina 走不通不阻塞 v1。
- 第 6 项是 v1 分类的**必需项**（[classification-quality.md](classification-quality.md) 明确「分类 provider 的接入与凭证管理」归 #15）：用户已有百炼账号，直接当默认 provider，不再单独开 OpenAI key。

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
  - 无 YouTube key → 视频元数据退到 oEmbed（只有标题/作者/缩略图，见 §3.2）
  - 无百炼 key → 分类器不可用，全部条目落 `00_未分类/待人工归档`（fail-safe，#18）
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

### 3.2 YouTube Data API v3（v1 暂不配）

**2026-10-21 决定：跳过。** 访问 Google Cloud 要挂代理，用户不想为「视频简介 + 热评」这一层信息单独开 Google 账号。v1 的降级：

| 路径 | 能拿到 | 实测结论（2026-10-21） |
|---|---|---|
| oEmbed（`youtube.com/oembed`） | 标题、作者、缩略图 | ✅ 无 key、免费、可用 |
| 页面直连解析 | 想拿简介 | ❌ watch 页对无 cookie 请求返回空壳，只有通用 meta description |
| Invidious / Piped 公共实例 | 简介 + 评论 | ⚠️ 免费但公共实例时好时坏 |
| yt-dlp 本地 | 全都能拿 | ✅ 最稳，但要随应用分发二进制 |

- 影响：YouTube 视频只有标题/作者/缩略图能喂给分类器，准确率低于「简介 + 热评」路径。按 #20 统计视频只占收藏 13%，可接受。
- 以后要补：<https://console.cloud.google.com/> 新建项目 → 启用 `YouTube Data API v3` → 创建 API 密钥（加 API 限制）→ 填 `.local/keys.env` 的 `YOUTUBE_API_KEY`。配额 10,000 单位/天；`videos.list` / `commentThreads.list` 各 1 单位/次；`search.list` 另有 100 次/天独立桶。
- B 站不需要任何 key：走官方 `x/web-interface/view` + `x/v2/reply`（见 #2 调研）。B 站字幕见 §6。

### 3.3 Jina Reader

1. 打开 <https://jina.ai/>，用 GitHub / Google 登录。
2. 打开 <https://jina.ai/api-dashboard> 复制 API Key。
3. 免费额度（#3 调研，2026-10-13）：无 key 20 RPM；免费 key 500 RPM；免费 key 附带的 token 配额限**非商用**（CC-BY-NC），个人自用无碍。
4. 平均延迟 7.9s（headless 渲染），热 URL 0.9–2.2s —— 所以 HUD 里必须异步，不能堵 UI。

已知坑：本机（2026-10-21）实测 `jina.ai` / `r.jina.ai` **直连 DNS 污染**（解析到 31.13.x.x / 199.59.x.x，直连必挂）；**必须走代理**，代理下实测 200 / 657ms。跑验证脚本时记得带 `HTTPS_PROXY` + `NODE_USE_ENV_PROXY=1`（见 §4）。这不影响 v1——默认 L1 直连抓取，Jina 只是降级路径。

### 3.4 阿里百炼 DashScope（分类 provider，必需）

v1 分类引擎走 OpenAI 兼容协议，百炼正好提供兼容端点，且用户已有账号（不再单独开 OpenAI key）。

1. 打开 <https://bailian.console.aliyun.com/> 登录并开通模型服务。
2. 「API-KEY 管理」→ 创建 API Key（按应用命名，如 `aetherhud-classifier`）。
3. 端点：国内 `https://dashscope.aliyuncs.com/compatible-mode/v1`；国际 `https://dashscope-intl.aliyuncs.com/compatible-mode/v1`。两个都实测存在（无 key 返回 401 的 OpenAI 兼容错误体，2026-10-21）。
4. 模型：分类是「短文本 → 多选映射」，用便宜快的小模型（`qwen-flash` 一类）即可；最终型号由 #21 评测结果定（#14 要求同条件基线，换模型要重跑 200 条金标）。
5. 计费：按 token；请求体很小（taxonomy 注入 + 正文 ≤500 字），单条成本可忽略。不要用大模型做分类（慢且贵）。

注意：HUD 只用 OpenAI 兼容端点，不引入 DashScope 原生 SDK。

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
| 百炼 | `GET /compatible-mode/v1/models`（不支持时退一次 `chat/completions`） | 200 且返回模型列表或有 choices | key 有效 + 区域端点正确 |

产物：控制台摘要（英文，避免 Windows 控制台乱码）+ `.local/keys-report.json`（实测结果，**不含密钥明文**，可直接贴到 issue）。退出码 0 = 全过。

### 4.1 `.local/keys.env` 模板

```
DASHSCOPE_API_KEY=
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
| 阿里百炼 DashScope | 按输入/输出 token 计费（新用户有赠送额度）；分类单条消耗可忽略 | 百炼控制台 / 官方定价页 |

## 6. 已知风险

| 风险 | 影响 | 应对 |
|---|---|---|
| 飞书个人版没有团队 | 无法进开发者后台 | 自建免费团队，自己当管理员（官方支持） |
| 飞书应用没加进 Base | 全部接口 91403 | checklist §3.1 第 6 步 |
| 飞书改了权限没发版 | 权限不生效 | 每次改权限都重新「创建版本 → 发布」 |
| Jina 域名在用户网络不可达 | L2 抓取路径不可用 | v1 默认 L1 直连；配 §7 代理后再验证 |
| B 站接口 412 风控 | 视频元数据拿不到 | UA + buvid3 指纹 + 低频 + 退避（#2 调研）；实现时实测验证 |
| B 站字幕接口不稳定 | 少一层分类信号 | 字幕是增强项，失败就降级到「简介 + 置顶/热评」 |
| 百炼 key 泄露 | 别人刷你账单 | Keychain 存储 + 控制台限额/告警 |
| 视频元数据只有标题 | 标题党视频分类偏差 | #14 评测会暴露；如果准确率不达标再补 YouTube key 或字幕 |

## 7. 网络代理（PRD v2 设置项）

用户环境（2026-10-21 确认）：本机开全局代理可用，但应用不能依赖系统代理设置，设置页要能自己指定。

设置 →「网络」分区三档，默认「跟随系统」：

| 档位 | 行为 |
|---|---|
| 跟随系统 | Windows 读 WinHTTP/WinINet 代理；macOS 读系统代理（reqwest 的系统代理探测） |
| 自定义代理 | 用户填 `http://127.0.0.1:10809` 这类地址；所有出站请求强制走它 |
| 直连 | 忽略系统代理，永远直连（内网/调试） |

- 覆盖范围：摄取抓取（Jina / 网页 / 视频元数据）、分类调用（百炼）、同步轮询（飞书）、更新检查——所有出站 HTTP。
- 代理地址不敏感，存 settings；带认证时用户名密码进 Keychain（`proxy.password`）。
- 冲突提示：百炼/飞书在国内直连更快，全局走代理会绕远。v1 先做「一档全局」，域名旁路（国内域名走直连）留作 v2 优化，不阻塞。

## 8. 实测记录（2026-10-21）

跑法：`node scripts/setup/verify-keys.mjs`（Jina 需代理：`HTTPS_PROXY=http://127.0.0.1:10809` + `NODE_USE_ENV_PROXY=1`）。

| 项 | 结果 | 实测数据 |
|---|---|---|
| 飞书自建应用 | ✅ | token 有效期 7200s；Base `K1odbfYaxarVMFsgyq0cYcOFnNf`；table `tblKbNrOt5WcPdKP`；字段=文本/已读/类别/添加日期；total=341 |
| 阿里百炼 | ✅ | `/models` 200、178 个模型、150ms；端点 `https://dashscope.aliyuncs.com/compatible-mode/v1` |
| Jina Reader | ✅（代理下） | 200 / 657ms；`x-ratelimit-limit: 500, 500;w=60`、`remaining: 499` → 免费档 500 RPM |
| YouTube | ⏸ | 未配置；oEmbed 兜底（§3.2） |

注意：Node 24 在 `NODE_USE_ENV_PROXY=1` 下退出时可能打印 `Assertion failed: !(handle->flags & UV_HANDLE_CLOSING)`（libuv/undici 的已知噪音），不影响探针结果与 report 生成。

结论：凭证全部就绪，v1 的分类（百炼）、同步（飞书）、抓取（Jina，代理下）链路均可跑通；Jina 与 YouTube 属降级路径，不影响主流程。
