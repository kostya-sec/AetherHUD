import { describe, expect, it, vi } from "vitest";
import {
  BALL_SLOTS,
  createDefaultHudState,
  createHudStore,
  hudReducer,
  selectKeyHints,
  selectSearchExpanded,
  selectSearchFocusRequest,
  selectSearchOrbVisible,
  selectSearchQuery,
  selectVisibleBallSlots,
} from "./index";

describe("HUD 默认态（S0）", () => {
  it("唤出后的默认态展开搜索框、隐藏搜索球且不请求聚焦", () => {
    const state = createDefaultHudState();

    expect(state.view).toBe("default");
    expect(selectSearchExpanded(state)).toBe(true);
    expect(selectSearchOrbVisible(state)).toBe(false);
    expect(selectSearchQuery(state)).toBe("");
    expect(selectSearchFocusRequest(state)).toBe(0);
    expect(selectVisibleBallSlots(state).map((slot) => slot.kind)).toEqual([
      "l3-filter",
      "l1",
      "l1",
      "l1",
      "l1",
      "l1",
      "quick-add",
      "settings",
    ]);
  });

  it("鼠标折叠搜索框后，球列顶部出现搜索球", () => {
    const collapsed = hudReducer(createDefaultHudState(), { type: "collapse-search" });
    const visible = selectVisibleBallSlots(collapsed);

    expect(selectSearchExpanded(collapsed)).toBe(false);
    expect(selectSearchOrbVisible(collapsed)).toBe(true);
    expect(visible[0]?.kind).toBe("search");
    expect(visible).toHaveLength(BALL_SLOTS.length);
  });

  it("重新展开搜索框会请求一次搜索框聚焦", () => {
    const collapsed = hudReducer(createDefaultHudState(), { type: "collapse-search" });
    const expanded = hudReducer(collapsed, { type: "expand-search" });

    expect(selectSearchExpanded(expanded)).toBe(true);
    expect(selectSearchFocusRequest(collapsed)).toBe(0);
    expect(selectSearchFocusRequest(expanded)).toBe(
      selectSearchFocusRequest(collapsed) + 1,
    );
  });

  it("唤出重置到默认态：清空查询、重新展开搜索框，但不请求聚焦", () => {
    let state = createDefaultHudState();
    state = hudReducer(state, { type: "set-query", query: "Rust" });
    state = hudReducer(state, { type: "collapse-search" });
    state = hudReducer(state, { type: "expand-search" });
    expect(selectSearchFocusRequest(state)).toBe(1);

    const summoned = hudReducer(state, { type: "summon" });

    expect(selectSearchQuery(summoned)).toBe("");
    expect(selectSearchExpanded(summoned)).toBe(true);
    expect(selectSearchFocusRequest(summoned)).toBe(0);
  });

  it("底部键位提示随焦点区变化：搜索框与球列", () => {
    const searchState = createDefaultHudState();
    const ballState = hudReducer(searchState, { type: "collapse-search" });

    expect(selectKeyHints(searchState).map((hint) => hint.label)).toContain("进结果");
    expect(selectKeyHints(ballState).map((hint) => hint.label)).toContain("选择");
    expect(selectKeyHints(ballState).flatMap((hint) => hint.keys)).toContain("Esc");
  });
});

describe("HUD store", () => {
  it("dispatch 后通知订阅者并且选择器读到新状态", () => {
    const store = createHudStore();
    const listener = vi.fn();
    const unsubscribe = store.subscribe(listener);

    store.dispatch({ type: "set-query", query: "rust" });

    expect(listener).toHaveBeenCalledTimes(1);
    expect(selectSearchQuery(store.getState())).toBe("rust");

    unsubscribe();
    store.dispatch({ type: "set-query", query: "tauri" });
    expect(listener).toHaveBeenCalledTimes(1);
  });
});
