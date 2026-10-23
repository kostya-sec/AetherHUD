import { invoke } from "@tauri-apps/api/core";

/**
 * Tauri platform adapter: ask the OS shell to reveal the window that was created
 * hidden, once React has mounted.
 *
 * Do not gate this on requestAnimationFrame: a hidden WebView may suspend
 * animation frames (wry/WebView2/WKWebView occlusion behavior), which would leave
 * the process alive but permanently invisible. A zero-delay task always runs; the
 * Tauri side additionally has a timeout fallback show.
 *
 * Resolves once the shell answered (or immediately in plain-browser dev). The shell
 * maps the window inside this call, so callers may re-measure window-relative
 * geometry afterwards (#29 回归：初次启动的 hitbox 量在窗口上屏之前).
 */
export async function revealWindowAfterFirstFrame(): Promise<void> {
  await new Promise<void>((resolve) => {
    globalThis.setTimeout(resolve, 0);
  });
  try {
    await invoke("frontend_ready");
  } catch {
    // Plain-browser dev has no Tauri IPC bridge.
  }
}
