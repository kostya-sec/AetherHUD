import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { createDefaultHudState, createHudStore } from "./hud-model";
import { connectHudBridge } from "./platform/hudBridge";
import { connectHitBoxPublishing } from "./platform/hitboxes";
import { connectInboxEvents } from "./platform/inbox";
import { connectSettingsEvents } from "./platform/settings";
import { loadHudNavigation } from "./platform/navigation";
import { revealWindowAfterFirstFrame } from "./platform/window";
import "./index.css";

/**
 * Boot order matters:
 * 1. fetch the effective-taxonomy projection from `aether-core` (the ball column
 *    must render real L1s on the very first frame; plain-browser dev falls back to
 *    placeholder balls),
 * 2. render React with that data,
 * 3. register the shell listeners (summon/dismiss),
 * 4. only then tell the shell the webview is ready. The shell emits its first
 *    `hud://summon` from `frontend_ready`, so listeners registered afterwards would
 *    miss the S0 reset (and the dismiss input lock) the shell event drives.
 */
async function boot(): Promise<void> {
  const navigation = await loadHudNavigation();
  const store = createHudStore(createDefaultHudState(navigation));

  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <App store={store} />
    </StrictMode>,
  );

  await connectHudBridge(store);
  // 入库事件（hud://inbox-updated）在监听注册后才不会漏；打开收件箱时据此重载第一页。
  await connectInboxEvents(store);
  // 托盘「设置…」「关于」入口（hud://settings/open、hud://about/open）：打开设置弹层。
  await connectSettingsEvents(store);
  // 点击穿透命中区：hud://summon（窗口上屏）与视口/布局变化后重测，避免沿用窗口
  // 隐藏期间量到的矩形（#29 回归）。装在 frontend_ready 之前，首次唤出不漏。
  const hitBoxes = connectHitBoxPublishing();
  // The Tauri window is created hidden to avoid a transparent-window white flash.
  // 窗口在 frontend_ready 里才上屏：上屏后再量一次，替换掉挂载时的那份。
  void revealWindowAfterFirstFrame().then(() => hitBoxes.request());
}

void boot();
