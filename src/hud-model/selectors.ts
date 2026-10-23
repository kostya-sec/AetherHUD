import {
  BALL_KEY_HINTS,
  L2_KEY_HINTS,
  L3_KEY_HINTS,
  L3_FILTER_BALL,
  L3_MEDIA_CHIPS,
  PLACEHOLDER_L1_SLOTS,
  QUICK_ADD_BALL,
  SEARCH_BALL,
  SEARCH_KEY_HINTS,
  SETTINGS_BALL,
  SETTINGS_ROW_ITEMS,
  SETTINGS_ROW_KEY_HINTS,
  taxonomyBall,
} from "./constants";
import type {
  BallSlot,
  HudNavigation,
  HudNotice,
  HudPhase,
  HudState,
  InboxEntry,
  InboxState,
  KeyHint,
  L2PanelState,
  QuickInputState,
  CredentialsState,
  SettingsRowItem,
  SettingsState,
  TaxonomyL1,
  ValidationTarget,
} from "./types";

/** L2 面板首项「全部子类」的合成 id（纯 UI 条目，不属于 taxonomy）。 */
export const ALL_CHILDREN_ID = "__all_children__";

export interface L2PanelItem {
  readonly id: string;
  readonly name: string;
  readonly selfBuilt: boolean;
  readonly isAllChildren: boolean;
}

export interface L2PanelView {
  readonly l1: TaxonomyL1;
  readonly items: readonly L2PanelItem[];
  readonly activeL2Id: string | null;
}

export interface L3MediaChip {
  readonly id: string;
  readonly name: string;
}

export interface L3BarView {
  readonly expanded: boolean;
  readonly mediaChips: readonly L3MediaChip[];
  readonly formatTags: HudNavigation["formatTags"];
}

export const selectSearchExpanded = (state: HudState): boolean => state.search.expanded;

export const selectSearchQuery = (state: HudState): string => state.search.query;

export const selectSearchFocusRequest = (state: HudState): number =>
  state.search.focusRequest;

export const selectSearchOrbVisible = (state: HudState): boolean => !state.search.expanded;

export const selectHudPhase = (state: HudState): HudPhase => state.phase;

export const selectNavigation = (state: HudState): HudNavigation | null =>
  state.navigation;

/**
 * Ball slots in visual order. L1 balls are projected from the effective taxonomy and
 * fall back to five inert placeholders until the shell delivers it (#27).
 */
export const selectVisibleBallSlots = (state: HudState): readonly BallSlot[] => {
  const l1Balls = state.navigation
    ? state.navigation.l1s.map(taxonomyBall)
    : PLACEHOLDER_L1_SLOTS;
  const searchBalls = state.search.expanded ? [] : [SEARCH_BALL];
  return [...searchBalls, L3_FILTER_BALL, ...l1Balls, QUICK_ADD_BALL, SETTINGS_BALL];
};

export const selectActiveL1Id = (state: HudState): string | null =>
  state.l2Panel?.l1Id ?? null;

export const selectActiveL2Id = (state: HudState): string | null =>
  state.l2Panel?.activeL2Id ?? null;

export const selectL2Panel = (state: HudState): L2PanelView | null => {
  const panel: L2PanelState | null = state.l2Panel;
  if (!panel || !state.navigation) return null;
  const l1 = state.navigation.l1s.find((node) => node.id === panel.l1Id);
  if (!l1) return null;
  return {
    l1,
    items: [
      { id: ALL_CHILDREN_ID, name: "全部子类", selfBuilt: false, isAllChildren: true },
      ...l1.l2s.map((l2) => ({
        id: l2.id,
        name: l2.name,
        selfBuilt: l2.selfBuilt,
        isAllChildren: false,
      })),
    ],
    activeL2Id: panel.activeL2Id,
  };
};

export const selectL3Bar = (state: HudState): L3BarView => ({
  expanded: state.l3Bar.expanded,
  mediaChips: L3_MEDIA_CHIPS,
  formatTags: state.navigation?.formatTags ?? [],
});

export const selectNotice = (state: HudState): HudNotice | null => state.notice;

export const selectQuickInput = (state: HudState): QuickInputState | null =>
  state.quickInput;

export interface SettingsRowView {
  readonly expanded: boolean;
  readonly items: readonly SettingsRowItem[];
}

export const selectSettingsRow = (state: HudState): SettingsRowView => ({
  expanded: state.settingsRow.expanded,
  items: SETTINGS_ROW_ITEMS,
});

export const selectInbox = (state: HudState): InboxState => state.inbox;

export const selectSettings = (state: HudState): SettingsState => state.settings;

export const selectSettingsOpen = (state: HudState): boolean => state.settings.open;

export const selectSettingsSection = (state: HudState): SettingsState["section"] =>
  state.settings.section;

/** 一次验证结果（按 target 取）；没验证过返回 null。 */
export const selectCredentialValidation = (
  credentials: CredentialsState,
  target: ValidationTarget,
) =>
  credentials.validations.find((entry) => entry.target === target) ?? null;

export interface InboxStatusBadge {
  readonly label: string;
  readonly tone: "pending" | "failed";
}

/**
 * 收件箱行徽标：pending/processing 显示「分类中」，failed 显示「失败」；
 * classified 不属于收件箱管线视图，返回 null。
 */
export const inboxStatusBadge = (
  status: InboxEntry["pipelineStatus"],
): InboxStatusBadge | null => {
  switch (status) {
    case "pending":
    case "processing":
      return { label: "分类中", tone: "pending" };
    case "failed":
      return { label: "失败", tone: "failed" };
    case "classified":
      return null;
  }
};

/** 条目媒体类型的中文展示（收件箱行；L3 筛选 chip 有自己的筛选标签）。 */
export const mediaTypeLabel = (mediaType: string): string => {
  switch (mediaType) {
    case "video":
      return "视频";
    case "article":
      return "文章 / 纯文本";
    case "link":
      return "网页";
    default:
      return mediaType;
  }
};

/** 管线失败原因的中文展示（收件箱行徽标区）。 */
export const failureReasonLabel = (reason: string): string => {
  switch (reason) {
    case "fetch_failed":
      return "失败：抓取失败";
    case "classify_failed":
      return "失败：分类失败";
    default:
      return `失败：${reason}`;
  }
};

/** 底部提示条随焦点区变化：搜索框 / L2 / L3 / 入口行 / 球列。弹窗/收件箱打开时隐藏。 */
export const selectKeyHints = (state: HudState): readonly KeyHint[] => {
  if (state.settingsRow.expanded) return SETTINGS_ROW_KEY_HINTS;
  if (state.search.expanded) return SEARCH_KEY_HINTS;
  if (state.l2Panel) return L2_KEY_HINTS;
  if (state.l3Bar.expanded) return L3_KEY_HINTS;
  return BALL_KEY_HINTS;
};