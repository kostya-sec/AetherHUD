/**
 * Framework-free HUD interaction vocabulary.
 *
 * The React layer may only read this state through selectors and change it through
 * commands; no HUD behavior lives in components.
 */

/** v1 HUD view states. T01 only exposes the default state (S0); later tickets add S1–S3. */
export type HudView = "default";

/**
 * Visibility phase driven by the OS shell (#26). `visible` means the shell has
 * summoned the HUD (including the slide-in animation, which must not block input);
 * `dismissing` starts when the shell begins sliding the HUD off screen, and the
 * keyboard router must swallow every key until the next summon.
 */
export type HudPhase = "visible" | "dismissing";

export type BallKind = "search" | "l3-filter" | "l1" | "quick-add" | "settings";

/** One slot in the pixel-ball column. T01 renders placeholders until the effective taxonomy lands. */
export interface BallSlot {
  readonly id: string;
  readonly kind: BallKind;
  readonly label: string;
  readonly placeholder: boolean;
}

export interface SearchState {
  /** When true the search bar is expanded; when false the search orb is visible instead. */
  readonly expanded: boolean;
  readonly query: string;
  /** Monotonic token consumed by the renderer to move DOM focus into the search input. */
  readonly focusRequest: number;
}

export interface KeyHint {
  readonly keys: readonly string[];
  readonly label: string;
}

export interface HudState {
  readonly view: HudView;
  readonly search: SearchState;
  readonly balls: readonly BallSlot[];
  readonly phase: HudPhase;
}

export type HudCommand =
  | { readonly type: "summon" }
  | { readonly type: "dismiss-start" }
  | { readonly type: "expand-search" }
  | { readonly type: "collapse-search" }
  | { readonly type: "set-query"; readonly query: string };

export interface HudStore {
  getState(): HudState;
  dispatch(command: HudCommand): void;
  subscribe(listener: () => void): () => void;
}
