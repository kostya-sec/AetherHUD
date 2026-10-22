import { describe, expect, it, vi } from "vitest";
import {
  BALL_SLOTS,
  createDefaultHudState,
  createHudStore,
  hudReducer,
  resolveGlobalKeyDown,
  selectHudPhase,
  selectInputLocked,
  selectKeyHints,
  selectSearchExpanded,
  selectSearchFocusRequest,
  selectSearchOrbVisible,
  selectSearchQuery,
  selectVisibleBallSlots,
  type KeyDownLike,
} from "./index";

const plainKey = (key: string, overrides: Partial<KeyDownLike> = {}): KeyDownLike => ({
  key,
  ...overrides,
});

describe("HUD 默认态（S0）", () => {
  it("唤出后的默认态展开搜索框、隐藏搜索球，并请求一次搜索框聚焦", () => {
    const state = createDefaultHudState();

    expect(state.view).toBe("default");
    expect(state.phase).toBe("visible");
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

  it("唤出重置到默认态：清空查询、重新展开搜索框并聚焦（#16 K1）", () => {
    let state = createDefaultHudState();
    state = hudReducer(state, { type: "set-query", query: "Rust" });
    state = hudReducer(state, { type: "collapse-search" });
    state = hudReducer(state, { type: "expand-search" });
    expect(selectSearchFocusRequest(state)).toBe(1);

    const summoned = hudReducer(state, { type: "summon" });

    expect(selectSearchQuery(summoned)).toBe("");
    expect(selectSearchExpanded(summoned)).toBe(true);
    expect(selectSearchFocusRequest(summoned)).toBe(2);
    expect(selectHudPhase(summoned)).toBe("visible");
  });

  it("底部键位提示随焦点区变化：搜索框与球列", () => {
    const searchState = createDefaultHudState();
    const ballState = hudReducer(searchState, { type: "collapse-search" });

    expect(selectKeyHints(searchState).map((hint) => hint.label)).toContain("进结果");
    expect(selectKeyHints(ballState).map((hint) => hint.label)).toContain("选择");
    expect(selectKeyHints(ballState).flatMap((hint) => hint.keys)).toContain("Esc");
  });
});

describe("HUD 显隐阶段", () => {
  it("收起开始进入 dismissing 并锁定输入；再次唤出回到 visible", () => {
    const visible = createDefaultHudState();
    expect(selectInputLocked(visible)).toBe(false);

    const dismissing = hudReducer(visible, { type: "dismiss-start" });
    expect(selectHudPhase(dismissing)).toBe("dismissing");
    expect(selectInputLocked(dismissing)).toBe(true);

    const summoned = hudReducer(dismissing, { type: "summon" });
    expect(selectHudPhase(summoned)).toBe("visible");
    expect(selectInputLocked(summoned)).toBe(false);
  });

  it("收起期间忽略展开/折叠/输入命令，避免动画中改界面", () => {
    let state = hudReducer(createDefaultHudState(), { type: "collapse-search" });
    state = hudReducer(state, { type: "dismiss-start" });

    expect(hudReducer(state, { type: "expand-search" })).toBe(state);
    expect(hudReducer(state, { type: "collapse-search" })).toBe(state);
    expect(hudReducer(state, { type: "set-query", query: "rust" })).toBe(state);
    expect(hudReducer(state, { type: "dismiss-start" })).toBe(state);
  });

  it("收起不清空界面内容，下一次唤出才重置到 S0", () => {
    let state = hudReducer(createDefaultHudState(), { type: "set-query", query: "rust" });
    state = hudReducer(state, { type: "dismiss-start" });

    expect(selectSearchQuery(state)).toBe("rust");
    expect(selectSearchQuery(hudReducer(state, { type: "summon" }))).toBe("");
  });
});

describe("全局键位守卫（Esc 收起 / 收起期间吞键 / 输入法让行）", () => {
  it("可见时 Esc 收起，其余键不处理", () => {
    expect(resolveGlobalKeyDown(plainKey("Escape"), "visible")).toBe("dismiss");
    expect(resolveGlobalKeyDown(plainKey("a"), "visible")).toBe("ignore");
    expect(resolveGlobalKeyDown(plainKey("Enter"), "visible")).toBe("ignore");
  });

  it("带系统修饰键的 Esc 交给系统，不收起", () => {
    expect(resolveGlobalKeyDown(plainKey("Escape", { ctrlKey: true }), "visible")).toBe(
      "ignore",
    );
    expect(resolveGlobalKeyDown(plainKey("Escape", { altKey: true }), "visible")).toBe(
      "ignore",
    );
    expect(resolveGlobalKeyDown(plainKey("Escape", { metaKey: true }), "visible")).toBe(
      "ignore",
    );
    expect(resolveGlobalKeyDown(plainKey("Escape", { shiftKey: true }), "visible")).toBe(
      "dismiss",
    );
  });

  it("中文输入法组合态下所有键让给输入法（含 Esc）", () => {
    expect(resolveGlobalKeyDown(plainKey("Escape", { isComposing: true }), "visible")).toBe(
      "ignore",
    );
    expect(resolveGlobalKeyDown(plainKey("Escape", { keyCode: 229 }), "visible")).toBe(
      "ignore",
    );
    expect(resolveGlobalKeyDown(plainKey("Process"), "visible")).toBe("ignore");
  });

  it("收起动画期间吞掉所有键（含 Esc 与输入法事件）", () => {
    expect(resolveGlobalKeyDown(plainKey("Escape"), "dismissing")).toBe("swallow");
    expect(resolveGlobalKeyDown(plainKey("a"), "dismissing")).toBe("swallow");
    expect(resolveGlobalKeyDown(plainKey("Escape", { isComposing: true }), "dismissing")).toBe(
      "swallow",
    );
    expect(resolveGlobalKeyDown(plainKey("Process"), "dismissing")).toBe("swallow");
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
