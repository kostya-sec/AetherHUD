import { BALL_KEY_HINTS, SEARCH_KEY_HINTS } from "./constants";
import type { BallSlot, HudPhase, HudState, KeyHint } from "./types";

export const selectSearchExpanded = (state: HudState): boolean => state.search.expanded;

export const selectSearchQuery = (state: HudState): string => state.search.query;

export const selectSearchFocusRequest = (state: HudState): number =>
  state.search.focusRequest;

export const selectSearchOrbVisible = (state: HudState): boolean => !state.search.expanded;

export const selectHudPhase = (state: HudState): HudPhase => state.phase;

/** Ball slots in visual order. The search orb only exists while the search bar is collapsed. */
export const selectVisibleBallSlots = (state: HudState): readonly BallSlot[] =>
  state.search.expanded
    ? state.balls.filter((slot) => slot.kind !== "search")
    : state.balls;

/** Bottom hints follow the current focus zone: search bar vs. ball column in T01. */
export const selectKeyHints = (state: HudState): readonly KeyHint[] =>
  state.search.expanded ? SEARCH_KEY_HINTS : BALL_KEY_HINTS;