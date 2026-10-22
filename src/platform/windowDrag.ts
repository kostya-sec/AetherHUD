import { getCurrentWindow } from "@tauri-apps/api/window";

/**
 * Tauri platform adapter: hand the current pointer gesture to the OS window move
 * loop. Call it from a pointerdown/pointermove handler so Windows/macOS treat the
 * press as a title-bar drag instead of a click.
 */
export function startWindowDrag(): void {
  try {
    void getCurrentWindow().startDragging().catch(() => undefined);
  } catch {
    // Plain-browser dev has no Tauri IPC bridge.
  }
}
