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

/** Effective-taxonomy L2 node, mirrored from `aether-core::navigation` (#27). */
export interface TaxonomyL2 {
  readonly id: string;
  readonly name: string;
  readonly selfBuilt: boolean;
}

export interface TaxonomyL1 {
  readonly id: string;
  readonly name: string;
  readonly icon: string | null;
  readonly l2s: readonly TaxonomyL2[];
}

export interface TaxonomyFormatTag {
  readonly id: string;
  readonly name: string;
  readonly color: string | null;
}

/** Effective taxonomy projection consumed by the HUD (ball column + L2 panel + L3 bar). */
export interface HudNavigation {
  readonly baseVersion: string;
  readonly effectiveVersion: string;
  readonly l1s: readonly TaxonomyL1[];
  readonly formatTags: readonly TaxonomyFormatTag[];
}

/** One slot in the pixel-ball column. L1 slots come from the effective taxonomy. */
export interface BallSlot {
  readonly id: string;
  readonly kind: BallKind;
  readonly label: string;
  readonly placeholder: boolean;
  readonly icon?: string | null;
  readonly l1Id?: string;
}

export interface SearchState {
  /** When true the search bar is expanded; when false the search orb is visible instead. */
  readonly expanded: boolean;
  readonly query: string;
  /** Monotonic token consumed by the renderer to move DOM focus into the search input. */
  readonly focusRequest: number;
}

/** 面板为 S2 的导航态：选中 L1 后原地展开；activeL2Id = null 表示「全部子类」。 */
export interface L2PanelState {
  readonly l1Id: string;
  readonly activeL2Id: string | null;
}

/** L3 标签条本票只落结构与顺序（筛选行为在 #34）。 */
export interface L3BarState {
  readonly expanded: boolean;
}

/** 启动期提示（aether-core 产出 code + detail；5 秒自动关闭由渲染层计时）。 */
export type HudNoticeCode = "taxonomy_overlay_corrupt" | "core_unavailable";

export interface HudNotice {
  readonly code: HudNoticeCode;
  readonly detail: string;
}

export interface KeyHint {
  readonly keys: readonly string[];
  readonly label: string;
}

export interface HudState {
  readonly view: HudView;
  readonly search: SearchState;
  /** 生效分类树投影；null = 壳尚未提供（浏览器调试/核心库启动失败）。 */
  readonly navigation: HudNavigation | null;
  readonly l2Panel: L2PanelState | null;
  readonly l3Bar: L3BarState;
  readonly notice: HudNotice | null;
  readonly phase: HudPhase;
}

export type HudCommand =
  | { readonly type: "summon" }
  | { readonly type: "dismiss-start" }
  | { readonly type: "expand-search" }
  | { readonly type: "collapse-search" }
  | { readonly type: "set-query"; readonly query: string }
  | { readonly type: "set-navigation"; readonly navigation: HudNavigation }
  | { readonly type: "activate-l1"; readonly l1Id: string }
  | { readonly type: "activate-l2"; readonly l2Id: string | null }
  | { readonly type: "toggle-l3-bar" }
  | { readonly type: "collapse-l3-bar" }
  | { readonly type: "show-notice"; readonly notice: HudNotice }
  | { readonly type: "dismiss-notice" };

export interface HudStore {
  getState(): HudState;
  dispatch(command: HudCommand): void;
  subscribe(listener: () => void): () => void;
}
