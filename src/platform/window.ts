import { invoke } from "@tauri-apps/api/core";

/**
 * Tauri platform adapter: ask the OS shell to reveal the window that was created
 * hidden, once React has mounted.
 *
 * Do not gate this on requestAnimationFrame: a hidden WebView may suspend
 * animation frames (wry/WebView2/WKWebView occlusion behavior), which would leave
 * the process alive but permanently invisible. A zero-delay task always runs; the
 * Tauri side additionally has a timeout fallback show.
 */
export function revealWindowAfterFirstFrame(): void {
  globalThis.setTimeout(() => {
    try {
      void invoke("frontend_ready").catch(() => undefined);
    } catch {
      // Plain-browser dev has no Tauri IPC bridge.
    }
  }, 0);
}
