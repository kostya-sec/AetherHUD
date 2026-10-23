# aether-eval（本地分类评测 CLI，#21）

> 规格：[docs/spec/classification-quality.md](../../docs/spec/classification-quality.md)（#14）、
> [classification-prompt.md](../../docs/spec/classification-prompt.md)（#19）、
> [taxonomy-evolution.md](../../docs/spec/taxonomy-evolution.md)（#18）。
> 本 crate 是独立实现，后续按票面迁入 `src-tauri/src/bin/aether-eval.rs`。

## 0. 目录与职责

```text
crates/
├── aether-classifier/   # 生产分类路径（评测与分类 worker 共用）：
│                        #   分类树视图 / V-C4 prompt / 输出校验 / provider / #11 重试 / 端到端引擎
├── aether-core/         # 应用核心库：分类树合并与导出、存储、路径、适配器
└── aether-eval/         # 本 crate：评测、指标、报告、基线、纠偏候选、few-shot 查重
```

“复用生产分类路径”的落点：`aether_classifier::prompt::build_classification_messages`、
`aether_classifier::schema::validate_classification` 与 `ClassificationEngine::classify`；
评测不另写 prompt 或校验。生效分类树的读取走 `aether_core::taxonomy::load_exported_view`
（缺失回退底表），与生产启动共用同一份加载实现。

## 1. 本地数据目录

默认 `app_data_dir` 与 Tauri 壳共用 `aether_core::paths`：平台数据目录 /
`io.github.kostya-sec.aetherhud`；开发期用 `--data-dir` 或 `AETHER_DATA_DIR` 指向任何符合
下列结构的目录（仓库内本地数据在 `.local/`，已 gitignore）：

```text
<data_dir>/
├── taxonomy.effective.json          # 生效分类树（缺失回退底表；#18 §8.4）
├── classifier/
│   ├── fewshot.json                 # 8 条本地 few-shot 实例（不入 git）
│   └── tag_pool.json                # 特征标签候选池快照（可缺省）
├── eval/
│   ├── sets/2026-10-v1.jsonl        # 主评测集，visible + blind 同文件
│   ├── sets/2026-10-v1.boundary.jsonl
│   ├── reports/<timestamp>.{json,md,items.jsonl,feature-review.jsonl}
│   └── candidates/candidates-<timestamp>.{jsonl,summary.json}
└── aether.db                        # 生产库；仅 candidates 读
```

底表 `taxonomy.json` 默认从 `<data_dir>/taxonomy.json` 读取，缺失时回退当前目录；也可用
`--base-taxonomy` 显式指定。

## 2. 评测（默认子命令）

```bash
target/release/aether-eval.exe \
  --data-dir .local \
  --set visible --provider dashscope --model qwen-flash --prompt-version 1 \
  --env-file .local/keys.env
```

- `--set visible|blind|boundary`：主集按 `split` 过滤；boundary 读 `<version>.boundary.jsonl`，
  不参与门线（#14 §3.5）。
- 固定条件：`temperature=0`、`max_tokens=400`、`response_format=json_object`、串行调用；
  provider 为 dashscope 时附加 `enable_thinking=false`（#19 §10 实测条件）。
- 重试：#11 语义（30s×2^n ±20% jitter、封顶 30min、最多 8 次；4xx/401 终态）；
  每次请求 30s 超时；校验失败时 1 次修复重试（#19 §8.3）。
- `--limit N`：只跑前 N 条，用于调试；**不登记基线、不与基线比较**。
- 凭证：`--api-key-env DASHSCOPE_API_KEY`（默认）读取环境变量；`--env-file` 可从本地
  `KEY=VALUE` 文件补充缺失变量。凭证永不写入代码、报告或仓库。
- 代理：默认读 `AETHER_HTTPS_PROXY` / `HTTPS_PROXY` / `HTTP_PROXY` / `ALL_PROXY`；
  `--no-proxy` 强制直连。DashScope 在本机可直连。

退出码：0 = 评测完成（门线不过不影响退出码，见报告判定）；2 = 参数 / 运行错误。
`dedupe` 主集存在重叠时退出码 1。

### 2.1 报告

每次运行写四个文件（只创建不覆盖，时间戳命名）：

| 文件 | 内容 |
|---|---|
| `<ts>.json` | 完整机器可读报告：参数、双版本、指标、分组、混淆矩阵、全部错例 / 失败、基线差值、警告 |
| `<ts>.md` | 人读报告：参数与版本、总体指标、门线判定、分组、混淆矩阵、错例 Top20、失败、组成、基线对比 |
| `<ts>.items.jsonl` | 逐条结果（金标 / 输出 / 原始输出 / 调用与 usage / 耗时），供复核与二次分析 |
| `<ts>.feature-review.jsonl` | 特征标签抽样 20 条，人工回填 `verdict` / `note` 后统计离谱率 |

指标口径与门线完全按 #14 §2.3 / §2.4：弃权计入 L1/L2 错误并单列；分类失败不计入准确率分母、
计入可用性；形态标签 micro-F1 按 TP/FP/FN 全局汇总；另给分 L1 / 分 L2 / 分难度与混淆矩阵。

版本校验：底表版本不一致 → 报告标注“不可比”且不做基线差值；仅生效版本不同 → 标注
“生效分类树已变更，本结果为新基线”（#18 §8.4）；评测集引用被合并的 `l2u_` id 时按
`merged_into` 重映射并提示。

### 2.2 基线

基线按组合 `provider | model | prompt_version | eval_set_version | taxonomy 底表版本 | 生效版本 | split`
索引于 `eval/reports/baseline-index.json`：

- 组合第一次成功运行 = 基线（报告时间戳命名，不可覆盖）；
- 后续同组合（且评测集文件哈希一致、版本校验通过）运行给出六项指标差值与 >2pt 回归告警（#14 §2.5）；
- 换版本 / 换模型 / 评测集内容变化 = 新基线；
- 盲测集重复运行次数会写入报告警告（#14 §4.3）。

## 3. 纠偏候选（`candidates`）

```bash
target/release/aether-eval.exe --data-dir .local candidates \
  --out .local/eval/candidates [--db .local/aether.db]
```

- `corrections LEFT JOIN entries`：只取 `dimension IN ('l1','l2')` 的分类错例；标签增删 /
  媒体类型变更按非错例排除并计数（#14 §6.1）。
- 按 `entry_id` 去重：一次“采纳建议”写 L1、L2 两行 → 一个候选（#18 §8.4）。
- `entry_id IS NULL`（条目硬删，无文本）跳过并计数。
- 输出字段：title / body_md 前 1500 字 / media_type / `before`（模型当时输出）/
  `after`（用户金标）/ taxonomy_version / corrections 明细 / `review.status=pending`。
  人工审核后回填 `review`，通过者才进入评测集或 few-shot 池（#14 §6.2）。

## 4. few-shot 查重（`dedupe`）

```bash
target/release/aether-eval.exe dedupe \
  --fewshot <data_dir>/classifier/fewshot.json \
  --set <data_dir>/eval/sets/2026-10-v1.jsonl \
  [--boundary <data_dir>/eval/sets/2026-10-v1.boundary.jsonl] [--json]
```

按 `entry_id`、内容哈希（标题 + 规范化 URL）、标题三级指纹查重：

- 与**主评测集**重叠 → 退出码 1（对应条目成绩被污染，必须替换示例，#14 §6.2）；
- 与**边界集**重叠 → 仅告警（边界集不参与门线；#19 的 2 条弃权示例本身取自边界集）。

## 5. 实现边界与已知简化

- `app_data_dir` 已与 Tauri 壳共用 `aether_core::paths`（identifier
  `io.github.kostya-sec.aetherhud`）；`AETHER_DATA_DIR` 可覆盖壳与 CLI 两侧。
- 网络错误按可重试处理并消耗 attempts；#11 的“断网挂起、不消耗 attempts”用于生产 worker，
  评测是单次本地运行，未实现离线挂起。
- 特征标签离谱率依赖人工回填 `feature-review.jsonl`，本工具不自动判定。
- 未实现非加法 taxonomy 变更的独立评测集重映射脚本（#14 §7.2）；报告侧 `merged_into`
  重映射已实现。
- 盲测集重复运行次数通过扫描 `reports/*.json` 统计；报告目录被清理后计数丢失。

## 6. 测试

```bash
cargo test          # 60 个单测：指标口径、校验、prompt 结构、重试、候选去重/跳过、查重、基线、报告
cargo fmt --all
```
