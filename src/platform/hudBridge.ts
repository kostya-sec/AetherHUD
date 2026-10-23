import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  resolveGlobalKeyDown,
  selectHudPhase,
  type HudKeyAction,
  type HudStore,
  type KeyDownLike,
} from "../hud-model";
import { showStartupNotice } from "./navigation";

/** Shell → webview events. Keep in sync with src-tauri/src/events.rs. */
export const SUMMON_EVENT = "hud://summon";
export const DISMISS_EVENT = "hud://dismiss";

/** A keydown as the DOM sees it; kept structural so tests need no real KeyboardEvent. */
export interface KeyboardLike extends KeyDownLike {
  preventDefault(): void;
  stopPropagation(): void;
}

type Scheduler = (task: () => void) => void;

const nextTask: Scheduler = (task) => {
  setTimeout(task, 0);
};

/**
 * Applies a `resolveGlobalKeyDown` decision to the real event and asks the shell to
 * dismiss when the HUD owns Esc.
 *
 * The dismiss request is deferred by one task (#17 §7): the global Esc decision must
 * not race the IME or the webview's own same-frame Esc handling. `schedule` is a seam
 * for tests; production always defers.
 */
export function applyKeyAction(
  action: HudKeyAction,
  event: KeyboardLike,
  schedule: Scheduler = nextTask,
): void {
  if (action === "ignore") return;
  event.preventDefault();
  event.stopPropagation();
  if (action === "dismiss") {
    schedule(dismissHud);
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
 * - `hud://summon` resets to S0 (ball column, search collapsed, no focus request),
 * - `hud://dismiss` starts the input lock (收起期间吞键),
 * - the capture-phase keydown guard handles Esc and swallows keys while dismissing,
 * - `compositionend` timestamps feed the 100 ms IME grace window.
 *
 * Resolves only after both shell listeners are registered: the caller must await it
 * before telling the shell the webview is ready, otherwise the very first summon is
 * emitted into the void and the HUD never gets its focus request.
 *
 * Returns a disposer; in plain-browser dev every Tauri call fails quietly.
 */
export async function connectHudBridge(store: HudStore): Promise<() => void> {
  const subscriptions: Promise<UnlistenFn>[] = [
    listen(SUMMON_EVENT, () => {
      // 先回到 S0，再取启动期提示：顺序反了会被 summon 重置清掉（提示条 5 秒后自动关闭）。
      store.dispatch({ type: "summon" });
      void showStartupNotice(store);
    }),
    listen(DISMISS_EVENT, () => store.dispatch({ type: "dismiss-start" })),
  ];

  const settled = await Promise.allSettled(subscriptions);
  const unlisteners: UnlistenFn[] = settled.flatMap((result) =>
    result.status === "fulfilled" ? [result.value] : [],
  );

  // The guard needs a monotonic clock shared with the composition listener. The
  // webview's performance.now() is that clock; Date.now() could jump.
  let compositionEndedAt: number | null = null;
  const onCompositionEnd = (): void => {
    compositionEndedAt = performance.now();
  };

  const onKeyDown = (event: KeyboardEvent): void => {
    const action = resolveGlobalKeyDown(event, selectHudPhase(store.getState()), {
      now: performance.now(),
      compositionEndedAt,
    });
    applyKeyAction(action, event);
  };

  window.addEventListener("compositionend", onCompositionEnd, { capture: true });
  window.addEventListener("keydown", onKeyDown, { capture: true });

  return () => {
    window.removeEventListener("compositionend", onCompositionEnd, { capture: true });
    window.removeEventListener("keydown", onKeyDown, { capture: true });
    for (const unlisten of unlisteners) unlisten();
    unlisteners.length = 0;
  };
}
