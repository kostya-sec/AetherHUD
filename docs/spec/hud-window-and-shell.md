# HUD 唤出/收起与常驻系统集成（施工依据）

- 票：[#26 唤出/收起与常驻系统集成](https://github.com/kostya-sec/AetherHUD/issues/26)（T02）
- 上位规格：[v1 实现规格 #24](https://github.com/kostya-sec/AetherHUD/issues/24) §8、§10；决策：#4（Tauri 陷阱）、#12（托盘/单实例/收起语义）、#16（唤出焦点）、#17（键盘模型）
- 适用代码：`src-tauri/src/window/`、`tray/`、`hotkey/`、`focus.rs`、`platform.rs`、`commands.rs`；前端 `src/platform/hudBridge.ts`、`hitboxes.ts`、`src/hud-model/keyboard.ts`

## 1. 显隐模型（不使用 hide()/show()）

- 进程启动时窗口 `visible:false`、`focus:false`（tao 创建窗口时可能让隐藏窗口取得前台，故创建期不聚焦）；前端首帧后调用 `frontend_ready`。手动启动：先在**屏外停靠位**（目标 x、监视器底边下方 1px）`show()` 一次再唤出；`--hidden` 驻留启动：**不映射窗口**，直到第一次唤出才「先停靠 → show → 聚焦 → 滑入」，因此开机自启/重复触发不会抢焦点。首次映射之后的窗口始终 visible，显隐只是位置变化。
- 状态机 `window/motion.rs`：`Hidden → Showing → Visible → Dismissing → Hidden`。
  - 动画 `SLIDE_DURATION = 180ms`，12ms/帧，`ease-out-cubic`；新动画通过代数计数器取消旧动画（连击不抖动）。
  - 用户切换（热键/托盘左键）防抖 `TOGGLE_DEBOUNCE = 160ms`；**显式 Esc / 关闭请求不受防抖限制**。
  - 收起动画期间前端进入 `dismissing` 阶段：吞掉所有键（含输入法事件），下一次唤出才解除。
- 唤出顺序（Windows 前台锁）：`show → set_position → set_focus` 紧凑调用，然后发 `hud://summon`，最后滑入。**事件必须晚于 set_focus**，保证窗口一出现就能接收键盘（2026-10-23 需求变更后网页不再自动聚焦搜索框，但窗口先拿到焦点仍是点击搜索球后立即可输入的前提）。滑动起点 = 窗口当前位置（已收起时即屏外停靠位），不做屏内钳制，保证「从屏外滑入」。
- 唤出回到 S0（**2026-10-23 需求变更，取代 #16「唤出即输入」**）：界面为球列 + 搜索框收起，**不自动聚焦**；点击搜索球才展开搜索框并聚焦。`hud://summon` 只负责重置，不含聚焦请求。
- Esc 的全局判定延后一拍（`setTimeout(0)`），且组合结束（`compositionend`）后 100ms 宽限内不判定 Esc（#17 §7，WKWebView 可能先给 compositionend 再给确认键）。
- 唤出动画期间窗口立即可交互（点击搜索球即可展开输入）；收起开始即归还焦点（见 §2）。

## 2. 焦点归还

- 唤出瞬间（HUD 尚未获得焦点）记录前台应用：Windows `GetForegroundWindow`；macOS `NSWorkspace.frontmostApplication` 的 pid。
- 显式收起（Esc / 热键 / 托盘 / 关闭请求）立即 `SetForegroundWindow`（macOS：`NSRunningApplication.activateWithOptions`）后再滑出，前台应用无需点击即可继续输入。
- **失焦收起不抢焦点**（用户已经切走）：只丢弃记录，不 restore。

## 3. 失焦收起

- Windows/通用：Tauri `WindowEvent::Focused(false)`；macOS 另有 NSPanel 的 `window_did_resign_key`（双通道）。
- 均进入 `BLUR_GRACE = 250ms` 宽限：期间重新获得焦点则取消；超时由 50ms supervisor 触发收起。
- 打开菜单、拖拽、动画期间的瞬时失焦由宽限期吸收；`Dismissing` 阶段的失焦事件被忽略。

## 4. 多显示器与 DPI

- 每次唤出重新查询 `available_monitors()`，目标监视器 = 上次可见位置所在屏（否则 `current_monitor`、`primary_monitor`）。**所有监视器统一除以窗口当前 scaleFactor** 得到一个不重叠的逻辑空间（逐屏各除会使混合 DPI 下的逻辑矩形重叠、可能钳到错误屏幕）；窗口跨屏后 scaleFactor 变化，下次唤出重新换算。
- 窗口尺寸每次都按 `outer_size / scaleFactor` 换算，并被钳制在单个监视器内（比屏幕大的窗口以监视器左上角为原点钳制）。
- 收起时记录当前屏幕位置（含用户拖动后的位置）作为下次唤出目标；从未唤出过则居中。

## 5. 点击穿透

- 窗口常态 `set_ignore_cursor_events(true)`（Windows 即 `WS_EX_TRANSPARENT|WS_EX_LAYERED`）。
- 前端把带 `data-hud-hitbox` 的元素矩形（CSS px，相对窗口）通过 `set_hit_boxes` 上报；布局变化（搜索框展开/折叠、resize）时整体替换。
- Rust supervisor 约 50ms 轮询物理光标位置，用**当前**窗口原点与 scaleFactor 换算 hitbox 后做命中测试，仅在结论变化时切换 ignore 标志（负坐标/多屏安全）。
- HUD 收起 / 滑出期间强制穿透。实测：空白区域 `WS_EX_TRANSPARENT` 置位、搜索框区域清除并由 HUD 接收点击。
- 弹窗（快速录入）与列表视图（收件箱）打开时铺满整窗的 backdrop 并标 `data-hud-hitbox`：弹窗内的点击始终落在 webview 里，不会穿透到桌面触发失焦收起，录入过程不被打断（#28）。

## 6. 热键

- 默认：Windows ``Alt+` ``（反引号）、macOS `Option+Space`（`global-shortcut` 插件，仅 Pressed 触发）。
- 注册失败绝不中断启动：捕获错误 + `eprintln!` 日志 + 托盘兜底 + `HotkeyStatus`（`hud_status` IPC，供 T22 设置页显示）。
- 展示串（`display_accelerator`）区分 Windows 文字与 macOS 符号（⌥/⌘/⇧），供托盘与设置复用。

## 7. 托盘（常驻入口）

- 左键切换 HUD；右键菜单：`显示 HUD / 收起 HUD`（文案随状态切换）→ `设置…` → `关于 / 检查更新` → 分隔线 → `退出 AetherHUD`；`show_menu_on_left_click(false)`。
- 状态只进 tooltip（`AetherHUD · 显示中/已收起 · 唤出 Alt + ```）；分类中/待处理/离线挂起文案随管线票接入。
- macOS 使用 template 图标（`icons/tray-template.png` + `icon_as_template(true)`），并设 Accessory 激活策略（不占 Dock）。
- `设置…` / `关于` 先唤出 HUD，再发 `hud://settings/open` / `hud://about/open`，分别由 T22 / T23 消费。

## 8. 单实例与启动参数

- `tauri-plugin-single-instance` **最先注册**；第二实例无参 → 唤起并置前；带 `--hidden` → 仅驻留（登录项重复触发场景）。
- 手动启动直接显示 HUD；`--hidden` 首次启动只驻留（`frontend_ready` 时停靠、不唤出）。
- 开发版标识符 `io.github.kostya-sec.aetherhud.dev`（`npm run tauri:dev --config src-tauri/tauri.dev.conf.json`），避免与安装版争单实例锁；发布版 `io.github.kostya-sec.aetherhud`。
- 退出只发生在托盘菜单与（后续）设置底部，统一 `app.exit(0)`。

## 9. 关闭与退出

- `CloseRequested`（Alt+F4 / Cmd+W / 系统关闭）→ `prevent_close` + 收起，进程与摄取继续。
- Esc、失焦、热键、托盘左键 = 收起；退出唯一入口见 §8。

## 10. 前后端契约

| 方向 | 名称 | 语义 |
| --- | --- | --- |
| Rust → 前端 | `hud://summon` | 开始唤出：模型重置 S0（球列 + 搜索框收起，不请求聚焦） |
| Rust → 前端 | `hud://dismiss` | 开始收起：进入 `dismissing`，吞键直到下次唤出 |
| Rust → 前端 | `hud://settings/open` / `hud://about/open` | 托盘入口（T22/T23 消费） |
| 前端 → Rust | `frontend_ready` | 首帧握手：注册事件监听之后调用；触发一次性映射与首次唤出 |
| 前端 → Rust | `request_dismiss` | Esc 收起 |
| 前端 → Rust | `set_hit_boxes` | 交互区矩形（CSS px，窗口相对） |
| 前端 → Rust | `hud_status` | 阶段、启动形态、热键注册状态 |
| 前端 → Rust | `hud_navigation` | 生效分类树投影：5 个 L1 + L2 + 形态标签（#27） |
| 前端 → Rust | `hud_take_notices` | 取启动期提示（覆盖层损坏 / 核心库未启动），每次取走即清空（#27） |
| 前端 → Rust | `ingest_text` | 本地录入：URL 进 fetch stage、纯文本直入 classify stage；双轨去重 + entries/entry_sources/pipeline_jobs 同事务（#28） |
| 前端 → Rust | `hud_inbox` | 收件箱一页（管线视图 = pending/processing/failed；50 条/页 keyset 分页，created_at 倒序）（#28） |
| Rust → 前端 | `hud://inbox-updated` | 入库成功（含重复投递刷新）：收件箱打开时重载第一页（#28） |
| Rust → 前端（事件顺序） | 唤出：focus 先于 `hud://summon` | 窗口先拿到系统焦点，键盘立即可用（2026-10-23 起不再自动聚焦搜索框） |

## 11. 验证

自动（本仓库可跑）：

- Rust 单测：几何钳制/停靠、动画缓动、状态机（防抖/失焦宽限/打断）、hitbox 命中与 ignore 翻转、启动参数、热键解析与展示。
- 前端单测：S0 重置与 focusRequest、dismissing 吞键、Esc 守卫、IPC 动作。
- Windows 真机手工验收清单（#24 决议：不新增 OS 壳自动化缝，v1 不做 WebDriver E2E）：启动唤出（默认球列）、点击搜索球展开并输入、点击穿透（`WS_EX_TRANSPARENT` 随 hitbox 切换）、热键/Esc 收起与焦点归还、WM_CLOSE 收起、第二实例（含 `--hidden`）、`--hidden` 首次启动驻留。本地可写脚本跑，但不入库。

真机回归（本机为 Windows，macOS 未实测）：

- macOS：`NSPanel` 转换与 `window_did_resign_key` 需在真机**编译并回归**（本票仅在源码层校对 crate API，未做 macOS 交叉编译）；混合 DPI 下的光标换算为近似实现，需实测。
- 中文输入法：普通按键在组合态表现为输入法 composition 文本（符合 #17 §7），Esc 在组合态只取消候选；K1/K7/K10 的输入法项仍需真机按键回归。

## 12. v1 不做

- 不做开机自启注册 UI（T22）；不建手势召回路（#16/T24）；不做 macOS 非激活面板/Spotlight 式免激活唤出；不做点击穿透的逐控件光标样式；不做 HudWindow 材质/vibrancy（外观票/#8 范围）。
