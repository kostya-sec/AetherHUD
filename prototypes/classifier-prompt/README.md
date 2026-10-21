# 原型：分类 prompt 设计（#19）

一次性原型，回答「分类 prompt 怎么组、few-shot 怎么排、弃权怎么下指令」。**不并入 main**，只作为 [docs/spec/classification-prompt.md](https://github.com/kostya-sec/AetherHUD/blob/main/docs/spec/classification-prompt.md) 的原始证据。

## 怎么跑

```bash
# 需要本地数据（不入库，放在 .local/proto/cp/data/）：
#   samples.json        真实样本 {key,title,url,media_type,body_excerpt,gold}
#   fewshot.json        few-shot 实例（含用户收藏原文，本地保存）
#   boundary-smoke.json 边界集样本
# 以及 .local/keys.env 里的 DASHSCOPE_API_KEY（百炼 OpenAI 兼容端点）
node prototypes/classifier-prompt/run.mjs        # 环境变量 SET=samples|boundary-smoke  VARIANTS=A,B,C,D,C2,C3,C4
node prototypes/classifier-prompt/report3.mjs    # 汇总指标表
```

## 版本

| 版本 | 结构 |
|---|---|
| A | 全量分类树 + JSON 契约（朴素基线） |
| B | A + 判定方法 + 特征标签候选池 + 6 条 few-shot |
| C | B + 弃权/建议指令 + 2 条弃权示例 + 校验重试 |
| D | C 去掉 suggestion 字段（对照） |
| C2 | C + 收紧标签条数措辞 |
| C3 | C2 + 00 未分类挪到树末尾 + 弃权措辞 v2 |
| C4 | C3 + 点名「不覆盖主题」 |

## 结果（qwen-flash，10 条真实收藏，2026-10-21）

| 版本 | 首轮合规 | 有效输出 | L1/L2 | 形态 F1 | 重试 | 平均输入 tok | 平均耗时 |
|---|---|---|---|---|---|---|---|
| A | 1/10 | 1/10 | 1/1 | 1.00 | 0 | 1,175 | 660ms |
| B | 6/10 | 6/10 | 6/6 | 1.00 | 0 | 2,774 | 619ms |
| C | 7/10 | 10/10 | 9/10 | 0.87 | 3 | 4,622 | 863ms |
| D | 9/10 | 10/10 | 9/10 | 0.87 | 1 | 3,657 | 718ms |
| C2 | 9/10 | 10/10 | 9/10 | 0.95 | 1 | 3,866 | 695ms |
| C3 | 10/10 | 10/10 | 9/10 | 0.91 | 0 | 3,531 | 639ms |
| C4 | 10/10 | 10/10 | 9/10 | 0.95 | 0 | 3,577 | 642ms |

边界集（人工标为无法唯一归类，5 条）：C2 弃权 0/5、C3 1/5、C4 2/5；可分类样本 0/10 弃权。

主要结论：模型不会自觉遵守「≤3 个特征标签」约束（A 有 9/10 超限），校验 + 1 次修复重试是硬要求；收紧措辞后重试降到 0；`response_format: json_schema` 在 qwen-flash 上输出退化，不可用。

## 文件

- `prompt.mjs`：7 个版本的 system prompt / few-shot 组装
- `taxonomy.mjs`：读 `taxonomy.json` 渲染分类树
- `validate.mjs`：输出 schema 校验（#18 §2.2 的 suggestion 规则）
- `run.mjs`：串行调用、校验、1 次修复重试、逐条落 JSONL
- `report3.mjs`：指标汇总
- `fewshot.example.json`：few-shot 结构示例（内容已脱敏；真实示例不入库）
