# aether-core（应用核心库，#27）

> 规格：[AetherHUD v1 实现规格 #24](https://github.com/kostya-sec/AetherHUD/issues/24) §1 / §7；
> [taxonomy-evolution.md](../../docs/spec/taxonomy-evolution.md) §3 / §8.4；ADR-0003；
> [sqlite-schema.md](../../docs/spec/sqlite-schema.md) §3.8 / §10。

本 crate 是**领域行为的唯一边界**：存储与迁移、分类树合并与生效树导出、HUD 导航数据、
可注入适配器。它不依赖 Tauri、不直接摸系统 API；带 Tauri 的那一层在 `src-tauri/webview`。

## 模块

| 模块 | 职责 |
|---|---|
| `adapters` | `Clock` / `FileSystem` / `HttpTransport` / `Keychain` 四个 trait + 系统实现（`SystemClock`、`StdFileSystem`、`ReqwestTransport`、`OsKeychain`）；`adapters::testing` 提供 `FakeClock` / `MemoryFileSystem` / `FakeHttpTransport` / `MemoryKeychain` |
| `taxonomy` | 底表 ⊕ 覆盖层合并：读底表、读/建覆盖层（缺失 = revision 0）、校验（父 L1、id 唯一、同父规范化名唯一、名称 2~10 字）、合并（官方保序、自建按 created_at 追加、停用/merged 排除、官方撞名并入）、原子导出 `taxonomy.effective.json`、覆盖层损坏备份 + 回退底表；`load_exported_view` 供评测只读加载 |
| `storage` | SQLite 打开（WAL / synchronous=NORMAL / foreign_keys=ON / busy_timeout=5000）+ `rusqlite_migration`；T03 只落 `meta` 表（#28 在同一 v1 迁移里补齐 §3 全表）；`meta.taxonomy_version` 读写 |
| `navigation` | 从 `TaxonomyView` 派生 HUD 球列/L2 面板 DTO（5 个 L1 + 子类 + 形态标签，顺序即生效树顺序） |
| `notice` | 启动期 HUD 提示（覆盖层损坏等），前端显示 5 秒提示条、不抢焦点 |
| `paths` | app_data_dir 解析：显式参数 > `AETHER_DATA_DIR` > 平台数据目录 / identifier |

## 启动流程（`Core::bootstrap`）

1. 读只读底表 `taxonomy.json`；
2. 在 `app_data_dir` 下读 `taxonomy.overlay.json`：缺失则创建空覆盖层（`revision = 0`）；
   损坏（JSON 非法或校验失败）则备份为 `taxonomy.overlay.corrupt-<ts>.json`、以空覆盖层
   继续，并产出一条 HUD 提示；
3. 合并为生效分类树（版本 `<底表版本>+u<revision>`），原子写 `taxonomy.effective.json`；
4. 打开 `aether.db`、跑迁移，把生效版本写入 `meta.taxonomy_version`；
5. 生成 `HudNavigation` 供 `hud_navigation` IPC 使用，启动提示由 `hud_take_notices` 取走。

> 条目失效引用重映射（sqlite-schema §10 第 2b/2c 步）与迁移前 VACUUM INTO 备份
> （§11）随 #28/#43 的真实业务表一起接入；本票只保证 `meta.taxonomy_version` 与生效树一致。
>
> 官方节点与自建节点撞名的**持久化**对账（写覆盖层 `status=merged`、`merged_into`、
> `revision+1`）属于 #41；本票在合并时做只读投影（官方优先、以 created_at 作稳定 at），
> 不让生效树出现重复节点，但不会改用户覆盖层文件。

## 唯一实现约束

- 「底表 ⊕ 覆盖层 → 生效树」只有本 crate 一份；`aether-classifier` 只做「文档 → 视图」，
  不读文件；`aether-eval` 用 `taxonomy::load_exported_view` 读应用导出的生效树
  （缺失回退底表）。
- 覆盖层、生效树的文件名与版本语义集中在 `taxonomy` 常量里，壳/CLI 不各自拼路径。

## 测试

```bash
cargo test -p aether-core
```

覆盖：空覆盖层创建与 effective 导出、自建节点排序、停用/merged 排除、官方撞名并入、
损坏覆盖层备份与回退、base 非法 fatal、meta 版本写入、HUD 导航顺序、评测只读加载回退、
原子写与适配器夹具。all tests use `MemoryFileSystem` + `FakeClock`（数据库用临时目录/内存库）。
