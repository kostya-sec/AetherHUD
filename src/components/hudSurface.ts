import type { CSSProperties } from "react";

/**
 * Shared translucent HUD surface from the #7 prototype: warm near-black at the
 * configured HUD opacity plus an 8px backdrop blur.
 */
export function hudSurfaceStyle(hudOpacity: number): CSSProperties {
  return {
    backgroundColor: `rgba(28, 25, 23, ${hudOpacity})`,
    backdropFilter: "blur(8px)",
  };
}
