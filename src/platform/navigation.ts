import { invoke } from "@tauri-apps/api/core";
import type { HudNavigation, HudNotice, HudStore } from "../hud-model";

/**
 * Shell → webview data for the navigation skeleton (#27): the effective taxonomy
 * projection that drives the ball column, L2 panel and L3 bar. Plain-browser dev
 * has no Tauri IPC bridge, so failures resolve to null and the model keeps its
 * placeholder balls.
 */
export async function loadHudNavigation(): Promise<HudNavigation | null> {
  try {
    return await invoke<HudNavigation>("hud_navigation");
  } catch {
    return null;
  }
}

/** 启动期提示（覆盖层损坏等）；同一提示由核心库保证只给出一次。 */
export async function takeHudNotices(): Promise<HudNotice[]> {
  try {
    return await invoke<HudNotice[]>("hud_take_notices");
  } catch {
    return [];
  }
}

/**
 * Called after each summon: the HUD is visible now, so a startup notice (if any)
 * is delivered once as a 5-second strip. It must run after the summon reset,
 * otherwise the reset would clear the notice.
 */
export async function showStartupNotice(store: HudStore): Promise<void> {
  const notices = await takeHudNotices();
  const first = notices[0];
  if (first) store.dispatch({ type: "show-notice", notice: first });
}
