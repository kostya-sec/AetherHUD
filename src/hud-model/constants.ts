import type { BallSlot, KeyHint, SettingsRowItem, TaxonomyL1 } from "./types";

/**
 * Ball column order (#17 §5.4 / PRD v2 §8):
 * search orb (only while the search bar is collapsed) → L3 filter → five L1 balls
 * (from the effective taxonomy) → quick input → settings.
 */
export const SEARCH_BALL: BallSlot = {
  id: "search",
  kind: "search",
  label: "全局搜索",
  placeholder: false,
};

export const L3_FILTER_BALL: BallSlot = {
  id: "l3-filter",
  kind: "l3-filter",
  label: "L3 标签筛选",
  placeholder: false,
};

export const QUICK_ADD_BALL: BallSlot = {
  id: "quick-add",
  kind: "quick-add",
  label: "快速录入",
  placeholder: false,
};

export const SETTINGS_BALL: BallSlot = {
  id: "settings",
  kind: "settings",
  label: "全局设置入口",
  placeholder: false,
};

/**
 * Fallback slots while the shell has not delivered the effective taxonomy
 * (plain-browser dev, or a failed core bootstrap). Labels stay neutral: the effective
 * taxonomy is the single runtime source of L1 names (ADR-0001), so the fallback must
 * not duplicate official names that would drift on a taxonomy upgrade.
 */
export const PLACEHOLDER_L1_SLOTS: readonly BallSlot[] = [
  "分类加载中",
  "分类加载中",
  "分类加载中",
  "分类加载中",
  "分类加载中",
].map((label, index) => ({
  id: `l1-placeholder-${index + 1}`,
  kind: "l1" as const,
  label,
  placeholder: true,
}));

export function taxonomyBall(l1: TaxonomyL1): BallSlot {
  return {
    id: l1.id,
    kind: "l1",
    label: l1.name,
    placeholder: false,
    icon: l1.icon,
    l1Id: l1.id,
  };
}

export const SEARCH_KEY_HINTS: readonly KeyHint[] = [
  { keys: ["Esc"], label: "收起" },
  { keys: ["Tab"], label: "换区" },
  { keys: ["Enter"], label: "进结果" },
  { keys: ["Mod", "Enter"], label: "入库" },
  { keys: ["?"], label: "键位表" },
];

export const BALL_KEY_HINTS: readonly KeyHint[] = [
  { keys: ["↑", "↓"], label: "选择" },
  { keys: ["Enter"], label: "进入" },
  { keys: ["Tab"], label: "换区" },
  { keys: ["Esc"], label: "收起" },
];

// #27 只有鼠标交互（键盘五区模型在 #38）：提示条只声明真正可用的键，
// 避免先展示 ↑↓/←→/Space 这些尚未接线的键位。
export const L2_KEY_HINTS: readonly KeyHint[] = [{ keys: ["Esc"], label: "收起" }];

export const L3_KEY_HINTS: readonly KeyHint[] = [{ keys: ["Esc"], label: "收起" }];

/**
 * 设置球入口行（#28）：本票只有收件箱可进入，键盘导航在 #38 接线，
 * 所以提示条只声明真正可用的 Esc。
 */
export const SETTINGS_ROW_KEY_HINTS: readonly KeyHint[] = [
  { keys: ["Esc"], label: "收起入口行" },
];

/** 设置球展开入口行（#24 §9 / #28）；未实现项 enabled=false，不可误触发。 */
export const SETTINGS_ROW_ITEMS: readonly SettingsRowItem[] = [
  { id: "inbox", label: "收件箱", enabled: true },
  { id: "all", label: "全部", enabled: false },
  { id: "tags", label: "标签管理", enabled: false },
  { id: "taxonomy", label: "分类管理", enabled: false },
  { id: "archive", label: "封存箱", enabled: false },
  { id: "trash", label: "回收站", enabled: false },
  { id: "settings", label: "设置", enabled: false },
  { id: "about", label: "关于", enabled: false },
];

/** L3 标签条的**结构顺序**（#17 §5.6）：媒体 chip → 形态标签 → 特征标签。 */
export const L3_MEDIA_CHIPS: readonly { readonly id: string; readonly name: string }[] = [
  { id: "link", name: "网页" },
  { id: "video", name: "视频" },
  { id: "article", name: "文章" },
];

/** 启动提示条的展示时长（#13：不抢焦点的一次性提示，5 秒后自动关闭）。 */
export const NOTICE_DURATION_MS = 5000;
