# 离线队列与重试策略规格（pipeline worker，v1）

> 状态：已确认（2026-10-20）
> 来源票：[决策：离线队列的持久化与重试策略](https://github.com/kostya-sec/AetherHUD/issues/11)
> 依赖决议：#3（抓取降级链与分级超时）、#8（先落库 pending、failed 可手动重试）、#10（pipeline_jobs 表形状）
> 本文件是 pipeline worker（抓取/分类执行循环）的施工依据；表结构以 [sqlite-schema.md](./sqlite-schema.md) §3.7 为准。

## 1. 范围与非目标

**范围**：断网检测与挂起恢复、错误分类、重试退避参数、租约与重启恢复语义、job 终态与 entries 状态派生的事务边界、worker 并发模型、手动重试语义。

**非目标**：队列表 DDL（#10 已定，见 sqlite-schema.md §3.7）；摄取入口与去重顺序（#8 已定「先落库 pending」）；云端轮询器（v1 仅本地，随云端同步功能立项再议）。

## 2. 队列与持久化

- 队列 = `pipeline_jobs` 表（SQLite WAL），不引入外部队列组件。
- 同一 `entry_id` 的 fetch 与 classify 是两行（`UNIQUE(entry_id, stage)`），fetch 成功后才插入 classify 行（链式触发，见 §7）。
- `job.state` 只有 `queued / running` 两态：**job 终结即删行**，终态结果显式迁移到 `entries.pipeline_status`（§7）。

## 3. 错误分类

| 类别 | 错误 | 处理 |
|---|---|---|
| 网络错误 | DNS / connect / TLS / 请求超时（对端不可达） | 触发离线挂起（§4），**不消耗 attempts** |
| 可重试错误 | HTTP 5xx、429、Jina/分类器响应超时（对端可达但失败） | 退避重试（§5），消耗 attempts |
| 终态错误 | HTTP 4xx（404/410/403）、URL 非法、分类器 401（Key 无效）、分类器返回格式非法 | 直接终态失败（§7） |

**fetch 的终态失败只在降级链穷尽后出现**：先走完 #3 降级链（L0 视频元数据 → L1 直连 → L2 Jina → L4 保底存标题）；L4 成功 → `metadata_status='partial'` 继续 classify（条目 classified+partial）。`failure_reason='fetch_failed'` 仅在连标题都拿不到（URL 完全无响应且无任何元数据）时出现。

## 4. 断网检测与离线挂起

- **纯被动检测**：不做常驻网络监控（无定时 ping、无 OS 网络事件订阅）。worker 执行任务时捕获到网络层错误即判定疑似离线。
- **离线挂起（Offline Suspension）**：进入挂起后 worker 停止领取新任务，当前 job 重新入队（`state='queued'`，`attempts` **不增**）。挂起期间新入库条目正常 `pending` 入队，等待恢复后统一处理。
- **恢复探针**：挂起期间每 **60s** 发一次轻量探针（对分类器 endpoint 或 `1.1.1.1` 的 HEAD，超时 3s）。探针成功 → 退出挂起，将所有 job 的 `next_attempt_at` 重置为 now，立即全量恢复。
- 设计理由：单用户桌面场景断网是常态（合盖、切网络），常驻监控是浪费；探针把恢复延迟压到 ≤60s；网络错误不计数避免「一觉睡醒全部 failed」。

## 5. 重试退避参数

- 可重试错误：指数退避 **30s × 2^attempts，±20% jitter，封顶 30min**。
- 序列：30s → 1m → 2m → 4m → 8m → 16m → 30m → 30m；`attempts` 上限 **8 次**，超限转终态失败（`failure_reason` 按 stage 取 `fetch_failed / classify_failed`）。
- 总重试窗口约 1.5 小时：足以覆盖 Jina/分类器临时故障，又不让死任务无限占队。
- jitter 防雪崩：多任务同时失败时退避时间不撞车。

## 6. 租约与重启恢复

- 领取任务：同一事务内 `SET state='running', lease_until = now + lease`。lease 时长：**fetch = 45s**（覆盖直连 8s + Jina 20s + 余量）、**classify = 30s**（P95 < 2s，余量充足）。
- 启动扫描：`state='running' AND lease_until < now` → 重置 `state='queued'`、`next_attempt_at = now`，**`attempts` 不增**（崩溃不是失败）。
- `queued` 且到期的 job 无需特殊恢复：进程重启后被 `ix_pipeline_jobs_ready` 索引自然捞到。

## 7. job 终态与 entries 状态派生（全部同事务）

| 事件 | 事务内容 |
|---|---|
| 入队（新条目 / 重试） | INSERT job + `entries.pipeline_status='pending'` |
| 领取执行 | `job.state='running'` + lease + `entries.pipeline_status='processing'` |
| fetch 成功 | DELETE fetch job + INSERT classify job + `entries` 写 `body_md/excerpt/fetch_source/fetched_at/metadata_status` |
| classify 成功 | DELETE classify job + `entries.pipeline_status='classified'` + 写 `l1_id/l2_id` + `entry_tags` |
| 终态失败 | DELETE job + `entries.pipeline_status='failed'` + 写 `failure_reason` |

- entries 侧状态全部**显式迁移**，不从 jobs 表派生；满足 #10「实现须在同一事务里更新两处」。

## 8. 并发模型

- **单 worker 串行循环**：一次一个 job（取出到期 job → 执行 → 同事务落定 → 取下一条），不开并发 worker。
- 理由：单用户桌面，串行已足够（分类 P95 < 2s 异步不阻塞 UI）；天然规避 Jina 限流、SQLite 写争用与任务乱序。
- **stage 路径**：纯文本条目入库时直接 INSERT classify job（跳过 fetch stage）；视频元数据抓取属于 fetch stage 内部路径（`fetch_source='video_api'`）。

## 9. 手动重试

- failed 条目点「重试」= 同事务：INSERT 新 job（`attempts=0`、`next_attempt_at=now`）+ `entries.pipeline_status='pending'` + 清空 `failure_reason`。
- 重试起点按失败时的 `failure_reason` 决定：`fetch_failed` 从 fetch stage 重来；`classify_failed` 直接进 classify stage（沿用已抓取的 `body_md`，不重复抓取）。

## 10. 验收清单

- [ ] 断网时：抓取/分类挂起，attempts 不增，本地搜索与 HUD 浏览不受影响；恢复网络后 ≤60s 队列自动续跑。
- [ ] 可重试错误按 30s×2ⁿ±20% 退避、封顶 30min，第 8 次失败后转 failed 且 failure_reason 正确。
- [ ] 终态错误（4xx / 401 / 格式非法）不重试，直接 failed。
- [ ] fetch 走完整降级链：L4 保底成功时条目 classified + metadata_status='partial'；只有连标题都拿不到才 fetch_failed。
- [ ] 进程杀掉重启后：过期租约 job 重置为 queued 且 attempts 不增，到期 queued job 被自然捞起。
- [ ] 手动重试：attempts 清零；classify_failed 重试不重复抓取。
- [ ] job 终态删行与 entries 状态迁移在同一事务（模拟崩溃不产生半截状态）。
- [ ] 全程单 worker 串行，无并发领取同一 job（lease 生效）。
