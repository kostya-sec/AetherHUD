import type { HudCommand, HudNavigation, HudState, HudStore } from "./types";

/**
 * The default state (S0): ball column with the search box collapsed, no filters, no cards.
 *
 * 2026-10-23 需求变更：唤出不再展开/聚焦搜索框——先显示球列，点击搜索球才展开并聚焦
 * （取代 #16 的「唤出即输入」；见 docs/spec/keyboard-navigation.md 的标注）。
 *
 * `navigation` is data, not interaction state: it survives a summon reset so the ball
 * column keeps the effective taxonomy between shows (#27).
 *
 * `focusRequest` is the token that makes the renderer move DOM focus into the search
 * input; only `expand-search` increments it, so a fresh default state starts at 0 and
 * the renderer acts once the token is > 0.
 */
export function createDefaultHudState(
  navigation: HudNavigation | null = null,
): HudState {
  return {
    view: "default",
    search: {
      expanded: false,
      query: "",
      focusRequest: 0,
    },
    navigation,
    l2Panel: null,
    l3Bar: { expanded: false },
    notice: null,
    phase: "visible",
  };
}

export function hudReducer(state: HudState, command: HudCommand): HudState {
  // 唤出回到 S0：球列 + 搜索框收起，不请求聚焦（2026-10-23 需求变更）。
  // 这是唯一能打断收起动画的命令；导航数据属于数据而非交互态，重置时保留。
  if (command.type === "summon") {
    return createDefaultHudState(state.navigation);
  }

  // 导航数据可以在收起动画期间更新（例如首帧晚于一次收起事件）；面板按新树校验。
  if (command.type === "set-navigation") {
    if (state.navigation === command.navigation) return state;
    const panel = state.l2Panel;
    const panelStillValid =
      panel !== null && command.navigation.l1s.some((l1) => l1.id === panel.l1Id);
    return {
      ...state,
      navigation: command.navigation,
      l2Panel: panelStillValid ? panel : null,
    };
  }

  // 收起动画期间界面冻结：只有下一次唤出（与导航数据）能改变状态（#26：收起期间吞键）。
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

    case "activate-l1": {
      const l1 = state.navigation?.l1s.find((node) => node.id === command.l1Id);
      if (!l1) return state;
      // 同一 L1 再点一次 = 原地收起面板。
      if (state.l2Panel?.l1Id === l1.id) {
        return { ...state, l2Panel: null };
      }
      return { ...state, l2Panel: { l1Id: l1.id, activeL2Id: null } };
    }

    case "activate-l2": {
      const panel = state.l2Panel;
      if (!panel) return state;
      const l1 = state.navigation?.l1s.find((node) => node.id === panel.l1Id);
      if (!l1) return state;
      if (command.l2Id === null) {
        if (panel.activeL2Id === null) return state;
        return { ...state, l2Panel: { ...panel, activeL2Id: null } };
      }
      if (!l1.l2s.some((l2) => l2.id === command.l2Id)) return state;
      if (panel.activeL2Id === command.l2Id) return state;
      return { ...state, l2Panel: { ...panel, activeL2Id: command.l2Id } };
    }

    case "toggle-l3-bar":
      return { ...state, l3Bar: { expanded: !state.l3Bar.expanded } };

    case "collapse-l3-bar": {
      if (!state.l3Bar.expanded) return state;
      return { ...state, l3Bar: { expanded: false } };
    }

    case "show-notice":
      return { ...state, notice: command.notice };

    case "dismiss-notice": {
      if (state.notice === null) return state;
      return { ...state, notice: null };
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
