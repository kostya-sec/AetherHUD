import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { createDefaultHudState, createHudStore } from "./hud-model";
import { connectHudBridge } from "./platform/hudBridge";
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
  // The Tauri window is created hidden to avoid a transparent-window white flash.
  revealWindowAfterFirstFrame();
}

void boot();
