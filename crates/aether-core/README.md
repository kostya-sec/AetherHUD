# aether-core（应用核心库，#27 / #28）

> 规格：[AetherHUD v1 实现规格 #24](https://github.com/kostya-sec/AetherHUD/issues/24) §1 / §2 / §3；
> [taxonomy-evolution.md](../../docs/spec/taxonomy-evolution.md)；[sqlite-schema.md](../../docs/spec/sqlite-schema.md)；
> [pipeline-worker.md](../../docs/spec/pipeline-worker.md)；ADR-0001/0002/0003。

本 crate 是**领域行为的唯一边界**：存储与迁移、本地录入与去重、收件箱分页、分类树合并与
生效树导出、HUD 导航数据、可注入适配器。它不依赖 Tauri、不直接摸系统 API；壳把
app_data_dir、底表路径、时钟与适配器注入进来。

## 模块

| 模块 | 职责 |
|---|---|
| `adapters` | `Clock` / `FileSystem` / `HttpTransport` / `Keychain` 四个 trait + 系统实现；`adapters::testing` 提供内存夹具 |
| `storage` | SQLite 打开（WAL / synchronous=NORMAL / foreign_keys=ON / busy_timeout=5000）+ `rusqlite_migration`；**migration v1 = sqlite-schema §3–§4 全表**（entries / tags / tag_aliases / entry_tags / entry_sources / corrections / pipeline_jobs / settings / meta / FTS5 + 触发器 + 索引）；`meta.taxonomy_version` 读写；本地录入的**双轨去重 + 条目/来源/队列同事务写入**；收件箱 keyset 分页 |
| `url` | canonical URL 的唯一实现：scheme/host 小写、去默认端口 / fragment / 跟踪参数、非根路径去末尾斜杠、query 按 (key,value) 排序；BLAKE3 前 128 bit 作 `url_hash` |
| `ids` | 对外 `uid` 的 ULID（48 bit 时间戳 + 80 bit 随机数，Crockford Base32） |
| `ingest` | 本地录入计划：URL → fetch stage、纯文本 → classify stage；纯文本落 `body_md`(≤1500) 与 `excerpt`(≤500)；fail-safe 分类 `l1_inbox/l2_manual_review` |
| `inbox` | 收件箱视图 DTO：`pending/processing/failed` 管线视图、50 条/页、`(created_at, id)` keyset 游标 |
| `search` | FTS5 查询构造（#10 §9）：切段 + 加引号 + AND，防语法注入；<3 字符走 LIKE 兜底 |
| `taxonomy` | 底表 ⊕ 覆盖层合并：读底表、读/建覆盖层、校验、合并、原子导出 `taxonomy.effective.json`、覆盖层损坏备份 + 回退；`load_exported_view` 供评测只读加载 |
| `navigation` | 从 `TaxonomyView` 派生 HUD 球列/L2 面板 DTO（5 个 L1 + 子类 + 形态标签） |
| `notice` | 启动期 HUD 提示（覆盖层损坏等），前端显示 5 秒提示条、不抢焦点 |
| `paths` | app_data_dir 解析：显式参数 > `AETHER_DATA_DIR` > 平台数据目录 / identifier |

## 本地录入与收件箱（#28）

`Core::ingest_text(text, now_ms)` 是本地录入的唯一入口（时间由壳侧时钟适配器传入）：

1. `ingest::plan_manual` 判定输入：能归一化为 http(s) URL → fetch stage（media_type 先记
   `link`，视频元数据由 #33 的抓取链回写）；否则纯文本 → classify stage（`article`，
   正文与摘录入库）。
2. `storage::Database::ingest` 在**一个事务**里完成去重与落库：
   - 轨道 1：`entry_sources(source_type, source_ref)` 命中 → 只更新条目与来源的
     `last_seen_at`；
   - 轨道 2：`entries.url_hash`（canonical URL 的 BLAKE3 前 128 bit）命中 → 只更新
     `last_seen_at`，并写入本次投递的来源行；
   - 都未命中 → INSERT `entries` + `entry_sources` + `pipeline_jobs`（`queued`，
     `next_attempt_at = now`，attempts=0）。
3. 壳的 `ingest_text` IPC 命令在成功后推 `hud://inbox-updated`；前端收件箱打开时据此
   重载第一页。

收件箱是**管线视图**（`pipeline_status ∈ pending/processing/failed`），不是分类节点：
新条目立刻以「分类中」出现，分类成功（`classified`）后离开。分页固定 50 条/页，
`ORDER BY created_at DESC, id DESC`，游标 = 上一页最后一行 `(created_at, id)`。

## 启动流程（`Core::bootstrap`）

1. 读只读底表 `taxonomy.json`；
2. 在 `app_data_dir` 下读 `taxonomy.overlay.json`：缺失则创建空覆盖层（`revision = 0`）；
   损坏（JSON 非法或校验失败）则备份为 `taxonomy.overlay.corrupt-<ts>.json`、以空覆盖层
   继续，并产出一条 HUD 提示；
3. 合并为生效分类树（版本 `<底表版本>+u<revision>`），原子写 `taxonomy.effective.json`；
4. 打开 `aether.db`、跑 v1 迁移，把生效版本写入 `meta.taxonomy_version`；
5. 生成 `HudNavigation` 供 `hud_navigation` IPC 使用，启动提示由 `hud_take_notices` 取走。

> 迁移前 VACUUM INTO 备份（sqlite-schema §11）与 taxonomy 失效引用重映射（§10 第 2b/2c 步）
> 随 #43/#41 接入；本票保证空库 → v1 迁移、PRAGMA 与 user_version 正确。
>
> v1 尚未发布，因此 #28 把 §3 全表并入 migration v1（#27 只落过 meta 的临时开发库
> 没有用户数据，删除旧开发库后即可重建）；发布后迁移只追加、不再修改。
>
> 官方节点与自建节点撞名的持久化对账属于 #41；本票在合并时做只读投影。

## 唯一实现约束

- 「底表 ⊕ 覆盖层 → 生效树」只有本 crate 一份；`aether-classifier` 只做「文档 → 视图」，
  不读文件；`aether-eval` 用 `taxonomy::load_exported_view` 读应用导出的生效树。
- **URL 归一化只有一份实现**（`url` 模块）：去重、未来的打开链接与导出都复用它。
- 覆盖层、生效树的文件名与版本语义集中在 `taxonomy` 常量里，壳/CLI 不各自拼路径。

## 测试

```bash
cargo test -p aether-core
```

覆盖：空库 → v1 迁移（全表/索引/触发器/user_version）、PRAGMA（WAL/synchronous/foreign_keys/
busy_timeout）、FTS 增删改同步、<3 字符 LIKE 兜底（通配符转义）、FTS5 语法注入不改语义、
双轨去重（来源外部 id / canonical URL，命中只更新 last_seen_at）、URL 归一化、
录入事务回滚、收件箱 50 条/页 keyset 分页、ULID 编码与已知向量、分类树合并与 HUD 导航。
领域测试用 `MemoryFileSystem` + `FakeClock`，存储测试用临时目录/内存库。
