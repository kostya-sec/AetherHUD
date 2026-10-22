import type { BallSlot, KeyHint } from "./types";

/**
 * Ball column order from the accepted #7 prototype:
 * search orb → L3 filter → five L1 slots → quick input → settings.
 * The search orb is rendered only while the search bar is collapsed.
 */
export const BALL_SLOTS: readonly BallSlot[] = [
  { id: "search", kind: "search", label: "全局搜索", placeholder: false },
  { id: "l3-filter", kind: "l3-filter", label: "L3 标签筛选", placeholder: true },
  { id: "l1-1", kind: "l1", label: "分类槽位 01", placeholder: true },
  { id: "l1-2", kind: "l1", label: "分类槽位 02", placeholder: true },
  { id: "l1-3", kind: "l1", label: "分类槽位 03", placeholder: true },
  { id: "l1-4", kind: "l1", label: "分类槽位 04", placeholder: true },
  { id: "l1-5", kind: "l1", label: "分类槽位 05", placeholder: true },
  { id: "quick-add", kind: "quick-add", label: "快速录入", placeholder: true },
  { id: "settings", kind: "settings", label: "全局设置", placeholder: true },
];

/**
 * Bottom key hints for the two zones that exist in this scaffold. T14 replaces
 * this with the full five-zone + search-bubble model.
 */
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
