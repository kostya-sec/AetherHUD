import { describe, expect, it, vi } from "vitest";
import {
  BALL_SLOTS,
  createDefaultHudState,
  createHudStore,
  hudReducer,
  resolveGlobalKeyDown,
  COMPOSITION_GRACE_MS,
  selectHudPhase,
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
  it("默认态为球列 + 搜索框收起，且不请求聚焦", () => {
    const state = createDefaultHudState();

    expect(state.view).toBe("default");
    expect(state.phase).toBe("visible");
    expect(selectSearchExpanded(state)).toBe(false);
    expect(selectSearchOrbVisible(state)).toBe(true);
    expect(selectSearchQuery(state)).toBe("");
    expect(selectSearchFocusRequest(state)).toBe(0);
    expect(selectVisibleBallSlots(state).map((slot) => slot.kind)).toEqual([
      "search",
      "l3-filter",
      "l1",
      "l1",
      "l1",
      "l1",
      "l1",
      "quick-add",
      "settings",
    ]);
    expect(selectVisibleBallSlots(state)).toHaveLength(BALL_SLOTS.length);
  });

  it("唤出重置到默认态：清空查询、收起搜索框，且不请求聚焦", () => {
    let state = createDefaultHudState();
    state = hudReducer(state, { type: "expand-search" });
    state = hudReducer(state, { type: "set-query", query: "Rust" });
    expect(selectSearchExpanded(state)).toBe(true);
    expect(selectSearchFocusRequest(state)).toBe(1);

    const summoned = hudReducer(state, { type: "summon" });

    expect(selectSearchQuery(summoned)).toBe("");
    expect(selectSearchExpanded(summoned)).toBe(false);
    expect(selectSearchFocusRequest(summoned)).toBe(0);
    expect(selectSearchOrbVisible(summoned)).toBe(true);
    expect(selectHudPhase(summoned)).toBe("visible");
  });

  it("点击搜索球展开并聚焦；折叠后再展开会再次请求聚焦", () => {
    const expanded = hudReducer(createDefaultHudState(), { type: "expand-search" });
    expect(selectSearchExpanded(expanded)).toBe(true);
    expect(selectSearchOrbVisible(expanded)).toBe(false);
    expect(selectSearchFocusRequest(expanded)).toBe(1);
    expect(selectVisibleBallSlots(expanded)[0]?.kind).not.toBe("search");

    const collapsed = hudReducer(expanded, { type: "collapse-search" });
    expect(selectSearchOrbVisible(collapsed)).toBe(true);

    const reExpanded = hudReducer(collapsed, { type: "expand-search" });
    expect(selectSearchExpanded(reExpanded)).toBe(true);
    expect(selectSearchFocusRequest(reExpanded)).toBe(2);
  });

  it("底部键位提示随焦点区变化：默认球列、展开后搜索框", () => {
    const ballState = createDefaultHudState();
    expect(selectKeyHints(ballState).map((hint) => hint.label)).toContain("选择");
    expect(selectKeyHints(ballState).flatMap((hint) => hint.keys)).toContain("Esc");

    const searchState = hudReducer(ballState, { type: "expand-search" });
    expect(selectKeyHints(searchState).map((hint) => hint.label)).toContain("进结果");
  });
});

describe("HUD 显隐阶段", () => {
  it("收起开始进入 dismissing；再次唤出回到 visible", () => {
    const visible = createDefaultHudState();
    expect(selectHudPhase(visible)).toBe("visible");

    const dismissing = hudReducer(visible, { type: "dismiss-start" });
    expect(selectHudPhase(dismissing)).toBe("dismissing");

    const summoned = hudReducer(dismissing, { type: "summon" });
    expect(selectHudPhase(summoned)).toBe("visible");
  });

  it("收起期间忽略展开/折叠/输入命令，避免动画中改界面", () => {
    const state = hudReducer(createDefaultHudState(), { type: "dismiss-start" });

    expect(hudReducer(state, { type: "expand-search" })).toBe(state);
    expect(hudReducer(state, { type: "collapse-search" })).toBe(state);
    expect(hudReducer(state, { type: "set-query", query: "rust" })).toBe(state);
    expect(hudReducer(state, { type: "dismiss-start" })).toBe(state);
  });

  it("收起不清空界面内容，下一次唤出才重置到 S0", () => {
    let state = hudReducer(createDefaultHudState(), { type: "expand-search" });
    state = hudReducer(state, { type: "set-query", query: "rust" });
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

  it("组合结束后 100ms 内的 Esc 不收起（确认键可能晚于 compositionend）", () => {
    const composedAt = 1000;
    expect(
      resolveGlobalKeyDown(plainKey("Escape"), "visible", {
        now: composedAt + COMPOSITION_GRACE_MS - 1,
        compositionEndedAt: composedAt,
      }),
    ).toBe("ignore");
    expect(
      resolveGlobalKeyDown(plainKey("Escape"), "visible", {
        now: composedAt + COMPOSITION_GRACE_MS,
        compositionEndedAt: composedAt,
      }),
    ).toBe("dismiss");
    // 从未组合过时不受宽限窗影响。
    expect(
      resolveGlobalKeyDown(plainKey("Escape"), "visible", {
        now: 10,
        compositionEndedAt: null,
      }),
    ).toBe("dismiss");
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
