import { DEFAULT_KEY_HINTS } from "./constants";
import type { BallSlot, HudState, KeyHint } from "./types";

export const selectSearchExpanded = (state: HudState): boolean => state.search.expanded;

export const selectSearchQuery = (state: HudState): string => state.search.query;

export const selectSearchFocusRequest = (state: HudState): number =>
  state.search.focusRequest;

export const selectSearchOrbVisible = (state: HudState): boolean => !state.search.expanded;

/** Ball slots in visual order. The search orb only exists while the search bar is collapsed. */
export const selectVisibleBallSlots = (state: HudState): readonly BallSlot[] =>
  state.search.expanded
    ? state.balls.filter((slot) => slot.kind !== "search")
    : state.balls;

export const selectKeyHints = (_state: HudState): readonly KeyHint[] => DEFAULT_KEY_HINTS;
