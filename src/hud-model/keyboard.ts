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
 * What the HUD keyboard router does with a keydown:
 * - `ignore`: default behavior untouched (typing, browser/OS shortcuts, IME).
 * - `swallow`: consume the event without acting (dismiss animation).
 * - `dismiss`: ask the shell to slide the HUD away.
 */
export type HudKeyAction = "ignore" | "swallow" | "dismiss";

/**
 * The global layer of the HUD keyboard model that belongs to the OS shell (#26):
 * Esc dismisses, and every key is swallowed while the dismiss animation runs.
 *
 * The five-zone router (T14 / #17 §9.1) will be layered on top of this; the checks
 * below stay first so an IME composition can never be interrupted by the HUD.
 */
export function resolveGlobalKeyDown(input: KeyDownLike, phase: HudPhase): HudKeyAction {
  // 收起动画期间吞键优先于输入法让行：此时焦点已归还前台应用，HUD 不能再捡键。
  if (phase === "dismissing") return "swallow";

  // 中文输入法组合态：所有键让给输入法（含 Esc、空格与方向键）。
  if (input.isComposing === true || input.keyCode === 229 || input.key === "Process") {
    return "ignore";
  }

  // Esc 收起；带 Ctrl/Alt/Meta 的 Esc 属于系统快捷键（如 Windows 开始菜单），交还系统。
  if (input.key === "Escape" && !input.ctrlKey && !input.altKey && !input.metaKey) {
    return "dismiss";
  }

  return "ignore";
}
