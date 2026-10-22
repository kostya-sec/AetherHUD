# AetherHUD

SAO 风格资源中枢的桌面 HUD。当前分支是 v1 的**应用脚手架**（#25）：Tauri v2 透明置顶窗口 + React 19 前端 + 框架无关的 HUD 交互模型。业务数据、分类、摄取管线、原型的 saoAudio 音效与卡片流尚未接入（分别由后续票负责）。

## 环境

- Node.js ≥ 22.12（Vite 8）
- Rust ≥ 1.90 与 Tauri v2 的 Windows / macOS 系统依赖
- Windows 需 WebView2 Runtime

## 前端

```powershell
npm install
npm run dev        # 仅前端，http://127.0.0.1:5789
npm run typecheck
npm test           # vitest（hud-model）
npm run build      # tsc + vite build
```

## 桌面 HUD

```powershell
npm run tauri dev    # 启动透明、无边框、置顶的 HUD 窗口
cargo test --workspace
```

## 当前脚手架行为

- 启动进入默认态：搜索框展开但**不自动聚焦**；点击搜索球/输入框后才获得焦点。
- 按住左侧球列任意位置（球体或间隙）可拖动窗口；移动超过 4px 才判定为拖动，轻点搜索球仍是展开搜索框。
- 占位球不可点击、不可 Tab 聚焦，但可按住拖动窗口。
- 雨幕 + 水面背景来自 #7 已验收原型；业务数据、搜索、录入、托盘/热键尚未接入。

## 结构

- `src/hud-model/`：框架无关的 HUD 状态、命令与选择器（vitest 测试缝）。
- `src/components/`：纯渲染层；`RainBackground` 取自 #7 已验收原型。
- `src/platform/`：Tauri 平台适配；首帧后调用 `frontend_ready` 显现隐藏窗口。
- `src-tauri/`：薄 OS 壳；透明窗口配置、`frontend_ready` 命令与 3 秒兜底 show（防止隐藏窗口成为僵尸进程）。
- 规格依据：[实现规格 #24](https://github.com/kostya-sec/AetherHUD/issues/24) §8、#7 原型。
