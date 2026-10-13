# 调研：Tauri v2 实现 SAO 风格悬浮 HUD 的跨平台陷阱清单

- **调研日期**：2026-10-13
- **对应 issue**：[kostya-sec/AetherHUD#4](https://github.com/kostya-sec/AetherHUD/issues/4)
- **目标读者**：src-tauri/src/window/ 模块实现者
- **资料范围**：优先官方文档（tauri.app / docs.rs / Microsoft Learn / 官方 schema）与 tauri-apps 官方仓库 issue/commit；社区实战经验仅作补充并单独标注。

---

## TL;DR（最重要结论）

1. **Windows 透明窗口谨慎使用 hide()/show() 路线**：hide() 会破坏 Acrylic/Mica 背景、触发白闪，且 WebView2 的 OLE drop target 需要"先合成再拖拽"。用 visible:true + 屏外停靠（如 x:-32000）或 noRedirectionBitmap 规避。
2. **无边框窗口 + shadow:true 在 Windows 会产生 1px 白边**（官方 schema 明确记载），HUD 必须 shadow:false。
3. **点击穿透没有"按像素穿透"官方支持**（tauri#13070 已关闭为 won't-implement）：只能整窗 set_ignore_cursor_events 并按光标位置动态切换，或用 hitbox 轮询方案。
4. **macOS 上 hide() 不会把焦点还给前一个 App**（tauri#7540，v2 仍未修）：正解是 tauri-nspanel 把窗口转成 NSPanel。
5. **global-shortcut 注册失败只是 Err 不是 panic**：冲突时 Windows 报 FailedToRegister、重复注册报 AlreadyRegistered；必须 catch + 日志 + 托盘兜底，绝不能中断启动。
6. **多显示器下 availableMonitors() 返回物理像素，WebviewWindow 的 x/y/width/height 要逻辑像素**（tauri#15519）：混合 DPI 下不换算必错位。
7. **Windows 前台锁（foreground lock）是系统级限制**：热键回调里 set_focus() 大多能成功（因为"收到最后输入事件"豁免），但 tao 内部仍有模拟 Alt 键的"偷焦点"hack，行为并非 100% 可靠，需做防抖和失败兜底。

---

## 1. Windows：transparent + decorations:false 的 DWM 合成问题

### 1.1 已知问题清单

| 问题 | 现象 | 来源 |
|---|---|---|
| 1px 白边 | 无边框窗口设 shadow:true 时出现 1px 白色边框；Win11 还会强加圆角 | [tauri config schema（官方）](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-cli/config.schema.json)：shadow — "true will make undecorated window have a 1px white border, and on Windows 11, it will have a rounded corners" |
| 创建时白闪 | 透明窗口在 WebView 内容渲染前出现白色闪烁 | 同上 schema：noRedirectionBitmap — "This can avoid the white flash that may appear before the webview content is rendered when using a transparent window. Windows only"（设置 WS_EX_NOREDIRECTIONBITMAP） |
| 缩放/全屏黑边白边 | transparent:false 时快速 resize 出现黑/白条、UI 跟不上鼠标；全屏切换时边缘露壁纸、旧标题栏按钮闪 1-2 帧 | [tauri#13270](https://github.com/tauri-apps/tauri/issues/13270)（OPEN，tauri 2.12.1 + tao 0.37.1 仍复现） |
| 启动白闪 vs 缩放流畅的权衡 | transparent:true 解决 resize 伪影，但 release 版首次加载有闪烁 | tauri#13270 评论：解法为 visible:false 启动 + 前端渲染完成后由 Rust 命令 show() |
| hide() 破坏 Acrylic | hide()→show() 后 Acrylic 效果丢失，残留黑色/纯色背景；每次 show 重新 apply 会闪烁 + DWM 重组卡顿 | 社区实战：[dev.to《Building a Transparent Drag-and-Drop Shelf》](https://dev.to/devrayat000/building-a-transparent-drag-and-drop-shelf-for-windows-with-tauri-the-engineering-war-stories-3c8c) |
| WebView2 静默拒收拖拽 | 窗口在拖拽开始前未被 DWM 合成时，IDropTarget 未注册，拖放静默失败 | 同上（dev.to）；另有 dragDropEnabled 在 Rust builder 设置会被静默忽略（tauri#13761），必须写在 tauri.conf.json |
| blur/acrylic 性能差 | apply_blur 在 Win11 22621+ 缩放/拖动性能差；apply_acrylic 在 Win10 1903+/Win11 22000 同样 | [window-vibrancy README（官方插件）](https://github.com/tauri-apps/window-vibrancy) |

### 1.2 规避方案（window 模块必须落实）

- tauri.conf.json："transparent": true, "decorations": false, "shadow": false，并加 "noRedirectionBitmap": true。
- **显隐策略二选一**：
  - A 方案（推荐，HUD 场景）：visible:false 启动，前端首帧渲染完成后调 Rust command 再 show()，消除启动白闪（tauri#13270 评论方案）。
  - B 方案（需要 OS 级拖放时）：visible:true + 启动即 set_position 停靠屏外（如 x:-32000），之后"显示/隐藏"全部用 set_position 滑入滑出，**永不调用 hide()**（dev.to 实战方案，同时保住 Acrylic 和 drop target）。
- 若用 window-vibrancy：优先 apply_mica（Win11，无性能问题标注），避免 apply_blur；失败时逐级 fallback。

## 2. 点击穿透（set_ignore_cursor_events）的正确用法与坑

### 2.1 事实

- 官方**不支持**"透明像素自动穿透"：功能请求 [tauri#13070](https://github.com/tauri-apps/tauri/issues/13070) 被明确关闭（"This won't be implemented"），指向 #2090 的变通与 #6164 跟踪。
- 现有 API 只有**整窗**开关 set_ignore_cursor_events(bool)（JS: getCurrentWindow().setIgnoreCursorEvents(true)，见 [@tauri-apps/api window.ts](https://github.com/tauri-apps/tauri/blob/dev/packages/api/src/window.ts)）；Webview 级独立控制的需求见 [tauri#10564](https://github.com/tauri-apps/tauri/issues/10564)。
- 开启后窗口**收不到任何鼠标事件**（包括 mousemove/hover），前端无法自己决定何时关回去——必须在 Rust 侧轮询光标位置做状态切换。

### 2.2 规避方案

- **光标轮询 + 动态切换**（SAO HUD 推荐）：Rust 线程每 ~50ms 读全局光标坐标（Win32 GetCursorPos / macOS CGEventGetLocation），与 HUD 各交互区 hitbox 比对：进入热区 set_ignore_cursor_events(false)，离开恢复 true。社区有完整 demo：[tauri-clickthrough-demo](https://github.com/Xinyu-Li-123/tauri-clickthrough-demo)（tauri#13070 评论中被推荐为"next closest alternative"），dev.to 文章也采用同一架构。
- 状态用原子量做单一事实源，避免轮询线程与 IPC 命令竞争。
- hitbox 要用**物理像素**（见第 6 节），且多显示器坐标是全局虚拟桌面坐标，可能为负值。
- 简化策略：若 HUD 只在热键唤起期间可交互，可在唤起时关穿透、隐藏时开穿透，避免常驻轮询。

## 3. macOS：vibrancy、NSPanel 与透明窗口注意事项

### 3.1 vibrancy/毛玻璃插件现状（官方 [window-vibrancy](https://github.com/tauri-apps/window-vibrancy)）

- apply_vibrancy 支持 macOS 10.10+，材质枚举含 NSVisualEffectMaterial::HudWindow 等；macOS 26+ 新增 apply_liquid_glass（NSGlassEffectView，可设 radius/tint/interactive，interactive 需 macOS 27+）。
- **Linux 不支持**（由 compositor 决定）。
- Tauri 集成三个必需配置：
  1. CSS: html, body { background: transparent }；
  2. "windows": [{ "transparent": true }]；
  3. macOS 需 "macOSPrivateApi": true（私有 API，上架 App Store 有风险；HUD 类工具一般走直接分发，可接受）。
- 已知小坑：vibrancy 与保留窗口边框不能兼得（[window-vibrancy#49](https://github.com/tauri-apps/window-vibrancy/issues/49)）——对 HUD（decorations:false）无影响。

### 3.2 NSPanel 级行为（Spotlight 式 HUD 的关键）

普通 NSWindow 做不到"不激活 App、不抢主窗焦点、失焦自动隐藏、浮于全屏空间之上"。社区标准解法是 [ahkohd/tauri-nspanel](https://github.com/ahkohd/tauri-nspanel)（v2 分支，crates.io 有 tauri-nspanel 2.1.1，被 Cap、EcoPaste、Overlayed 等生产项目使用）：

- window.to_panel() 把 WebviewWindow swizzle 成 NSPanel（**每个窗口只能调一次**）。
- 关键动因：[tauri#7540（OPEN，v2 未修）](https://github.com/tauri-apps/tauri/issues/7540)——appWindow.hide() 不会把焦点交还给之前的 App；官方维护者评论承认 macOS 不允许跨 App 聚焦窗口，NSPanel 是社区验证的解决方案。
- 配套能力：panel_delegate! 宏监听 window_did_become_key / window_did_resign_key（实现"失焦自动隐藏"）；set_released_when_closed(true) 才能在 close() 后真正释放（NSPanel 默认 releasedWhenClosed=false，因为它按"轻量可复用"设计）。
- 参考实现：[ahkohd/tauri-macos-spotlight-example](https://github.com/ahkohd/tauri-macos-spotlight-example)。
- 注意：visibleOnAllWorkspaces 配置项仅 macOS 有效（schema 标注 Windows 不支持）；转成 panel 后跨空间行为可自行用 collectionBehavior 控制。

### 3.3 其他 macOS 透明窗口注意点

- 焦点/blur 事件历史上在 macOS 不派发（[tauri#2485](https://github.com/tauri-apps/tauri/issues/2485)，CLOSED 但属回归史）；**不要盲信 tauri://blur 一定到达**，失焦隐藏建议双保险：NSPanel delegate 的 window_did_resign_key + 前端 tauri://blur 监听 + 超时兜底。
- tao 在 macOS 上 WebviewWindow 不总是遵守初始 x,y（[tao#1023](https://github.com/tauri-apps/tao/issues/1023)）：创建后再显式 set_position 一次。

## 4. global-shortcut 插件：注册失败原因与冲突检测

### 4.1 注册失败的常见原因（基于 [global-hotkey crate 文档](https://docs.rs/global-hotkey/latest/global_hotkey/)与[插件 README](https://github.com/tauri-apps/plugins-workspace/tree/v2/plugins/global-shortcut)）

| 原因 | 错误表现 | 平台 |
|---|---|---|
| 快捷键已被**本进程**注册 | Error::AlreadyRegistered(HotKey) | 全平台 |
| 被**系统或其他 App**占用 | Error::FailedToRegister(String)（Windows 上即 RegisterHotKey 冲突错误） | 主要 Windows |
| 快捷键字符串解析失败 | Error::HotKeyParseError（如 cmd 与 CommandOrControl 写法差异） | 全平台 |
| 线程模型错误 | Windows：manager 必须与 win32 事件循环同线程创建；macOS：必须在**主线程**创建——违反时行为异常甚至 panic（见 [Discussion #12991 Global Shortcuts Panic (MacOS)](https://github.com/orgs/tauri-apps/discussions/12991)）；Tauri 插件内部已处理，但绕过插件直接用 global-hotkey crate 时必踩 | Win/macOS |
| Linux Wayland | 协议安全禁止后台全局热键抓取（crate 仅支持 X11） | Linux（本项目不覆盖） |

### 4.2 冲突检测手段与工程实践

- **没有跨平台"查询冲突"API**：只能"尝试注册 → 捕获 Err"。Windows 上被占用会明确报错；**macOS 上系统快捷键（如 Cmd+Space）不会报错但永远不会触发回调**——检测只能靠引导用户换绑。
- 注册失败必须 **catch + 日志 + 继续启动**，绝不能 unwrap 中断 setup；生产案例（asyar 启动器）：冲突不 crash、无自动 fallback，用户提供托盘入口进设置页改绑（[asyar 文档](https://github.com/Xoshbin/asyar/blob/main/docs/explanation/host-os-integration.md)，社区参考）。
- 用 Rust 侧 Builder::new().with_shortcuts([...]).with_handler(...) 注册（v2 推荐方式，见插件 README）；handler 里判断 event.state == ShortcutState::Pressed，否则按一下会触发 Pressed+Released 两次。
- Alt+Space 与 Windows 系统窗口菜单、Option+Space 与部分输入法有天然冲突风险，默认热键必须可在设置中重绑定。

## 5. 焦点行为：Windows 前台锁与失焦隐藏可靠性

### 5.1 Windows 前台锁（foreground lock）

[Microsoft Learn: SetForegroundWindow remarks](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow) 明确：系统限制进程抢前台，除非满足其一——前台锁超时已过（SPI_GETFOREGROUNDLOCKTIMEOUT）、调用进程是前台进程、由前台进程启动、**收到最后输入事件**、当前无前台窗口、任一方被调试；且前台进程可用 LockSetForegroundWindow 禁用。不满足时 Windows 只闪任务栏按钮。

- **好消息**：热键回调路径天然满足"收到最后输入事件"（用户刚按下热键），所以热键弹出窗口 + set_focus() 在多数情况合法可用。
- **坏消息**：tao 内部仍保留"模拟 Alt 键偷前台权限"的 hack（[tao@62db431](https://github.com/tauri-apps/tao/commit/62db431338636dd0adafca15d636c08f68941984)，先正常 SetForegroundWindow，不行再 hack），说明官方也认为单纯 API 调用不可靠。推论：
  - 热键 handler 里按 show() → set_position() → set_focus() 顺序紧凑调用，中间不要插入长 await；
  - visible:false 冷启动首秀耗时（WebView 初始化）可能让用户连按热键，需做**唤起状态机防抖**（hidden → showing → visible）；
  - "No menus are active" 等条件不满足时抢前台会被拒、任务栏闪烁，属系统行为无需处理。

### 5.2 失焦（blur）自动隐藏的可靠性

- macOS：hide() 不还焦点（#7540）→ 用 NSPanel（第 3.2 节）；tauri://blur 有历史回归（#2485）→ 双通道监听。
- 生产实践（asyar）：设 **blur 宽限期**（其用 250ms BLUR_HIDE_GRACE），忽略窗口获得焦点前到达的虚假 Focused(false) 事件——HUD 唤起瞬间常有"先 blur 后 focus"的事件乱序，不设宽限期会导致刚弹出就被自己隐藏。
- 点击穿透开启时窗口拿不到焦点/失焦事件，blur 隐藏逻辑要与第 2 节的穿透状态机联动（唤起时先关穿透）。

## 6. 多显示器与高 DPI 下的窗口定位

### 6.1 核心事实（官方）

- availableMonitors() / currentMonitor() 返回的 position/size 是**物理像素**，并附 scaleFactor；而 WebviewWindow 创建参数 x/y/width/height 与 setPosition(LogicalPosition) 用**逻辑像素**。官方 API 注释明确要求 "convert with Monitor.scaleFactor first"（[@tauri-apps/api window.ts Monitor 注释](https://github.com/tauri-apps/tauri/blob/dev/packages/api/src/window.ts)）。
- 该换算缺失是真实事故源：[tauri#15519](https://github.com/tauri-apps/tauri/issues/15519)（CLOSED，要求 JS Monitor API 增加逻辑 bounds；关联 #12167、#7139、tao#816）——macOS Retina + 外接 1x 显示器混用时，直接把物理 position 传给窗口选项会把窗口放到错误位置。
- 混合 DPI 下窗口跨屏还会出现尺寸溢出/无法调和，生产项目的修法是**把窗口钳制在单个显示器内**（openhuman PR #5221、cc-switch PR #6056，社区参考）。

### 6.2 规避方案

- 封装唯一的坐标换算函数：logical = physical / scaleFactor（Rust 侧 PhysicalPosition::to_logical(scale_factor)），禁止散落换算。
- HUD 定位基准用 currentMonitor()（光标所在屏）而非主屏；获取光标全局坐标（物理）→ 找到包含它的显示器 → 在该屏逻辑坐标系内居中/靠边。
- 唤起时先 set_position 再 show（macOS 上尤其要显式 set，见 tao#1023），避免在旧位置闪一帧。
- 显示器配置会运行时变化（拔插、缩放比调整）：每次唤起重新查询，不缓存。
- 跨屏拖动 HUD 的需求直接不做（钳制单屏），省掉混合 DPI 的 size 换算地狱。

## 7. window 模块实现注意事项汇总（落地清单）

| # | 检查项 | 方案 |
|---|---|---|
| 1 | 白边/圆角 | decorations:false + shadow:false（schema 明示 shadow:true → 1px 白边） |
| 2 | 启动白闪 | noRedirectionBitmap:true（Windows）；visible:false 首帧渲染后 show |
| 3 | resize/全屏伪影 | 保持 transparent:true（#13270 证实反而消除伪影） |
| 4 | Acrylic/Mica 丢失 | 不用 hide()；屏外停靠或重建时重新 apply |
| 5 | 点击穿透 | 整窗 set_ignore_cursor_events + Rust 光标轮询 hitbox 切换；物理坐标、负坐标 |
| 6 | macOS 焦点归还 | tauri-nspanel 转 NSPanel；window_did_resign_key 做失焦隐藏 |
| 7 | 毛玻璃 | window-vibrancy：HudWindow / apply_liquid_glass(macOS 26+)；macOSPrivateApi:true |
| 8 | 热键注册失败 | catch Err，日志 + 托盘兜底 + 可重绑；ShortcutState::Pressed 去抖 |
| 9 | 前台锁 | 热键 → show → set_position → set_focus 紧凑顺序；唤起状态机防抖 |
| 10 | blur 误隐藏 | ~250ms blur 宽限期；blur/focus 双通道监听 |
| 11 | 多显示器/DPI | currentMonitor + scaleFactor 换算；每次唤起重查；钳制单屏 |
| 12 | macOS 初始位置 | 创建后显式 set_position（tao#1023） |

## 来源索引

**官方/一手**
- tauri config schema（transparent/shadow/noRedirectionBitmap/visibleOnAllWorkspaces 描述）：https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-cli/config.schema.json
- @tauri-apps/api window.ts（setIgnoreCursorEvents/Monitor 注释）：https://github.com/tauri-apps/tauri/blob/dev/packages/api/src/window.ts
- window-vibrancy README：https://github.com/tauri-apps/window-vibrancy
- tauri-plugin-global-shortcut README：https://github.com/tauri-apps/plugins-workspace/tree/v2/plugins/global-shortcut
- global-hotkey crate（线程约束、Error 枚举）：https://docs.rs/global-hotkey/latest/global_hotkey/
- tauri#13270（transparent 与 resize 伪影）：https://github.com/tauri-apps/tauri/issues/13270
- tauri#13070（点击穿透 won't-implement）：https://github.com/tauri-apps/tauri/issues/13070
- tauri#7540（macOS hide 不还焦点，OPEN）：https://github.com/tauri-apps/tauri/issues/7540
- tauri#2485（focus/blur 事件回归史）：https://github.com/tauri-apps/tauri/issues/2485
- tauri#15519（Monitor 物理/逻辑像素，CLOSED）：https://github.com/tauri-apps/tauri/issues/15519
- tao#1023（macOS 初始 x/y 不生效）：https://github.com/tauri-apps/tao/issues/1023
- tao@62db431（SetForegroundWindow + focus hack）：https://github.com/tauri-apps/tao/commit/62db431338636dd0adafca15d636c08f68941984
- Discussion #12991（macOS 热键 panic）：https://github.com/orgs/tauri-apps/discussions/12991
- Microsoft Learn SetForegroundWindow：https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow
- tauri-nspanel：https://github.com/ahkohd/tauri-nspanel 、https://docs.rs/crate/tauri-nspanel
- tauri-macos-spotlight-example：https://github.com/ahkohd/tauri-macos-spotlight-example

**社区实战（二手，已标注）**
- dev.to《Building a Transparent Drag-and-Drop Shelf for Windows with Tauri》：https://dev.to/devrayat000/building-a-transparent-drag-and-drop-shelf-for-windows-with-tauri-the-engineering-war-stories-3c8c
- tauri-clickthrough-demo：https://github.com/Xinyu-Li-123/tauri-clickthrough-demo
- asyar Host OS Integration 文档（blur 宽限期、热键失败兜底）：https://github.com/Xoshbin/asyar/blob/main/docs/explanation/host-os-integration.md
