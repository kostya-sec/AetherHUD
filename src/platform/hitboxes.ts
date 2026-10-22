import { invoke } from "@tauri-apps/api/core";

/**
 * Marks an element whose area must capture the cursor. Everything else in the HUD
 * window stays click-through, so a click beside the balls reaches the desktop (#26).
 */
export const HITBOX_ATTRIBUTE = "data-hud-hitbox";

/** Window-relative CSS pixel rectangle, the unit the shell converts to physical px. */
export interface CssRect {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

/**
 * Collects the hitboxes of every tagged element. Hidden or zero-sized elements are
 * skipped so the shell never captures the cursor over nothing.
 */
export function collectHitBoxes(root: ParentNode = document): CssRect[] {
  const rects: CssRect[] = [];
  for (const element of root.querySelectorAll<HTMLElement>(
    `[${HITBOX_ATTRIBUTE}]`,
  )) {
    const rect = element.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) continue;
    rects.push({
      x: rect.left,
      y: rect.top,
      width: rect.width,
      height: rect.height,
    });
  }
  return rects;
}

/**
 * Publishes the current hitboxes to the Rust shell. Called whenever the HUD layout
 * changes: the window itself never needs to re-report after a move or DPI change,
 * because the shell re-reads the window origin on every poll.
 */
export function publishHitBoxes(rects: CssRect[] = collectHitBoxes()): void {
  try {
    void invoke("set_hit_boxes", { rects }).catch(() => undefined);
  } catch {
    // Plain-browser dev has no Tauri IPC bridge.
  }
}
