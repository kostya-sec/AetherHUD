# 分类金标评测集（本地）

> 规格真源：[docs/spec/classification-quality.md](../spec/classification-quality.md)（#14 决议）
> 任务票：[#20 任务：标注 200 条分类金标评测集](https://github.com/kostya-sec/AetherHUD/issues/20)
> 校验器：[`scripts/eval/validate-eval-set.mjs`](../../scripts/eval/validate-eval-set.mjs)

真实评测集**只存本地、不入 git**（含真实收藏内容）。仓库只保留本说明、空模板与校验脚本。

## 1. 存放位置

Tauri `app_data_dir`（实现后以 `tauri.conf.json` 的 identifier 为准）：

| 平台 | 路径 |
|---|---|
| Windows | `%APPDATA%\<identifier>\eval\sets\` |
| macOS | `~/Library/Application Support/<identifier>/eval/sets/` |

- 主评测集：`eval/sets/2026-10-v1.jsonl`（200 条；`split` 字段区分 visible / blind）
- 边界集：`eval/sets/2026-10-v1.boundary.jsonl`（不参与门线、不计入 200 条）
- 报告：`eval/reports/`；纠偏候选：`eval/candidates/`（均由 #21 的工具生成）

## 2. 字段（#14 §3.7）

| 字段 | 类型 | 规则 |
|---|---|---|
| `entry_id` | string | 条目 ULID（与库内 `entries.uid` 对齐；未入库的占位 id 必须唯一并在 `notes` 注明） |
| `title` | string | 非空 |
| `url` | string | 网页/视频原文链接；**纯文本条目留空字符串** |
| `body_excerpt` | string | 正文前 ≤1500 字快照；「仅标题」难例可留空并在 `notes` 说明 |
| `media_type` | enum | `link` / `video` / `article`（条目固有属性，不参与准确率） |
| `gold.l1_id` | string | 必须是 taxonomy 的**正常 L1**（不允许 `l1_inbox`） |
| `gold.l2_id` | string | 必须是该 L1 的**正常 L2**（不允许 `l2_manual_review`） |
| `gold.format_tags` | string[] | 形态标签 **1~2 个**，取值 `global_l3_tags.format_tags` |
| `gold.feature_tags` | string[] | 核心特征标签 **0~3 个**（1~20 字，宁少不多） |
| `difficulty` | enum | `easy` / `hard` |
| `split` | enum | `visible` / `blind`（边界集为 `boundary`） |
| `eval_set_version` | string | 形如 `2026-10-v1`，绑定 taxonomy 版本 |
| `taxonomy_version` | string | 必须等于 `taxonomy.json` 的 `version`（当前 1.1.0） |
| `notes` | string | 可空；难例/占位 id/存疑点写这里 |

## 3. 分层与覆盖（门线验收前必须齐）

- 总量 **200 条**；`split` = **visible 150 + blind 50**（按 L1 分层，每类约 75% / 25%）。
- **12 个正常 L2 每个 ≥10 条**；**4 个正常 L1 每个 ≥40 条**。
- **难例（hard）≥20%**（≥40 条）：仅标题、<100 字短内容、跨类模糊、多语言/噪声。
- 媒体形态：**网页 ≥60%**、**视频 ≥20%**、**纯文本 ≥10%**（校验口径见下）。
- 边界集（模糊、无法唯一归类、金标有争议）**单独维护**，不参与门线，金标明确前不得移入主集。

### 媒体形态口径

`media_type` 只有 `link/video/article` 三值，而 §3.1 的「网页/视频/纯文本」是统计口径，校验器按以下方式计算：

- **视频** = `media_type: video`
- **纯文本** = `url` 为空字符串（手输文本条目；`media_type` 建议填 `article`）
- **网页** = 其余（`link`，或带 URL 的 `article`）

## 4. 标注流程

1. **候选**：从真实收藏 / 待归档条目里挑「你真的会用 HUD 找它」的条目；不要为凑数编造。
2. **拆层**：先按 L2 配额补齐 120 条，再按真实收藏分布补到 200，同时满足 L1 ≥40；按 L1 分层随机拆 150/50。
3. **标注**：L1/L2 唯一答案；形态标签 1~2 个必标；特征标签只标「搜索时会用、必须有」的核心标签；拿不准 → 边界集。
4. **校验**：`node scripts/eval/validate-eval-set.mjs --file <主集> [--boundary <边界集>]`，全绿后再冻结。
5. **冻结**：版本发布后不再修改；taxonomy 变更按 #14 §7 重映射并 bump 版本，跨版本指标不可比。

## 5. 校验命令

```bash
# 主集（visible + blind 同一文件）
node scripts/eval/validate-eval-set.mjs --file "<app_data_dir>/eval/sets/2026-10-v1.jsonl"

# 连同边界集一起校验
node scripts/eval/validate-eval-set.mjs \
  --file "<...>/2026-10-v1.jsonl" \
  --boundary "<...>/2026-10-v1.boundary.jsonl"

# 机器可读输出（供 #21 报告/CI 使用）
node scripts/eval/validate-eval-set.mjs --file "<...>" --json
```

校验器要求 Node ≥18（零依赖）。退出码：0 通过、1 有错误、2 参数或读取失败。


## 6. 与 #21 的衔接

- 校验器的字段规则与层次配额是 #21 `aether-eval` 的输入契约；两边口径不一致时以本文件与 #14 规格为准。
- 评测集版本、taxonomy 版本写进每条记录，报告必须回显两个版本并做可比性校验。

## 7. 从收藏导出导入候选

真实收藏导出（浏览器书签 HTML/JSON、飞书/表格 CSV、URL 清单）先转成候选 JSONL，再进入标注：

```bash
node scripts/eval/import-collection.mjs \
  --in "<收藏导出文件>" [--in <另一个文件> ...] \
  --out "<app_data_dir>/eval/candidates/collection-2026-10.jsonl" [--limit N]
```

- 支持格式：Chrome/Edge `Bookmarks` JSON、通用 JSON 数组、Netscape 书签 HTML、CSV（`url/title` 或 `链接/标题` 表头）、TXT/MD（一行一 URL 或 Markdown 链接）。
- 自动去重（canonical URL、去 `utm_*` 等跟踪参数）、按域名推断 `media_type`（视频站 → `video`，其余 → `link`）。
- 候选字段：`entry_id / title / url / media_type / source / notes`；`gold`、`difficulty`、`split` 在标注阶段补齐，**候选文件不得直接当评测集**。
- 收藏导出与候选文件都含真实内容，只放本地（`.local/` 或 `app_data_dir`），不入 git。
