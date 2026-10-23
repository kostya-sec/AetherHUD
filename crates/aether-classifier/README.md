# aether-classifier（共享分类路径）

> 施工依据：[classification-prompt.md](../../docs/spec/classification-prompt.md)（V-C4）、
> [taxonomy-evolution.md](../../docs/spec/taxonomy-evolution.md)（生效分类树 / 弃权建议）、
> [pipeline-worker.md](../../docs/spec/pipeline-worker.md)（#11 重试语义）。

本 crate 是生产分类器与评测工具的唯一实现，避免“评测专用分类器”：

| 模块 | 职责 |
|---|---|
| `taxonomy` | 分类树**文档 → 视图**：解析、校验、派生索引、`merged_into` 重映射、树渲染（inbox 置末）；文件读取与覆盖层合并在 `aether-core::taxonomy`（#27），本 crate 不碰文件系统 |
| `prompt` | V-C4 system / few-shot / 条目块组装；`build_classification_messages`；模板 / few-shot / 候选池哈希；修复重试消息 |
| `schema` | `validate_classification`：JSON → 结构 → 枚举层级 → 形态标签 → 特征标签 → 建议逐字段降级 |
| `provider` | OpenAI 兼容 Chat Completions（DashScope 默认端点、json_object、代理/直连、usage） |
| `retry` | #11 重试策略（30s×2^n±20%、封顶 30min、最多 8 次、错误二分） |
| `config` | 本地 few-shot 池与特征标签候选池快照加载 |
| `engine` | 端到端：组装 → 调用（重试）→ 校验 → 1 次修复重试；输出状态 / usage / 调用明细 |

## 与生产 worker 的衔接

- worker 取 `entries` 行 → 组 `EntryForClassify` → `ClassificationEngine::classify` →
  写回 `l1_id/l2_id`、标签、`classify_suggestion`；失败语义用 `OutcomeStatus` +
  `failure_reason` 映射到 #11 的 `classify_failed`。
- prompt / 校验 / 重试 / 失败判定只有这一份；评测通过 `aether-eval` 复用。
- 生效分类树由 `aether-core::taxonomy` 合并/导出后，以 `TaxonomyView` 注入引擎；本 crate
  不再自己读 `taxonomy.json` 或 `taxonomy.effective.json`（#27 单实现约束）。

## 测试

```bash
cargo test -p aether-classifier
```
