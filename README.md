# AetherHUD

SAO 风格资源中枢的桌面 HUD。当前进度：**常驻 HUD 外壳**（#25 脚手架 + #26 唤出/收起与系统集成）——Tauri v2 真透明置顶窗口、React 19 前端、框架无关的 HUD 交互模型、全局热键、托盘与单实例。业务数据、分类、摄取管线、卡片流、音效尚未接入（由后续票负责）。

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

- **唤出**：手动启动时 HUD 从屏外滑入并聚焦搜索框（0.5s 内可输入）；Windows 默认 `Alt+```、macOS 默认 `Option+Space`，可后续在设置中重绑。
- **收起**：Esc、失焦（250ms 宽限）、再次热键、托盘左键都只收起；Alt+F4 / Cmd+W / 关闭窗口经 `prevent_close` 也只收起；收起开始即把焦点归还原应用，无需点击即可继续输入。
- **退出**只发生在托盘菜单（设置底部入口随 T22 接入），统一 `app.exit()`。
- **托盘**：左键切换 HUD；右键菜单含 显示/收起、设置…、关于/检查更新、退出；状态只放 tooltip；macOS 使用 template 图标并隐藏 Dock 图标。
- **单实例**：第二个进程不会起窗口，只会唤起既有实例；`--hidden`（登录项/重复触发）只驻留不唤出。
- **点击穿透**：整窗 `set_ignore_cursor_events` + Rust 侧 50ms 光标轮询 hitbox（物理坐标、负坐标），只有搜索框/球/提示条接收点击，空白区域的点击落到桌面。
- **显隐不用 hide()/show()**：窗口只 show 一次并屏外停靠，之后靠 `set_position` 滑入滑出；多显示器按每次唤出重新查询的 `currentMonitor + scaleFactor` 换算并钳制在单屏内。
- 热键注册失败不会中断启动：记录 `HotkeyStatus`（`hud_status` IPC）+ 托盘兜底。

详见 [docs/spec/hud-window-and-shell.md](docs/spec/hud-window-and-shell.md)。

## 前后端契约

- Rust → 前端事件：`hud://summon`（重置 S0 + 聚焦搜索框）、`hud://dismiss`（进入 `dismissing`，吞键到下次唤出）、`hud://settings/open` / `hud://about/open`（T22 / T23 消费）。
- 前端 → Rust 命令：`frontend_ready`（首帧握手，须在事件监听注册后调用）、`request_dismiss`（Esc）、`set_hit_boxes`（交互区矩形）、`hud_status`（阶段/启动形态/热键状态）。

## 结构

- `src/hud-model/`：框架无关的 HUD 状态、命令、选择器与全局键位守卫（vitest 测试缝）。
- `src/components/`：纯渲染层；`RainBackground` 取自 #7 已验收原型。
- `src/platform/`：Tauri 适配——`hudBridge`（外壳事件 + Esc/吞键守卫）、`hitboxes`（点击穿透上报）、`window`（首帧握手）、`windowDrag`（球列拖窗）。
- `src-tauri/src/`：薄 OS 壳——`window/`（几何钳制、显隐状态机、点击穿透、macOS NSPanel）、`tray/`、`hotkey/`、`focus.rs`（焦点归还）、`platform.rs`（光标位置）、`commands.rs`（IPC）。
- `docs/spec/`：施工详规（本票新增 `hud-window-and-shell.md`）。

## 已知限制

- macOS 的 NSPanel / `window_did_resign_key` 路径尚未在真机验证（本机为 Windows），需真机编译与回归；混合 DPI 下的光标换算为近似实现。
- 中文输入法组合态下普通按键表现为输入法 composition 文本（符合 #17 §7），输入法相关验收仍需真机按键回归。
- 开机自启、设置页、鼠标手势、更新的托盘状态文案、卡片流与业务数据在后续票。
