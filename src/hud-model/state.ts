import { BALL_SLOTS } from "./constants";
import type { HudCommand, HudState, HudStore } from "./types";

/**
 * The default state (S0): ball column with the search box collapsed, no filters, no cards.
 *
 * 2026-10-23 需求变更：唤出不再展开/聚焦搜索框——先显示球列，点击搜索球才展开并聚焦
 * （取代 #16 的「唤出即输入」；见 docs/spec/keyboard-navigation.md 的标注）。
 *
 * `focusRequest` is the token that makes the renderer move DOM focus into the search
 * input; only `expand-search` increments it, so a fresh default state starts at 0 and
 * the renderer acts once the token is > 0.
 */
export function createDefaultHudState(): HudState {
  return {
    view: "default",
    search: {
      expanded: false,
      query: "",
      focusRequest: 0,
    },
    balls: BALL_SLOTS,
    phase: "visible",
  };
}

export function hudReducer(state: HudState, command: HudCommand): HudState {
  // 收起动画期间界面冻结：只有下一次唤出能改变状态（#26：收起期间吞键、开始收起即归还焦点）。
  if (command.type === "summon") {
    // 唤出回到 S0：球列 + 搜索框收起，不请求聚焦（2026-10-23 需求变更）。
    // 这是唯一能打断收起动画的命令。
    return createDefaultHudState();
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
