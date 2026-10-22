import { BALL_SLOTS } from "./constants";
import type { HudCommand, HudState, HudStore } from "./types";

/**
 * The summoned default state: empty search bar, search expanded, no filters, no cards.
 *
 * `focusRequest` is the token that makes the renderer move DOM focus into the search
 * input. Summoning resets to S0 *and* asks for focus (#16 K1: 唤出即输入), so a fresh
 * default state start at request 0 and the renderer only acts once the token is > 0.
 */
export function createDefaultHudState(focusRequest = 0): HudState {
  return {
    view: "default",
    search: {
      expanded: true,
      query: "",
      focusRequest,
    },
    balls: BALL_SLOTS,
    phase: "visible",
  };
}

export function hudReducer(state: HudState, command: HudCommand): HudState {
  // 收起动画期间界面冻结：只有下一次唤出能改变状态（#26：收起期间吞键、开始收起即归还焦点）。
  if (command.type === "summon") {
    // 唤出回到 S0 并聚焦搜索框；这是唯一能打断收起动画的命令。
    return createDefaultHudState(state.search.focusRequest + 1);
  }
  if (state.phase === "dismissing") return state;

  switch (command.type) {
    case "dismiss-start":
      return { ...state, phase: "dismissing" };

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
