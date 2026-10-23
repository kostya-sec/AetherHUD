import { CREDENTIAL_KINDS, DEFAULT_SETTINGS_SECTION } from "./constants";
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
    quickInput: null,
    settingsRow: { expanded: false },
    settings: {
      open: false,
      section: DEFAULT_SETTINGS_SECTION,
      credentials: {
        statuses: [],
        loading: false,
        busy: null,
        validations: [],
        message: null,
      },
    },
    inbox: {
      open: false,
      entries: [],
      nextCursor: null,
      loading: false,
      revision: 0,
    },
    notice: null,
    phase: "visible",
  };
}

/** 保存/清除某凭证后，哪些验证结论会过期（同一外部服务）。 */
function credentialValidationTargets(
  kind: HudState["settings"]["credentials"]["statuses"][number]["kind"],
): ReadonlyArray<HudState["settings"]["credentials"]["validations"][number]["target"]> {
  switch (kind) {
    case "classifierApiKey":
      return ["classifier"];
    case "jinaApiKey":
      return ["jina"];
    case "feishuAppId":
    case "feishuAppSecret":
      return ["feishu"];
  }
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
      // 同一 L1 再点一次 = 原地折叠面板。
      if (state.l2Panel?.l1Id === l1.id) {
        return { ...state, l2Panel: null };
      }
      // 同屏只显示一条展开条：打开/切换 L1 时折叠 L3 条，否则垂直球列上相邻球
      // （尤其 L3 球后紧邻的「00 未分类」）会让 L2 面板与 L3 条互相重叠。
      // 这里只改 L3 条的**展开可见性**，不触碰 #34 的标签筛选选中态。
      return {
        ...state,
        l2Panel: { l1Id: l1.id, activeL2Id: null },
        l3Bar: { expanded: false },
        settingsRow: { expanded: false },
      };
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

    case "toggle-l3-bar": {
      if (state.l3Bar.expanded) {
        return { ...state, l3Bar: { expanded: false } };
      }
      // 同屏只显示一条展开条：展开 L3 条时折叠 L2 面板（两支不会同时占位）。
      // 折叠 L2 不改变任何筛选态——可见性互斥与 #34 的标签选中态是两回事。
      return {
        ...state,
        l2Panel: null,
        settingsRow: { expanded: false },
        l3Bar: { expanded: true },
      };
    }

    case "collapse-l3-bar": {
      if (!state.l3Bar.expanded) return state;
      return { ...state, l3Bar: { expanded: false } };
    }

    case "open-quick-input":
      return {
        ...state,
        quickInput: { seed: command.seed, submitting: false },
      };

    case "close-quick-input": {
      if (state.quickInput === null) return state;
      return { ...state, quickInput: null };
    }

    case "set-quick-input-submitting": {
      if (state.quickInput === null) return state;
      if (state.quickInput.submitting === command.submitting) return state;
      return {
        ...state,
        quickInput: { ...state.quickInput, submitting: command.submitting },
      };
    }

    case "toggle-settings-row": {
      if (state.settingsRow.expanded) {
        return { ...state, settingsRow: { expanded: false } };
      }
      // 同屏只显示一条展开条：入口行展开时折叠 L2 面板与 L3 条。
      return {
        ...state,
        settingsRow: { expanded: true },
        l2Panel: null,
        l3Bar: { expanded: false },
      };
    }

    case "collapse-settings-row": {
      if (!state.settingsRow.expanded) return state;
      return { ...state, settingsRow: { expanded: false } };
    }

    case "open-settings": {
      const section = command.section ?? DEFAULT_SETTINGS_SECTION;
      if (state.settings.open) {
        if (section === state.settings.section) return state;
        return { ...state, settings: { ...state.settings, section } };
      }
      // 与入口行/面板同屏互斥；打开即开始读取凭证状态（loading 防闪烁，数据由平台层送来）。
      return {
        ...state,
        settingsRow: { expanded: false },
        l2Panel: null,
        l3Bar: { expanded: false },
        settings: {
          ...state.settings,
          open: true,
          section,
          credentials: {
            ...state.settings.credentials,
            loading: true,
            busy: null,
            message: null,
          },
        },
      };
    }

    case "close-settings": {
      if (!state.settings.open) return state;
      return {
        ...state,
        settings: {
          ...state.settings,
          open: false,
          credentials: {
            ...state.settings.credentials,
            loading: false,
            busy: null,
          },
        },
      };
    }

    case "select-settings-section": {
      if (!state.settings.open) return state;
      if (state.settings.section === command.section) return state;
      return {
        ...state,
        settings: { ...state.settings, section: command.section },
      };
    }

    case "set-credentials-loading": {
      if (!state.settings.open) return state;
      if (state.settings.credentials.loading === command.loading) return state;
      return {
        ...state,
        settings: {
          ...state.settings,
          credentials: {
            ...state.settings.credentials,
            loading: command.loading,
          },
        },
      };
    }

    case "set-credential-statuses": {
      if (!state.settings.open) return state;
      return {
        ...state,
        settings: {
          ...state.settings,
          credentials: {
            ...state.settings.credentials,
            statuses: command.statuses,
            loading: false,
          },
        },
      };
    }

    case "update-credential-status": {
      if (!state.settings.open) return state;
      const current = state.settings.credentials.statuses;
      const known = current.some((status) => status.kind === command.status.kind);
      const statuses = known
        ? current.map((status) =>
            status.kind === command.status.kind ? command.status : status,
          )
        : // 状态页还没加载完就保存成功时，按四行固定顺序补进列表。
          [...current, command.status].sort(
            (a, b) =>
              CREDENTIAL_KINDS.indexOf(a.kind) - CREDENTIAL_KINDS.indexOf(b.kind),
          );
      // 保存/清除会让相关验证结论过期：只保留与本次无关的 target。
      const staleTargets = credentialValidationTargets(command.status.kind);
      const validations = state.settings.credentials.validations.filter(
        (validation) => !staleTargets.includes(validation.target),
      );
      return {
        ...state,
        settings: {
          ...state.settings,
          credentials: {
            ...state.settings.credentials,
            statuses,
            validations,
            loading: false,
          },
        },
      };
    }

    case "set-credential-busy": {
      if (!state.settings.open) return state;
      if (state.settings.credentials.busy === command.busy) return state;
      return {
        ...state,
        settings: {
          ...state.settings,
          credentials: {
            ...state.settings.credentials,
            busy: command.busy,
          },
        },
      };
    }

    case "set-credential-validation": {
      if (!state.settings.open) return state;
      const validations = state.settings.credentials.validations.filter(
        (validation) => validation.target !== command.validation.target,
      );
      return {
        ...state,
        settings: {
          ...state.settings,
          credentials: {
            ...state.settings.credentials,
            validations: [...validations, command.validation],
          },
        },
      };
    }

    case "set-credential-message": {
      if (!state.settings.open) return state;
      return {
        ...state,
        settings: {
          ...state.settings,
          credentials: {
            ...state.settings.credentials,
            message: command.message,
          },
        },
      };
    }

    case "open-inbox": {
      if (state.inbox.open) return state;
      // 入口行只负责打开，列表数据由平台层取第一页（loading 先置位，避免空态闪烁）。
      return {
        ...state,
        settingsRow: { expanded: false },
        inbox: {
          ...state.inbox,
          open: true,
          entries: [],
          nextCursor: null,
          loading: true,
          revision: state.inbox.revision + 1,
        },
      };
    }

    case "close-inbox": {
      if (!state.inbox.open) return state;
      return {
        ...state,
        inbox: {
          ...state.inbox,
          open: false,
          entries: [],
          nextCursor: null,
          loading: false,
        },
      };
    }

    case "set-inbox-loading": {
      if (!state.inbox.open) return state;
      if (state.inbox.loading === command.loading) return state;
      return { ...state, inbox: { ...state.inbox, loading: command.loading } };
    }

    case "replace-inbox": {
      if (!state.inbox.open) return state;
      return {
        ...state,
        inbox: {
          ...state.inbox,
          entries: command.page.entries,
          nextCursor: command.page.nextCursor,
          loading: false,
        },
      };
    }

    case "append-inbox": {
      if (!state.inbox.open) return state;
      // 翻页期间可能有入库事件重载过第一页：按 id 去重，避免重复行。
      const seen = new Set(state.inbox.entries.map((entry) => entry.id));
      const appended = command.page.entries.filter((entry) => !seen.has(entry.id));
      return {
        ...state,
        inbox: {
          ...state.inbox,
          entries: [...state.inbox.entries, ...appended],
          nextCursor: command.page.nextCursor,
          loading: false,
        },
      };
    }

    case "invalidate-inbox": {
      // 已打开的列表由渲染层 effect 重载第一页；未打开时只记版本，下次打开必然重载。
      return {
        ...state,
        inbox: { ...state.inbox, revision: state.inbox.revision + 1 },
      };
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