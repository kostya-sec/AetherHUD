# Jina Reader（r.jina.ai）现状调研与抓取管线设计

> 调研日期：2026-10-13（以最近 3 个月资料为准，官方文档实时抓取）
> 调研人：后台调研子代理（issue #3）
> 范围：免费层限速 / 延迟特征 / 自托管 / 备选方案 / 抓取管线推荐

---

## 0. 背景事件：Jina AI 已被 Elastic 收购

Elastic（NYSE: ESTC）于 **2025-10-09 完成**对 Jina AI 的收购。目前 r.jina.ai / s.jina.ai 服务仍在正常运营，定价与限速表在 2026-10 仍有效；但 ReaderLM-v2 / jina-vlm 模型的**商用授权自 2026-08-10 起改由 Elastic 单独销售**（模型本身为 CC-BY-NC 4.0，非开源许可证）。长期看服务策略存在不确定性，架构上应把 Jina Reader 当作「可替换的外部依赖」而非硬绑定。

来源：[Elastic IR 新闻稿](https://ir.elastic.co/News--Events/news/news-details/2025/Elastic-Completes-Acquisition-of-Jina-AI-a-Leader-in-Frontier-Models-for-Multimodal-and-Multilingual-Search/default.aspx)、[jina.ai/reader FAQ](https://jina.ai/reader#faq)

---

## 1. 免费层限速与定价（官方表格，2026-10-13 实时抓取）

官方页面同时给出 RPM（每 IP/API key 每分钟请求数）与官方平均延迟：

| 产品 | 端点 | 无 API Key | 免费 API Key | 付费 Key（$50 档） | Premium Key（$500 档） | 官方平均延迟 | 计费方式 |
|---|---|---|---|---|---|---|---|
| Reader | `r.jina.ai` | **20 RPM** | 500 RPM | 500 RPM | **5000 RPM** | **7.9 s** | 按**输出** token 计数 |
| Search | `s.jina.ai` | **不可用（block）** | 100 RPM | 100 RPM | 1000 RPM | 2.5 s | 每请求固定 ≥10,000 token |
| Embedding | api.jina.ai | 不可用 | 100 RPM & 100K TPM | 500 RPM & 2M TPM | 5000 RPM & 50M TPM | 取决于输入 | 按输入 token |
| Reranker | api.jina.ai | 不可用 | 100 RPM & 100K TPM | 500 RPM & 2M TPM | 5000 RPM & 50M TPM | 取决于输入 | 按输入 token |

充值档位（token 型，非订阅）：

| 档位 | token 数 | 价格 | 单价 | 备注 |
|---|---|---|---|---|
| Toy Experiment | 10M | **免费** | — | 仅限非商用（CC-BY-NC） |
| Prototype Development | 1B | $50 | $0.050 / 1M | Standard key |
| Production Deployment | 11B | $500 | $0.045 / 1M | Premium key，更高限速 |

要点：
- 新定价模型自 **2025-05-06** 起生效；token 余额可设置自动充值。
- 有 key 与无 key 的差别主要在**信任池与配额**：官方 README 明确说匿名流量「被最激进地限速、落入最低信任池」，付费 key 还解锁托管代理池（`x-proxy: auto`）。
- 对 AetherHUD（桌面端、个人用户各自粘贴 URL 的场景）：**每用户 20 RPM 无 key 已完全够用**——单个用户手动保存 URL 的频率远低于此；建议不内置任何共享 key（共享 key 反而会被 500 RPM 上限打爆），让用户可选填自己的 key。

来源：[jina.ai/reader#pricing（实时抓取）](https://jina.ai/reader#pricing)、[jina-ai/reader README](https://github.com/jina-ai/reader)

---

## 2. 延迟特征：PRD 的「4 秒超时」不现实

- 官方公布的 r.jina.ai **平均延迟为 7.9 秒**（浏览器引擎渲染占大头）。FAQ 说明：direct/curl 引擎抓简单页面通常几百毫秒；默认 headless Chrome 引擎「通常在低个位数秒」；重 SPA、慢源站、大 PDF 更慢。
- 缓存：同一 URL 在缓存窗口内重复请求几乎即时返回（FAQ 说 5 分钟窗口；README 提到缓存寿命 3600s，可用 `x-cache-tolerance` / `x-no-cache` 控制——两处数字不一致，以实测为准）。
- 本次实测（2026-10-13，经代理，无 key，3 个样本）：
  - `example.com` → 200，321 B，**1.4 s**
  - `en.wikipedia.org/wiki/Tauri_(computing)` → 200，7 KB，**2.2 s**
  - `github.com/jina-ai/reader` → 200，26 KB，**0.9 s**
  - 均为热 URL（命中缓存），冷抓取 SPA 页面落入官方 7.9 s 均值区间属正常。

**结论：4 秒硬超时（AGENTS.md / PRD 中「timeout <= 4s」）会误杀大量冷抓取。**建议改为分级超时（见 §5）。

来源：[jina.ai/reader FAQ](https://jina.ai/reader#faq)、[rate-limit 表](https://jina.ai/reader#rate-limit)、本地实测

---

## 3. 自托管：jina-ai/reader 开源版

- 仓库 [jina-ai/reader](https://github.com/jina-ai/reader)（Apache-2.0）是 SaaS 代码的开源分支，**2026-04 刚与 SaaS 代码重新同步**。预构建镜像：`ghcr.io/jina-ai/reader:oss`（内含 headless Chrome、LibreOffice、CJK 字体）。
- 端口：8080（h2c，生产用）/ 8081（HTTP/1.1，curl 调试用）。默认完全无状态：无缓存、无限速；可选挂 S3/MinIO 桶做页面缓存（`docker compose` 提供 MinIO）。
- 与 SaaS 的能力差异：
  1. **不含 MongoDB 存储层**（SaaS 专属，OSS 只能无状态/桶缓存）；
  2. **不含托管代理池**（`x-proxy: auto` 需 key，OSS 只能 `x-proxy-url` 自带代理）；
  3. **无内置限速/鉴权/计费**（要自己加前置）；
  4. 构建需下载受许可资产（GeoLite2 mmdb、Source Han Sans 字体、UA 列表，`npm run assets:download`）；
  5. 内置模型 ReaderLM-v2 / jina-vlm 为 **CC-BY-NC**，商用需向 Elastic 购买授权。
- 部署门槛：headless Chrome + LibreOffice 使镜像较重，适合「家里/公司有一台常开机器」的高级用户，**不适合随 Tauri 桌面应用分发**。对 AetherHUD 而言自托管只能作为可选高级项，不能作为主路径。

来源：[reader README §Self-host with Docker / Local development](https://github.com/jina-ai/reader)、[jina.ai/reader FAQ](https://jina.ai/reader#faq)

---

## 4. 备选方案对比（Rust/Tauri 桌面应用视角）

| 方案 | 语言/形态 | 在 Tauri 中的集成方式 | 许可证 | 维护现状 | 适配度评估 |
|---|---|---|---|---|---|
| **dom_smoothie** | Rust crate（readability.js 的 Rust 移植） | 直接 `cargo add`，与 reqwest 抓回的 HTML 在 Rust 侧解析 | MIT | 活跃：v0.18.2（2026-09-21），累计下载 63 万+ | **首选本地降级**：纯 Rust、零外部进程；仅处理静态 HTML，不跑 JS |
| **Mozilla Readability.js** | JS 库 | Tauri 前端 webview 里直接跑（`@mozilla/readability` + DOMParser），HTML 由 Rust 侧 reqwest 抓取后送入 | Apache-2.0 | 稳定：v0.6.0（2025-03），Mozilla 官方 | 可行但跨 IPC 传大 HTML 有开销；与 dom_smoothie 二选一即可，不必都用 |
| **Defuddle** | JS/TS 库（Obsidian Web Clipper 出品） | 同 Readability.js（webview/Node DOM 实现）；自带 Markdown 输出与更完整元数据（schema.org） | MIT | 活跃：v0.19.5（2026 年持续更新）；官方自称 WIP | 输出质量与元数据优于 Readability，但 Rust 侧无移植，只能放前端；适合做「前端增强降级」 |
| **trafilatura** | Python 库+CLI | 需捆绑 Python 运行时或调用外部进程 | Apache-2.0 | 非常活跃：v2.3.1（2026-10-06） | **不推荐**：Tauri 桌面应用内嵌 Python 代价过大，破坏「ultra-lightweight」目标 |
| **Firecrawl Cloud** | SaaS API | reqwest 调 HTTP API | 商业 | 活跃 | 可作 Jina 的**平级 SaaS 备选**：免费档 1000 credits/月（≈1000 页）、2 并发浏览器、10 req/min；Hobby $19/月 5000 credits。免费额度对重度用户偏小 |
| **Firecrawl 自托管** | Docker Compose 栈（API + Playwright + Redis 等） | 本地服务，HTTP 调用 | 需查各组件 | 活跃 | 核心 scrape/crawl/map/search 开源可用，但 LLM 抽取、反爬代理、Agent/Browser 等高级面为 Cloud 专属；运维成本与 jina reader 自托管同级，不适合桌面分发 |
| **jina-ai/reader OSS** | 单容器 Docker | 本地服务，HTTP 调用 | Apache-2.0（代码） | 活跃，2026-04 与 SaaS 同步 | 见 §3，高级用户可选项 |

补充：Rust 生态另有老牌 `readability` crate（v0.3.0，2023-12 后未更新），已明显落后于 dom_smoothie，不建议采用。

来源：[dom_smoothie GitHub](https://github.com/niklak/dom_smoothie) / [crates.io](https://crates.io/crates/dom_smoothie)、[@mozilla/readability npm](https://www.npmjs.com/package/@mozilla/readability)、[defuddle GitHub](https://github.com/kepano/defuddle) / [npm](https://www.npmjs.com/package/defuddle)、[trafilatura PyPI](https://pypi.org/project/trafilatura/)、[Firecrawl Pricing（2026-09-04 生效）](https://www.firecrawl.dev/pricing)、[Firecrawl OSS vs Cloud 文档](https://docs.firecrawl.dev/contributing/open-source-or-cloud)

---

## 5. 产出：抓取管线推荐配置与降级链

### 5.1 推荐配置（src-tauri/src/parser/）

| 参数 | 推荐值 | 理由 |
|---|---|---|
| 直连抓取超时 | **8 s**（connect 5 s + body 8 s） | 大多数站点直连足够；过短误杀慢站 |
| Jina Reader 超时 | **20 s**（并传 `x-timeout: 15` 给 Jina 自身） | 官方均值 7.9 s，4 s 会系统性失败；Jina `x-timeout` 上限 180 s，15 s 覆盖绝大多数页面又不至于拖垮 UX |
| 整体管线预算 | **30 s**，异步执行不阻塞 UI | 保存操作先入 Inbox（pending），抓取在后台完成，与 AGENTS.md fail-safe 流程一致 |
| 并发 | **3**（tokio Semaphore） | 桌面单用户场景无需更高；无 key 20 RPM 上限下 3 并发不会触限 |
| 重试 | **最多 2 次**，指数退避 1s→4s，仅对网络错误/429/5xx 重试；4xx（除 429）不重试 | 429 时读取 `Retry-After` 并降级到下一级而非死等 |
| 缓存 | SQLite 存抓取快照，同 URL 24h 内复用 | 避免重复消耗 Jina 配额，也利用其 5min/3600s 服务端缓存 |
| Jina 请求头 | 默认引擎（auto）；`x-respond-with: markdown` 无需显式（默认即 readability 过滤）；可设 `x-retain-images: alt`、`x-retain-links: text` 省 token | 分类器只需要文本信号 |
| API key | 可选设置项，用户自填；不内置共享 key | 见 §1 要点 |

### 5.2 降级链（fail-safe，逐级回退）

```
URL 输入
  │
  ├─ L0 元数据特判（零抓取）
  │    Bilibili/YouTube 等视频站 → 走专用 metadata scraper（title/UP主/简介）
  │    .pdf 结尾 → 标记类型，交给 L2 Jina（原生支持 PDF）
  │
  ├─ L1 直连抓取 + 本地解析（默认路径，~1-3s）
  │    reqwest（浏览器 UA、gzip）→ dom_smoothie 提取正文+元数据
  │    成功（正文 ≥ 200 字符）→ 进分类器
  │
  ├─ L2 Jina Reader r.jina.ai（L1 失败/正文过短/已知 SPA 域名，≤20s）
  │    可选带用户自有 key；429 → 等 Retry-After 重试 1 次
  │
  ├─ L3 用户可选增强（互斥、默认关）
  │    a) 用户自托管 reader:oss（docker，localhost:8081）
  │    b) Firecrawl Cloud key
  │
  └─ L4 保底（全部失败）
       仅存 URL + <title>（L1 即使正文失败也常拿到 title）+
       HTTP 状态/错误摘要 → status="pending_fallback"，
       分类器走默认 (l1: "00_未分类", l2: "待人工归档")，
       Tauri Event 通知前端标记「抓取失败，可重试」
```

设计要点：
1. **L1 优先于 Jina**：本地直连对中文主流站点（知乎/微信公号以外的静态内容）通常 1–2 s 完成，不消耗任何外部配额，也无隐私外泄；Jina 只用于直连失败的硬骨头（JS 渲染站、反爬站）。
2. **超时预算与 UX 解耦**：保存动作即时返回（入 Inbox pending），抓取+分类全部异步；前端用 pending 态卡片展示，彻底回避「4 秒超时」级别的 UX 压力。
3. **Jina 为可替换依赖**：Elastic 收购后服务策略可能变化，parser 层应对 `ParserBackend` 做 trait 抽象（direct / jina / firecrawl / self-hosted），配置驱动切换。
4. **不要在应用内打包 headless 浏览器或 Python**：与 ultra-lightweight 目标冲突；JS 渲染需求全部外包给 L2/L3。

---

## 附：来源清单

- https://jina.ai/reader （定价/限速表/FAQ，2026-10-13 实时抓取）
- https://github.com/jina-ai/reader （README：OSS 能力、自托管、header 参数）
- https://ir.elastic.co/News--Events/news/news-details/2025/Elastic-Completes-Acquisition-of-Jina-AI-a-Leader-in-Frontier-Models-for-Multimodal-and-Multilingual-Search/default.aspx
- https://github.com/niklak/dom_smoothie · https://crates.io/crates/dom_smoothie
- https://www.npmjs.com/package/@mozilla/readability
- https://github.com/kepano/defuddle · https://www.npmjs.com/package/defuddle
- https://pypi.org/project/trafilatura/ · https://github.com/adbar/trafilatura
- https://www.firecrawl.dev/pricing · https://docs.firecrawl.dev/contributing/open-source-or-cloud
