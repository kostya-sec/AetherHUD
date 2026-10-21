# SQLite 数据模型与 Schema 规格（v1）

> 状态：已确认（2026-10-20）
> 来源票：[决策：SQLite 数据模型与 schema 设计](https://github.com/kostya-sec/AetherHUD/issues/10)
> 依赖决议：#8（生命周期状态机与操作集）、#9（L3 标签治理）、#3（抓取降级链与快照缓存）、#5（云端同步 API 调研）
> 本文件是 `src-tauri/src/storage/` 的施工依据；实现时 DDL 与迁移内容以本文件为准。

## 1. 范围与非目标

**范围**：本地 SQLite 的 v1 schema——表、索引、触发器、FTS5 配置、迁移与备份策略、taxonomy 版本迁移流程、检索约定、容量旋钮。

**非目标**（本票明确不做，见 §14）：云端同步的本地存储（`sync_sources` / 增量游标，仅本地）、跨机迁移与导出格式、分类运行日志表、多语言展示。

## 2. 存储约定

| 约定 | 规则 |
|---|---|
| 时间 | `INTEGER` Unix epoch **毫秒 UTC**（前端 `Date` 零转换）；展示时区由设置键 `display.timezone` 决定，**默认 `Asia/Shanghai`** |
| 枚举 | `TEXT + CHECK (...)`；不用整数枚举 |
| 布尔 | `INTEGER 0/1` + `CHECK (col IN (0,1))` |
| JSON | **仅限不可查询快照**：`corrections.before_value/after_value`、`settings.value_json`、`entries.classify_suggestion`（#18）。任何参与过滤/排序的字段禁止进 JSON |
| 主键 | 业务表 `INTEGER PRIMARY KEY AUTOINCREMENT`（防 id 复用）；`entries.uid` 存 ULID 作对外标识（导出/调试 JSON/未来跨机迁移） |
| 字符 | UTF-8（SQLite 默认） |

**连接 PRAGMA**（每个连接打开时执行）：

```sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 5000;
```

**数据目录**：Tauri `app_data_dir`（Windows `%APPDATA%\<identifier>\`，macOS `~/Library/Application Support/<identifier>/`）下：

```text
app_data_dir/
├── aether.db                     # 主库（WAL/SHM 同目录）
├── taxonomy.overlay.json         # 自建 L2 覆盖层（#18；随 DB 一并备份）
├── taxonomy.effective.json       # 底表 ⊕ 覆盖层的合并结果（#18；只写，供调试与评测工具读取）
└── backups/                      # 迁移前自动备份（VACUUM INTO + 覆盖层快照）
```

## 3. 表结构（migration v1 的完整 DDL）

### 3.1 entries（条目）

```sql
CREATE TABLE entries (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  uid             TEXT    NOT NULL UNIQUE,              -- ULID
  title           TEXT    NOT NULL,
  url             TEXT,                                  -- 原始 URL（打开链接用）
  canonical_url   TEXT,                                  -- 归一化 URL（去 utm/fragment/尾斜杠）
  url_hash        BLOB,                                  -- BLAKE3(前 128 bit)
  media_type      TEXT    NOT NULL DEFAULT 'link'
                          CHECK (media_type IN ('link','video','article')),
  author          TEXT,
  l1_id           TEXT    NOT NULL,                      -- taxonomy L1 id（如 l1_tech）
  l2_id           TEXT    NOT NULL,                      -- taxonomy L2 id
  body_md         TEXT,                                  -- 抓取正文，上限 1500 字符（分类上下文 + 本地留存）
  excerpt         TEXT,                                  -- body_md 前 500 字符，专供 FTS
  notes           TEXT,                                  -- 用户笔记（预留，UI 未开放前恒 NULL）
  pipeline_status TEXT    NOT NULL DEFAULT 'pending'
                          CHECK (pipeline_status IN ('pending','processing','classified','failed')),
  failure_reason  TEXT    CHECK (failure_reason IN ('fetch_failed','classify_failed')),
  metadata_status TEXT    NOT NULL DEFAULT 'ok'
                          CHECK (metadata_status IN ('ok','partial','failed')),
  lifecycle       TEXT    NOT NULL DEFAULT 'active'
                          CHECK (lifecycle IN ('active','archived','trashed')),
  pinned          INTEGER NOT NULL DEFAULT 0 CHECK (pinned IN (0,1)),
  done            INTEGER NOT NULL DEFAULT 0 CHECK (done IN (0,1)),
  corrected_by    TEXT    CHECK (corrected_by IN ('ai','human')),
  classify_suggestion TEXT,                              -- JSON 快照：弃权时的分类建议（#18）；非弃权或已改分类为 NULL
  fetch_source    TEXT    CHECK (fetch_source IN ('dom_smoothie','jina','video_api','none')),
  fetched_at      INTEGER,
  trashed_at      INTEGER,
  created_at      INTEGER NOT NULL,
  updated_at      INTEGER NOT NULL,
  last_seen_at    INTEGER NOT NULL
);
```

字段语义（与 #8 决议的映射）：

- `pipeline_status`：系统处理进度轴；`failed` 必须配 `failure_reason`，可重试（重新入队 → `pending`）。
- `metadata_status`：元数据完整度，正交于 pipeline；L4 保底 = `classified + partial`。
- `lifecycle`：用户可见性轴；`trashed` 必须配 `trashed_at`，30 天后硬删。
- `pinned` / `done` / `corrected_by`：正交标记，不进状态机。
- `l1_id` / `l2_id`：**只存 id，不存显示名**（显示名从内存 taxonomy 解析；否定 demo 的冗余 name 方案）。
- `classify_suggestion`：分类建议的 JSON 快照（#18，见 [taxonomy-evolution.md](./taxonomy-evolution.md) §4）；仅弃权条目非空，任何 L1/L2 变更即清空。

### 3.2 tags（L3 标签实体，全局唯一）

```sql
CREATE TABLE tags (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  kind            TEXT    NOT NULL CHECK (kind IN ('format','feature')),
  origin          TEXT    NOT NULL CHECK (origin IN ('format','preset','ai','user')),
  display_name    TEXT    NOT NULL,
  normalized_key  TEXT    NOT NULL,
  color           TEXT,
  is_resident     INTEGER NOT NULL DEFAULT 0 CHECK (is_resident IN (0,1)),
  is_hidden       INTEGER NOT NULL DEFAULT 0 CHECK (is_hidden IN (0,1)),
  deleted_at      INTEGER,                               -- 软删墓碑（防预设 seed 复活）
  created_at      INTEGER NOT NULL,
  updated_at      INTEGER NOT NULL
);
CREATE UNIQUE INDEX ux_tags_normalized_key ON tags (normalized_key);
```

- `kind='format'`：5 个形态标签（工具/教程/提示词/工作流/素材），由 taxonomy `format_tags` 启动时 upsert，`origin='format'` 系统级保护（不可删改，恒显）。
- `kind='feature'`：特征标签；`origin` 记录创建来源（`preset`=L2 预设种子、`ai`=分类器、`user`=用户输入）。
- `normalized_key` 唯一且**包含软删行**；用户删除的标签留下的墓碑行，seed 时跳过；用户再次输入同名标签时**复活**该行（清空 `deleted_at`，id 稳定，别名保留）。
- 常驻/隐藏是标签级覆写（#9）：可见性 =（active 使用数 ≥ 2）∨ `is_resident=1`，且 `is_hidden=0`——**查询时计算，不存计数**。

### 3.3 tag_aliases（别名表）

```sql
CREATE TABLE tag_aliases (
  alias_normalized TEXT PRIMARY KEY,
  tag_id           INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  alias_display    TEXT    NOT NULL,
  created_at       INTEGER NOT NULL
);
CREATE INDEX ix_tag_aliases_tag ON tag_aliases (tag_id);
```

别名全局唯一、指向唯一标签；标签合并时追加 `源名 → 目标名`；标签删除（CASCADE）时别名一并消失。

### 3.4 entry_tags（条目-标签关联）

```sql
CREATE TABLE entry_tags (
  entry_id    INTEGER NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
  tag_id      INTEGER NOT NULL REFERENCES tags(id)   ON DELETE CASCADE,
  assigned_by TEXT    NOT NULL CHECK (assigned_by IN ('ai','human')),
  created_at  INTEGER NOT NULL,
  PRIMARY KEY (entry_id, tag_id)
) WITHOUT ROWID;
CREATE INDEX ix_entry_tags_tag ON entry_tags (tag_id, entry_id);
```

### 3.5 entry_sources（摄取来源，多来源一条目）

```sql
CREATE TABLE entry_sources (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  entry_id    INTEGER NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
  source_type TEXT    NOT NULL,   -- manual|shortcut|webhook:telegram|webhook:feishu|webhook:dingtalk|webhook:wecom
  source_ref  TEXT    NOT NULL,   -- 平台消息 ID / 外部 record_id / 手动输入生成的 ULID
  ingested_at INTEGER NOT NULL,
  last_seen_at INTEGER NOT NULL
);
CREATE UNIQUE INDEX ux_entry_sources_ref ON entry_sources (source_type, source_ref);
CREATE INDEX ix_entry_sources_entry ON entry_sources (entry_id);
```

`source_type` 是**开放集合**（渠道会持续增加），故不加 CHECK，取值以本文件 §8 为准。云端渠道（`cloud:feishu` / `cloud:gsheets`）的写入随云端同步功能落地启用。

### 3.6 corrections（纠偏记录，匿名保留）

```sql
CREATE TABLE corrections (
  id               INTEGER PRIMARY KEY AUTOINCREMENT,
  entry_id         INTEGER REFERENCES entries(id) ON DELETE SET NULL,  -- 硬删后置 NULL，匿名保留
  dimension        TEXT    NOT NULL
                           CHECK (dimension IN ('l1','l2','tag_add','tag_remove','media_type')),
  before_value     TEXT,   -- JSON 快照，如 {"id":"l1_tech","name":"03 技术与工程"}
  after_value      TEXT,
  trigger          TEXT    NOT NULL CHECK (trigger IN ('manual_edit','reclassify')),
  taxonomy_version TEXT,
  created_at       INTEGER NOT NULL
);
CREATE INDEX ix_corrections_entry   ON corrections (entry_id, created_at DESC);
CREATE INDEX ix_corrections_created ON corrections (created_at);
```

- 一条标签增删 = 一行（`tag_add` / `tag_remove`），before/after 存标签名/id 快照。
- **只记分类维度**：notes/title 编辑不入表（避免用户正文进入 prompt 样本）。
- 硬删条目后 `entry_id` 置 NULL，其余字段即匿名样本，供 #14 评测与 prompt 优化使用。

### 3.7 pipeline_jobs（执行队列，形状归本票；策略归 #11）

```sql
CREATE TABLE pipeline_jobs (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  entry_id        INTEGER NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
  stage           TEXT    NOT NULL CHECK (stage IN ('fetch','classify')),
  state           TEXT    NOT NULL DEFAULT 'queued' CHECK (state IN ('queued','running')),
  attempts        INTEGER NOT NULL DEFAULT 0,
  next_attempt_at INTEGER NOT NULL,
  lease_until     INTEGER,                               -- 崩溃恢复扫描用
  last_error      TEXT,
  created_at      INTEGER NOT NULL,
  updated_at      INTEGER NOT NULL,
  UNIQUE (entry_id, stage)
);
CREATE INDEX ix_pipeline_jobs_ready ON pipeline_jobs (next_attempt_at) WHERE state = 'queued';
CREATE INDEX ix_pipeline_jobs_lease ON pipeline_jobs (lease_until)     WHERE state = 'running';
```

- 同一 `entry_id` 的 fetch 与 classify 是两行（串行插入），故唯一约束是 `(entry_id, stage)`。
- `entries.pipeline_status` 保持为**对外状态**；jobs 表承担重试计数/下次时间/租约。状态派生、退避、断网挂起与重启恢复语义见 [pipeline-worker.md](./pipeline-worker.md)（#11 决议），实现须在同一事务里更新两处。

### 3.8 settings（非机密配置）与 meta（内部元数据）

```sql
CREATE TABLE settings (
  key        TEXT PRIMARY KEY,
  value_json TEXT NOT NULL,
  updated_at INTEGER NOT NULL
) WITHOUT ROWID;

CREATE TABLE meta (
  key        TEXT PRIMARY KEY,
  value      TEXT NOT NULL,
  updated_at INTEGER NOT NULL
) WITHOUT ROWID;
```

- `settings` 默认键（v1）：`display.timezone` = `"Asia/Shanghai"`、`ui.hotkey`、`ui.theme_hue`、`webhook.path`、`classifier.provider`、`classifier.model`、`classifier.prompt_version`、`classifier.api_key_ref`、`window.position`。**API Key 本体只存 OS Keychain**，DB 只存条目引用名。
- `meta` 键（v1）：`taxonomy_version`（taxonomy.json 的 `version`）、`schema_created_at`、`last_update_check_at`（最近一次更新检查，Unix ms；见 [distribution-and-update.md](./distribution-and-update.md)）、`ignored_update_version`（被用户忽略的版本号）。schema 版本用 `PRAGMA user_version`（rusqlite_migration 管理），不入 meta。

### 3.9 fts_entries（全文索引，external content）

```sql
CREATE VIRTUAL TABLE fts_entries USING fts5(
  title,
  excerpt,
  notes,
  author,
  content = 'entries',
  content_rowid = 'id',
  tokenize = 'trigram'
);
```

- **只用 trigram**；不索引 `url`、`body_md` 全量、标签名（标签名走独立查询路径，见 §9）。
- 正文进 FTS 的长度由 `excerpt` 决定（见 §12 容量旋钮）。
- 首建后必须执行一次 `INSERT INTO fts_entries(fts_entries) VALUES('rebuild');`（触发器只对建立后的写入生效）。

### 3.10 FTS 同步触发器

```sql
CREATE TRIGGER entries_ai AFTER INSERT ON entries BEGIN
  INSERT INTO fts_entries (rowid, title, excerpt, notes, author)
  VALUES (new.id, new.title, new.excerpt, new.notes, new.author);
END;

CREATE TRIGGER entries_ad AFTER DELETE ON entries BEGIN
  INSERT INTO fts_entries (fts_entries, rowid, title, excerpt, notes, author)
  VALUES ('delete', old.id, old.title, old.excerpt, old.notes, old.author);
END;

CREATE TRIGGER entries_au AFTER UPDATE OF title, excerpt, notes, author ON entries BEGIN
  INSERT INTO fts_entries (fts_entries, rowid, title, excerpt, notes, author)
  VALUES ('delete', old.id, old.title, old.excerpt, old.notes, old.author);
  INSERT INTO fts_entries (rowid, title, excerpt, notes, author)
  VALUES (new.id, new.title, new.excerpt, new.notes, new.author);
END;
```

## 4. 索引清单

```sql
-- entries
CREATE UNIQUE INDEX ux_entries_url_hash ON entries (url_hash) WHERE url_hash IS NOT NULL;
CREATE INDEX ix_entries_l1_l2          ON entries (lifecycle, l1_id, l2_id, pinned DESC, created_at DESC);
CREATE INDEX ix_entries_pipeline       ON entries (pipeline_status, created_at DESC);
CREATE INDEX ix_entries_lifecycle_created ON entries (lifecycle, created_at DESC);
CREATE INDEX ix_entries_trashed        ON entries (lifecycle, trashed_at) WHERE lifecycle = 'trashed';
-- entry_tags / entry_sources / corrections / pipeline_jobs / tags / tag_aliases 见 §3 各表
```

对应查询模式：

| 查询 | 命中索引 |
|---|---|
| L1/L2 过滤 + 收件箱/全部（active）+ pinned 优先 | `ix_entries_l1_l2` |
| 收件箱视图（pipeline_status ∈ pending/processing/failed） | `ix_entries_pipeline` |
| 全部视图 / 封存箱（lifecycle=active/archived） | `ix_entries_lifecycle_created` |
| 回收站（lifecycle=trashed） | `ix_entries_trashed` |
| 标签 AND 过滤 | `ix_entry_tags_tag` + entries 主键回表 |
| 标签使用计数（#9 无状态） | `ix_entry_tags_tag` + `entries.lifecycle` 过滤 |
| 去重查询 | `ux_entries_url_hash`、`ux_entry_sources_ref` |
| 队列取任务 / 租约恢复 | `ix_pipeline_jobs_ready` / `ix_pipeline_jobs_lease` |

索引只增不减，直到出现慢查询证据；不做「以防万一」的索引。

## 5. 状态机落地映射（#8）

```text
pipeline_status: pending → processing → classified
                                     ↘ failed(failure_reason) ──重试──→ pending
lifecycle:       active → archived（封存）→ active
                 active|archived → trashed（回收站，trashed_at）→ 30 天硬删
正交标记:        pinned / done / corrected_by(ai|human)
```

- 封存/回收站条目**搜索仍可达**（FTS 不按 lifecycle 过滤，查询侧 join 后带徽标展示）。
- `trashed` 30 天硬删：启动扫描 + 每日定时任务，条件 `lifecycle='trashed' AND trashed_at < now - 30d`；硬删由外键级联清 `entry_tags`/`entry_sources`/`pipeline_jobs`，`corrections.entry_id` 置 NULL。
- 5 秒撤销只在前端暂存操作（软删后生命周期未变则立即恢复），不涉及 schema 变更。

## 6. 标签模型落地（#9）

- 实体：`tags`（display_name / normalized_key / aliases / kind / origin / 常驻 / 隐藏）。
- 可见性：**查询时计算**，无计数列、无升降级事件：`可见 = (active 使用数 ≥ 2) OR is_resident=1`，且 `is_hidden=0`；形态标签恒显（不受门槛、置灰逻辑在 UI）。
- 归一化：trim + 拉丁大小写折叠 + 全半角统一 → `normalized_key`；1~20 字符，非法输出丢弃。
- 合并（事务）：源标签全部 `entry_tags` 改写为目标 → `tag_aliases` 追加 `源名→目标` → 源标签软删（墓碑）。
- 删除：摘除 `entry_tags` 引用 + `deleted_at` 墓碑（预设标签 seed 不复活）；零使用标签可一键清理，但**不自动删**。
- 形态标签 sync：启动时按 taxonomy `format_tags` upsert（`kind='format'`、`origin='format'`），名称/颜色以 taxonomy 为准。

## 7. taxonomy.json 变更（本票落地）

1. **新增** fail-safe 节点（与 AGENTS.md 文案一致）：

```json
{
  "id": "l1_inbox",
  "name": "00 未分类",
  "icon": "inbox",
  "children": [
    { "id": "l2_manual_review", "name": "待人工归档", "l3_preset_tags": [] }
  ]
}
```

2. **移除** `global_l3_tags.media_types`（#9 决议：媒体类型改为条目固有 `media_type`，枚举 `link/video/article`）。
3. **bump `version`**（如 1.0.0 → 1.1.0），DB 侧迁移见 §10。
4. `l3_preset_tags` 保持为种子定义：标签实体的创建来源 `origin='preset'`，跨 L2 复用。

**收件箱 ≠ 未分类**：收件箱是管线视图（`pipeline_status ∈ pending/processing/failed`）；「00 未分类」是 `classified` 但分类引擎无法落位的条目所在节点。两者在 UI 上是不同入口。

## 8. 去重与来源（双轨）

1. 外部来源 `record_id` 优先：先查 `entry_sources(source_type, source_ref)`。
2. canonical URL 哈希兜底：再查 `entries.url_hash`。
3. 都未命中 → 新建条目；命中 → 更新 `last_seen_at`（条目 + 来源行），**不新建**。
4. 同一条目可挂多来源（bot + 云端表格同时投递 → 两条 `entry_sources`，一条 `entries`）。
5. URL 归一化规则（Rust 侧实现）：scheme/host 小写、去默认端口、去 fragment、去跟踪参数（`utm_*` 等）、去末尾斜杠、query 参数按 key 排序；纯文本条目 `url_hash = NULL`（不受唯一约束）。
6. 哈希：BLAKE3 前 128 bit，存 BLOB。

## 9. FTS 检索约定

**查询构造**（安全：用户输入是纯文本，不是 FTS5 语法）：

1. 按空白 + 中英文边界切段。
2. 每段用双引号包裹，段内双引号双写（`"` → `""`）。
3. 段间用 `AND` 连接（多词收敛）；需要前缀补全时，`*` 放在引号外、最后一个片段之后。
4. **< 3 字符**（按 Unicode 字符计）的查询走 `LIKE '%…%'` 兜底（转义 `%`、`_`，不加 `ESCAPE`），10k 级 3–6 ms。

**排序**：`ORDER BY rank`（= bm25，负值，越小越相关）；标题加权用 `bm25(fts_entries, 10.0, 1.0, 1.0, 1.0)`。

**标签名搜索**（独立路径）：`tags.normalized_key = ?` 或 `display_name LIKE ?` → `entry_tags` → `entries`；结果与 FTS 结果合并，标签命中优先。**标签改名不重建 FTS**。

**结果过滤**：搜索覆盖全部 lifecycle（封存/回收站条目带徽标返回）；pipeline_status 过滤由调用方按视图决定。

**维护命令**：`rebuild_fts()`（`INSERT INTO fts_entries(fts_entries) VALUES('rebuild')`），首建后必跑、tokenizer/索引长度变更后必跑。

**已知限制**：trigram 不含空格语义（带空格的查询是连续子串匹配，故必须切段）；无拼音；1–2 字靠 LIKE。

## 10. taxonomy 版本迁移

分类由**只读底表 + 用户覆盖层**组成（#18，见 [taxonomy-evolution.md](./taxonomy-evolution.md) §3）：底表是安装包内的 `taxonomy.json`，覆盖层是 `app_data_dir/taxonomy.overlay.json`（只存自建 L2），两者合并为**生效分类树**；生效版本形如 `1.1.0+u3`，`meta.taxonomy_version` 存生效版本。

1. 启动时读底表 + 覆盖层，校验（父 L1 存在、id 唯一、同父下名字唯一）并合并出生效分类树；覆盖层损坏则备份后以底表启动。再比对 `meta.taxonomy_version`。
2. 版本不一致时：
   a. `VACUUM INTO 'backups/aether-<ts>.db'` 备份，并把覆盖层快照复制到 `backups/`（各保留最近 5 份）。
   b. 校验 `entries` 中 distinct `(l1_id, l2_id)` 是否存在于生效分类树；失效引用重映射到 `l1_inbox/l2_manual_review`。
   c. L2 换父节点：按生效分类树重算受影响条目的 `l1_id`。
   d. 更新 `meta.taxonomy_version`。
3. 失效引用重映射**不写 corrections**（系统迁移不进纠偏样本；见 #14 的数据口径）。
4. 自建节点的新建 / 停用 / 删除 / 改名只改覆盖层（`revision + 1`）；官方节点不在应用内改动。

## 11. 迁移与备份

- 框架：`rusqlite_migration`（`PRAGMA user_version`，顺序执行 + 原子提交 + 降级检测）。
- v1 迁移内容：本文件 §3–§4 全部 DDL + 触发器 + `rebuild_fts`。
- 迁移前自动 `VACUUM INTO` 备份到 `backups/`，并把 `taxonomy.overlay.json` 一并复制为 `backups/taxonomy.overlay.<ts>.json`，保留最近 5 份。
- 大数据量 `rebuild`（FTS 重建）放在迁移之后、可中断、带进度；不放进事务迁移脚本。
- 迁移脚本改动（后续版本）：只追加，不改已发布迁移。

## 12. 容量旋钮

| 常量 | 值 | 含义 |
|---|---|---|
| `BODY_STORE_CHARS` | 1500 | `body_md` 存储上限（字符）；覆盖 PRD 分类上下文 |
| `FTS_EXCERPT_CHARS` | 500 | `excerpt` 长度 = 进 FTS 的正文；也是「搜索覆盖」旋钮 |

体积估算（1 万条目）：正文 ~45 MB + FTS 增量 ~70 MB + 元数据 ≈ **100–130 MB**。若条目数长期超过 2 万，或用户反馈「搜不到正文深处」，评估切换到 `jieba-rs` 字符级预分词（索引 ~1.9×，全文覆盖）——届时只需重建 FTS，不动业务表。

## 13. 验收清单

- [ ] 迁移 v1 后 `PRAGMA user_version = 1`，所有表/索引/触发器存在。
- [ ] WAL 生效（`PRAGMA journal_mode` 返回 `wal`）。
- [ ] 插入条目 → FTS 可搜；改 `title` → 搜索结果更新；删条目 → FTS 行消失。
- [ ] 硬删条目后 `corrections.entry_id IS NULL`，其余字段保留。
- [ ] 重复投递同一 URL / 同一 `(source_type, source_ref)` 不新建条目，`last_seen_at` 更新。
- [ ] 删除预设标签后重启，seed 不复活（墓碑生效）；再次输入同名标签复活且 id 不变。
- [ ] 标签合并后条目引用指向目标标签，别名表可反向解析。
- [ ] 迁移前自动备份文件生成且可打开。
- [ ] `< 3` 字符搜索走 LIKE、`≥ 3` 字符走 MATCH；FTS5 语法注入（`OR`/`NOT`/引号）不改语义不报错。
- [ ] taxonomy 版本 bump 后失效引用重映射到 00 未分类，L2 换父后 `l1_id` 同步。
- [ ] 覆盖层合并生效：自建 L2 出现在球列/L2 面板；停用节点不注入分类候选；覆盖层损坏时回退底表并提示。
- [ ] `entries.classify_suggestion` 仅弃权条目非空，改分类后清空。

## 14. 未纳入本票

- **云端同步的本地存储**：`sync_sources` 表与增量游标（#5 调研的落地形态）。本票明确「仅本地」，待云端同步功能立项时以 migration 追加。
- 跨机迁移 / 导入导出格式（地图 Not yet specified）。
- 分类运行日志表（#14 评测需要时再定，corrections 已提供纠偏样本）。
- 多语言与展示时区以外的时间处理。
- 列表虚拟滚动/分页策略（地图 Not yet specified）。
