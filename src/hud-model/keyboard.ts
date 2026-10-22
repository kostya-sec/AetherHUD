import type { HudPhase } from "./types";

/** The subset of `KeyboardEvent` the HUD global guard reads. */
export interface KeyDownLike {
  readonly key: string;
  readonly isComposing?: boolean;
  readonly keyCode?: number;
  readonly ctrlKey?: boolean;
  readonly metaKey?: boolean;
  readonly altKey?: boolean;
  readonly shiftKey?: boolean;
}

/**
 * Grace window after a composition ends. WKWebView can deliver `compositionend`
 * before the confirming keydown (WebKit 317127), so the key that ends composition must
 * not be read as a HUD command (#17 §7).
 */
export const COMPOSITION_GRACE_MS = 100;

/** Clock context for the IME grace window. */
export interface KeyGuardContext {
  /** Monotonic "now" in milliseconds, same clock as `compositionEndedAt`. */
  readonly now: number;
  /** Timestamp of the last `compositionend`, or null when the user never composed. */
  readonly compositionEndedAt: number | null;
}

/**
 * What the HUD keyboard router does with a keydown:
 * - `ignore`: default behavior untouched (typing, browser/OS shortcuts, IME).
 * - `swallow`: consume the event without acting (dismiss animation).
 * - `dismiss`: ask the shell to slide the HUD away.
 */
export type HudKeyAction = "ignore" | "swallow" | "dismiss";

const NO_CONTEXT: KeyGuardContext = { now: 0, compositionEndedAt: null };

/**
 * The global layer of the HUD keyboard model that belongs to the OS shell (#26):
 * Esc dismisses, and every key is swallowed while the dismiss animation runs.
 *
 * The five-zone router (T14 / #17 §9.1) will be layered on top of this; the checks
 * below stay first so an IME composition can never be interrupted by the HUD.
 */
export function resolveGlobalKeyDown(
  input: KeyDownLike,
  phase: HudPhase,
  context: KeyGuardContext = NO_CONTEXT,
): HudKeyAction {
  // 收起动画期间吞键优先于输入法让行：此时焦点已归还前台应用，HUD 不能再捡键。
  if (phase === "dismissing") return "swallow";

  // 中文输入法组合态：所有键让给输入法（含 Esc、空格与方向键）。
  if (isComposing(input)) return "ignore";

  // 组合结束宽限窗：确认键可能晚于 compositionend 到达，不能据此收起 HUD。
  if (
    context.compositionEndedAt !== null &&
    context.now - context.compositionEndedAt < COMPOSITION_GRACE_MS
  ) {
    return "ignore";
  }

  // Esc 收起；带 Ctrl/Alt/Meta 的 Esc 属于系统快捷键（如 Windows 开始菜单），交还系统。
  if (input.key === "Escape" && !input.ctrlKey && !input.altKey && !input.metaKey) {
    return "dismiss";
  }

  return "ignore";
}

function isComposing(input: KeyDownLike): boolean {
  return (
    input.isComposing === true || input.keyCode === 229 || input.key === "Process"
  );
}
