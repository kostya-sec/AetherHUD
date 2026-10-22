import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { createHudStore } from "./hud-model";
import { connectHudBridge } from "./platform/hudBridge";
import { revealWindowAfterFirstFrame } from "./platform/window";
import "./index.css";

// The store lives outside React: the OS shell drives it through `hud://summon` /
// `hud://dismiss`, and React only renders it.
const store = createHudStore();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App store={store} />
  </StrictMode>,
);

// Handshake order matters: register the shell listeners first, then let the shell
// map the window and (on a manual launch) summon it. The shell emits its first
// `hud://summon` from `frontend_ready`, so a listener registered afterwards would
// miss the focus request that makes 唤出即输入 work.
async function boot(): Promise<void> {
  await connectHudBridge(store);
  // The Tauri window is created hidden to avoid a transparent-window white flash.
  revealWindowAfterFirstFrame();
}

void boot();