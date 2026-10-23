import type { CSSProperties } from "react";

/**
 * 全局 HUD 透明度（#7 原型默认 0.65）。外观设置（#46）接入后从设置读取；
 * 设置弹层与其他浮层一律 = 全局透明度 + 3%（#29 验收）。
 */
export const HUD_OPACITY = 0.65;

/** 浮层相对全局透明度的增量（弹窗比 HUD 本体更实一点，保证可读）。 */
export const MODAL_OPACITY_BONUS = 0.03;

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

/** 弹窗/浮层表面：全局透明度 + 3%，上限 1。 */
export function modalSurfaceStyle(hudOpacity: number): CSSProperties {
  return hudSurfaceStyle(Math.min(hudOpacity + MODAL_OPACITY_BONUS, 1));
}
