# AetherHUD

SAO 风格资源中枢的桌面 HUD。当前进度：**常驻 HUD 外壳 + 生效分类树/HUD 导航骨架**（#25 脚手架、#26 唤出/收起与系统集成、#27 分类树与导航）——Tauri v2 真透明置顶窗口、React 19 前端、框架无关的 HUD 交互模型、全局热键、托盘与单实例、`aether-core` 核心库（底表 ⊕ 覆盖层合并、生效树导出、HUD 球列/L2 面板数据）。业务数据、摄取管线、卡片流、音效尚未接入（由后续票负责）。

## 环境

- Node.js ≥ 22.12（Vite 8）
- Rust ≥ 1.90 与 Tauri v2 的 Windows / macOS 系统依赖
- Windows 需 WebView2 Runtime

## 前端

```powershell
npm install
npm run dev        # 仅前端，http://127.0.0.1:5789
npm run typecheck
npm test           # vitest（hud-model、HUD 桥接）
npm run build      # tsc + vite build
```

## 桌面 HUD

```powershell
npm run tauri:dev    # 开发版：标识符 io.github.kostya-sec.aetherhud.dev，与安装版不抢单实例锁
npm run tauri build  # 发布构建：嵌入前端资源（custom-protocol）
cargo test --workspace
```

> 直接 `cargo build` 的 debug 载体仍按 Tauri 约定加载 `devUrl`，需先跑 `npm run dev`。
> 想要独立验证发布行为（嵌入资源、无 dev server）：`cargo build --release -p aetherhud --features tauri/custom-protocol`。

## 常驻行为（#26）

- **唤出**：手动启动时 HUD 从屏外滑入，默认显示球列、搜索框收起（不自动聚焦）；点击搜索球才展开搜索框并聚焦。Windows 默认 ``Alt+` ``（反引号）、macOS 默认 `Option+Space`，可后续在设置中重绑。
- **收起**：Esc、失焦（250ms 宽限）、再次热键、托盘左键都只收起；Alt+F4 / Cmd+W / 关闭窗口经 `prevent_close` 也只收起；收起开始即把焦点归还原应用，无需点击即可继续输入。
- **退出**只发生在托盘菜单（设置底部入口随 T22 接入），统一 `app.exit()`。
- **托盘**：左键切换 HUD；右键菜单含 显示/收起、设置…、关于/检查更新、退出；状态只放 tooltip；macOS 使用 template 图标并隐藏 Dock 图标。
- **单实例**：第二个进程不会起窗口，只会唤起既有实例；`--hidden`（登录项/重复触发）只驻留不唤出。
- **点击穿透**：整窗 `set_ignore_cursor_events` + Rust 侧 50ms 光标轮询 hitbox（物理坐标、负坐标），只有搜索框/球/提示条接收点击，空白区域的点击落到桌面。
- **显隐不用 hide()/show()**：窗口只 show 一次并屏外停靠，之后靠 `set_position` 滑入滑出；多显示器按每次唤出重新查询的 `currentMonitor + scaleFactor` 换算并钳制在单屏内。
- 热键注册失败不会中断启动：记录 `HotkeyStatus`（`hud_status` IPC）+ 托盘兜底。

详见 [docs/spec/hud-window-and-shell.md](docs/spec/hud-window-and-shell.md)。

## 生效分类树与 HUD 导航（#27）

- **核心库**：新增 `crates/aether-core`（领域唯一入口）。启动时读只读底表 `taxonomy.json` +
  `app_data_dir/taxonomy.overlay.json`，校验后合并为**生效分类树**（版本 `1.1.0+uN`），
  原子导出 `taxonomy.effective.json`，并把生效版本写入数据库 `meta.taxonomy_version`。
- **覆盖层**：不存在时创建空覆盖层（`revision = 0`）；损坏时备份为
  `taxonomy.overlay.corrupt-<ts>.json`、按底表启动，并在 HUD 内显示 5 秒提示条（不抢焦点）。
- **HUD 导航**：球列 = 搜索球（仅搜索框收起时）→ L3 筛选球 → 5 个 L1 球（含 00 未分类）→
  快速录入球 → 设置球；点 L1 球原地展开 L2 面板（顶部与被点球平行对齐，首项「全部子类」，
  自建节点在父 L1 末尾）；L3 球展开标签条（当前只渲染结构与顺序：形态标签 → 特征标签占位）。
- **唯一实现**：分类器不读分类树文件，只消费生效树视图；评测 CLI 用同一份
  `load_exported_view` 读取 `app_data_dir/taxonomy.effective.json`（缺失回退底表）。
- **数据目录覆盖**：开发/冒烟测试可设 `AETHER_DATA_DIR`，壳与 `aether-eval` 共用同一目录
  （发布版默认 `%APPDATA%\io.github.kostya-sec.aetherhud` / `~/Library/Application Support/io.github.kostya-sec.aetherhud`）。
- 施工依据：[docs/spec/taxonomy-evolution.md](docs/spec/taxonomy-evolution.md) §3 / §8.4、ADR-0003、issue #27。

## 前后端契约

- Rust → 前端事件：`hud://summon`（重置 S0：球列 + 搜索框收起，不请求聚焦）、`hud://dismiss`（进入 `dismissing`，吞键到下次唤出）、`hud://settings/open` / `hud://about/open`（T22 / T23 消费）。
- 前端 → Rust 命令：`frontend_ready`（首帧握手，须在事件监听注册后调用）、`request_dismiss`（Esc）、`set_hit_boxes`（交互区矩形）、`hud_status`（阶段/启动形态/热键状态）、`hud_navigation`（生效分类树投影：5 个 L1 + L2 + 形态标签）、`hud_take_notices`（启动期提示，逐次取走）。

## 结构

- `crates/aether-classifier/`：分类路径唯一实现（分类树视图、V-C4 prompt、校验、provider、重试、引擎）；不读分类树文件。
- `crates/aether-core/`：应用核心库（#27）——分类树合并/生效树导出、最小 SQLite 存储、HUD 导航 DTO、可注入时钟/HTTP/Keychain/文件系统适配器；见 [crates/aether-core/README.md](crates/aether-core/README.md)。
- `crates/aether-eval/`：离线评测 CLI（指标、报告、基线、纠偏候选、few-shot 查重）；读应用导出的生效树。
- `src/hud-model/`：框架无关的 HUD 状态、命令、选择器与全局键位守卫（vitest 测试缝）；球列/L2/L3 状态都在这里。
- `src/components/`：纯渲染层——`HudBallColumn`（L1/L3/搜索/入口球）、`HudL2Panel`（原地展开、与球顶对齐）、`HudL3Bar`（结构/顺序）、`HudNoticeBar`（5 秒提示）、`RainBackground`（取自 #7 原型）。
- `src/platform/`：Tauri 适配——`hudBridge`（外壳事件 + Esc/吞键守卫）、`navigation`（生效树/提示 IPC）、`hitboxes`（点击穿透上报）、`window`（首帧握手）、`windowDrag`（球列拖窗）。
- `src-tauri/src/`：薄 OS 壳——`window/`（几何钳制、显隐状态机、点击穿透、macOS NSPanel）、`tray/`、`hotkey/`、`focus.rs`（焦点归还）、`platform.rs`（光标位置）、`runtime.rs`（核心库装配/底表路径解析）、`commands.rs`（IPC）。
- `docs/spec/`：施工详规（`hud-window-and-shell.md`、`taxonomy-evolution.md`、`sqlite-schema.md` 等）。

## 已知限制

- macOS 的 NSPanel / `window_did_resign_key` 路径尚未在真机验证（本机为 Windows），需真机编译与回归；混合 DPI 下的光标换算为近似实现。
- 中文输入法组合态下普通按键表现为输入法 composition 文本（符合 #17 §7），输入法相关验收仍需真机按键回归。
- L3 标签筛选行为、自建节点治理（升格/停用/升级对账）、卡片流与业务数据在后续票；本票的 L3 条只渲染结构与顺序。
- 分类树版本不一致时的条目失效引用重映射（#10 §10）随 #28 的 `entries` 表接入；本票只写 `meta.taxonomy_version`。
- 开机自启、设置页、鼠标手势、更新的托盘状态文案在后续票。
