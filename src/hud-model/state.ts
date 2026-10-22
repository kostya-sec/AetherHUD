import { BALL_SLOTS } from "./constants";
import type { HudCommand, HudState, HudStore } from "./types";

/** The summoned default state: empty search bar, search expanded, no filters, no cards. */
export function createDefaultHudState(focusRequest = 1): HudState {
  return {
    view: "default",
    search: {
      expanded: true,
      query: "",
      focusRequest,
    },
    balls: BALL_SLOTS,
  };
}

export function hudReducer(state: HudState, command: HudCommand): HudState {
  switch (command.type) {
    case "summon":
      return createDefaultHudState(state.search.focusRequest + 1);

    case "expand-search": {
      if (state.search.expanded) return state;
      return {
        ...state,
        search: {
          ...state.search,
          expanded: true,
          focusRequest: state.search.focusRequest + 1,
        },
      };
    }

    case "collapse-search": {
      if (!state.search.expanded) return state;
      return {
        ...state,
        search: { ...state.search, expanded: false },
      };
    }

    case "set-query": {
      if (state.search.query === command.query) return state;
      return {
        ...state,
        search: { ...state.search, query: command.query },
      };
    }
  }
}

/** Tiny observable store; React binds to it with useSyncExternalStore. */
export function createHudStore(initialState: HudState = createDefaultHudState()): HudStore {
  let state = initialState;
  const listeners = new Set<() => void>();

  return {
    getState: () => state,
    dispatch(command) {
      const next = hudReducer(state, command);
      if (next === state) return;
      state = next;
      for (const listener of listeners) listener();
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}
