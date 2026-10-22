import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  resolveGlobalKeyDown,
  type HudKeyAction,
  type HudStore,
} from "../hud-model";

/** Shell → webview events. Keep in sync with src-tauri/src/events.rs. */
export const SUMMON_EVENT = "hud://summon";
export const DISMISS_EVENT = "hud://dismiss";

/** A keydown as the DOM sees it; kept structural so tests need no real KeyboardEvent. */
export interface KeyboardLike {
  readonly key: string;
  readonly isComposing?: boolean;
  readonly keyCode?: number;
  readonly ctrlKey?: boolean;
  readonly metaKey?: boolean;
  readonly altKey?: boolean;
  readonly shiftKey?: boolean;
  preventDefault(): void;
  stopPropagation(): void;
}

/**
 * Applies a `resolveGlobalKeyDown` decision to the real event and asks the shell to
 * dismiss when the HUD owns Esc. Exported for tests: this is the only place where the
 * key guard touches the OS.
 */
export function applyKeyAction(action: HudKeyAction, event: KeyboardLike): void {
  if (action === "ignore") return;
  event.preventDefault();
  event.stopPropagation();
  if (action === "dismiss") {
    dismissHud();
  }
}

/** Asks the OS shell to slide the HUD away (Esc). */
export function dismissHud(): void {
  try {
    void invoke("request_dismiss").catch(() => undefined);
  } catch {
    // Plain-browser dev has no Tauri IPC bridge.
  }
}

/**
 * Wires the HUD model to the OS shell:
 * - `hud://summon` resets to S0 and requests search focus (唤出即输入),
 * - `hud://dismiss` starts the input lock (收起期间吞键),
 * - the capture-phase keydown guard handles Esc and swallows keys while dismissing.
 *
 * Resolves only after both shell listeners are registered: the caller must await it
 * before telling the shell the webview is ready, otherwise the very first summon is
 * emitted into the void and the HUD never gets its focus request.
 *
 * Returns a disposer; in plain-browser dev every Tauri call fails quietly.
 */
export async function connectHudBridge(store: HudStore): Promise<() => void> {
  const subscriptions: Promise<UnlistenFn>[] = [
    listen(SUMMON_EVENT, () => store.dispatch({ type: "summon" })),
    listen(DISMISS_EVENT, () => store.dispatch({ type: "dismiss-start" })),
  ];

  const settled = await Promise.allSettled(subscriptions);
  const unlisteners: UnlistenFn[] = settled.flatMap((result) =>
    result.status === "fulfilled" ? [result.value] : [],
  );

  const onKeyDown = (event: KeyboardEvent): void => {
    applyKeyAction(resolveGlobalKeyDown(event, store.getState().phase), event);
  };
  window.addEventListener("keydown", onKeyDown, { capture: true });

  return () => {
    window.removeEventListener("keydown", onKeyDown, { capture: true });
    for (const unlisten of unlisteners) unlisten();
    unlisteners.length = 0;
  };
}