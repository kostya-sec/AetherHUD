import { describe, expect, it } from "vitest";
import {
  SETTINGS_ROW_ITEMS,
  createDefaultHudState,
  failureReasonLabel,
  hudReducer,
  inboxStatusBadge,
  isIngestShortcut,
  resolveEscapeCommand,
  selectInbox,
  selectKeyHints,
  selectQuickInput,
  selectSettingsRow,
  mediaTypeLabel,
  type HudState,
  type InboxEntry,
  type InboxPage,
  type KeyDownLike,
} from "./index";

const key = (value: string, overrides: Partial<KeyDownLike> = {}): KeyDownLike => ({
  key: value,
  ...overrides,
});

const entry = (id: number, status: InboxEntry["pipelineStatus"] = "pending"): InboxEntry => ({
  id,
  uid: `ulid-${id}`,
  title: `条目 ${id}`,
  url: null,
  mediaType: "article",
  pipelineStatus: status,
  failureReason: status === "failed" ? "fetch_failed" : null,
  createdAt: 1_000 + id,
  lastSeenAt: 1_000 + id,
  l1Id: "l1_inbox",
  l2Id: "l2_manual_review",
});

const page = (entries: InboxEntry[], nextCursor: InboxPage["nextCursor"] = null): InboxPage => ({
  entries,
  nextCursor,
});

const openedInbox = (): HudState =>
  hudReducer(createDefaultHudState(), { type: "open-inbox" });

describe("快速录入（Mod+Enter）", () => {
  it("Mod+Enter 触发入库；普通 Enter、Alt+Enter、输入法组合态不触发", () => {
    expect(isIngestShortcut(key("Enter"))).toBe(false);
    expect(isIngestShortcut(key("Enter", { ctrlKey: true }))).toBe(true);
    expect(isIngestShortcut(key("Enter", { metaKey: true }))).toBe(true);
    expect(isIngestShortcut(key("Enter", { ctrlKey: true, altKey: true }))).toBe(false);
    expect(isIngestShortcut(key("Enter", { ctrlKey: true, shiftKey: true }))).toBe(true);
    expect(isIngestShortcut(key("Enter", { ctrlKey: true, isComposing: true }))).toBe(false);
    expect(isIngestShortcut(key("Enter", { ctrlKey: true, keyCode: 229 }))).toBe(false);
    // 长按连发不重复打开/重复入库（#17 §7）。
    expect(isIngestShortcut(key("Enter", { ctrlKey: true, repeat: true }))).toBe(false);
    expect(isIngestShortcut(key("a", { ctrlKey: true }))).toBe(false);
  });

  it("打开弹窗带入当前输入，可关闭；提交中防重复", () => {
    let state = hudReducer(createDefaultHudState(), {
      type: "open-quick-input",
      seed: "https://example.com/a",
    });
    expect(selectQuickInput(state)).toEqual({
      seed: "https://example.com/a",
      submitting: false,
    });

    state = hudReducer(state, { type: "set-quick-input-submitting", submitting: true });
    expect(selectQuickInput(state)?.submitting).toBe(true);

    state = hudReducer(state, { type: "close-quick-input" });
    expect(selectQuickInput(state)).toBeNull();
    // 已关闭时再关闭/设置提交态是 no-op。
    expect(hudReducer(state, { type: "close-quick-input" })).toBe(state);
    expect(
      hudReducer(state, { type: "set-quick-input-submitting", submitting: true }),
    ).toBe(state);
  });

  it("唤出重置会关闭弹窗；收起动画期间忽略打开/关闭", () => {
    const open = hudReducer(createDefaultHudState(), {
      type: "open-quick-input",
      seed: "随手记",
    });
    expect(selectQuickInput(hudReducer(open, { type: "summon" }))).toBeNull();

    const dismissing = hudReducer(open, { type: "dismiss-start" });
    expect(
      hudReducer(dismissing, { type: "open-quick-input", seed: "x" }),
    ).toBe(dismissing);
    expect(hudReducer(dismissing, { type: "close-quick-input" })).toBe(dismissing);
  });
});

describe("设置球入口行", () => {
  it("入口顺序固定，收件箱与设置已实现（其余占位不可触发）", () => {
    expect(SETTINGS_ROW_ITEMS.map((item) => item.label)).toEqual([
      "收件箱",
      "全部",
      "标签管理",
      "分类管理",
      "封存箱",
      "回收站",
      "设置",
      "关于",
    ]);
    expect(SETTINGS_ROW_ITEMS.filter((item) => item.enabled).map((item) => item.id)).toEqual([
      "inbox",
      "settings",
    ]);
  });

  it("展开入口行会折叠 L2 面板与 L3 条；再点一次收起", () => {
    let state = hudReducer(createDefaultHudState(), { type: "toggle-l3-bar" });
    expect(selectSettingsRow(state).expanded).toBe(false);

    state = hudReducer(state, { type: "toggle-settings-row" });
    expect(selectSettingsRow(state).expanded).toBe(true);
    expect(state.l3Bar.expanded).toBe(false);
    expect(selectKeyHints(state).map((hint) => hint.label)).toContain("收起入口行");

    state = hudReducer(state, { type: "toggle-settings-row" });
    expect(selectSettingsRow(state).expanded).toBe(false);
  });

  it("展开入口行也会折叠 L2 面板", () => {
    let state = hudReducer(createDefaultHudState(), { type: "expand-search" });
    state = hudReducer(state, { type: "set-navigation", navigation: NAVIGATION });
    state = hudReducer(state, { type: "activate-l1", l1Id: "l1_tech" });
    expect(state.l2Panel).not.toBeNull();

    state = hudReducer(state, { type: "toggle-settings-row" });
    expect(state.l2Panel).toBeNull();
    expect(selectSettingsRow(state).expanded).toBe(true);
  });
});

describe("收件箱列表视图", () => {
  it("打开时进入 loading 并收起入口行，第一页替换数据", () => {
    let state = hudReducer(createDefaultHudState(), { type: "toggle-settings-row" });
    state = hudReducer(state, { type: "open-inbox" });
    expect(selectInbox(state).open).toBe(true);
    expect(selectInbox(state).loading).toBe(true);
    expect(selectInbox(state).entries).toEqual([]);
    expect(selectSettingsRow(state).expanded).toBe(false);

    state = hudReducer(state, {
      type: "replace-inbox",
      page: page([entry(2), entry(1)], { createdAt: 1_001, id: 1 }),
    });
    expect(selectInbox(state).entries.map((item) => item.id)).toEqual([2, 1]);
    expect(selectInbox(state).nextCursor).toEqual({ createdAt: 1_001, id: 1 });
    expect(selectInbox(state).loading).toBe(false);
  });

  it("加载更多按 id 去重；关闭后清空", () => {
    let state = hudReducer(openedInbox(), {
      type: "replace-inbox",
      page: page([entry(3), entry(2)]),
    });
    state = hudReducer(state, {
      type: "append-inbox",
      page: page([entry(2), entry(1)], { createdAt: 1_001, id: 1 }),
    });
    expect(selectInbox(state).entries.map((item) => item.id)).toEqual([3, 2, 1]);
    expect(selectInbox(state).nextCursor).toEqual({ createdAt: 1_001, id: 1 });

    state = hudReducer(state, { type: "close-inbox" });
    expect(selectInbox(state).open).toBe(false);
    expect(selectInbox(state).entries).toEqual([]);
  });

  it("inbox-updated 递增 revision 触发重载；关闭状态下忽略列表数据", () => {
    const closed = createDefaultHudState();
    const invalidated = hudReducer(closed, { type: "invalidate-inbox" });
    expect(selectInbox(invalidated).revision).toBe(1);
    expect(hudReducer(closed, { type: "replace-inbox", page: page([entry(1)]) })).toBe(
      closed,
    );
    expect(hudReducer(closed, { type: "set-inbox-loading", loading: true })).toBe(closed);
  });

  it("唤出回到 S0：收件箱视图与 revision 一起重置", () => {
    let state = openedInbox();
    state = hudReducer(state, { type: "invalidate-inbox" });
    const summoned = hudReducer(state, { type: "summon" });
    expect(selectInbox(summoned).open).toBe(false);
    expect(selectInbox(summoned).entries).toEqual([]);
    expect(selectInbox(summoned).revision).toBe(0);
  });

  it("pending/processing 显示「分类中」，failed 显示「失败」", () => {
    expect(inboxStatusBadge("pending")).toEqual({ label: "分类中", tone: "pending" });
    expect(inboxStatusBadge("processing")).toEqual({ label: "分类中", tone: "pending" });
    expect(inboxStatusBadge("failed")).toEqual({ label: "失败", tone: "failed" });
    expect(inboxStatusBadge("classified")).toBeNull();
  });
});

describe("条目展示文案", () => {
  it("媒体类型与失败原因有单一中文映射", () => {
    expect(mediaTypeLabel("link")).toBe("网页");
    expect(mediaTypeLabel("video")).toBe("视频");
    expect(mediaTypeLabel("article")).toBe("文章 / 纯文本");
    expect(mediaTypeLabel("unknown")).toBe("unknown");
    expect(failureReasonLabel("fetch_failed")).toBe("失败：抓取失败");
    expect(failureReasonLabel("classify_failed")).toBe("失败：分类失败");
    expect(failureReasonLabel("other")).toBe("失败：other");
  });
});

describe("Esc 退一层", () => {
  it("弹窗 → 收件箱 → 入口行 → 收起 HUD", () => {
    const base = createDefaultHudState();
    expect(resolveEscapeCommand(base)).toBeNull();

    // 入口行：Esc 只收入口行，不收起 HUD。
    const row = hudReducer(base, { type: "toggle-settings-row" });
    const rowCommand = resolveEscapeCommand(row);
    expect(rowCommand).toEqual({ type: "collapse-settings-row" });
    expect(resolveEscapeCommand(hudReducer(row, rowCommand!))).toBeNull();

    // 收件箱视图：Esc 返回 HUD。
    const inbox = hudReducer(base, { type: "open-inbox" });
    const inboxCommand = resolveEscapeCommand(inbox);
    expect(inboxCommand).toEqual({ type: "close-inbox" });
    expect(resolveEscapeCommand(hudReducer(inbox, inboxCommand!))).toBeNull();

    // 快速录入弹窗优先级最高（即使底层还有视图）。
    const modal = hudReducer(inbox, { type: "open-quick-input", seed: "x" });
    const modalCommand = resolveEscapeCommand(modal);
    expect(modalCommand).toEqual({ type: "close-quick-input" });
    // 关掉弹窗后仍停留在收件箱视图，Esc 再退一层。
    expect(resolveEscapeCommand(hudReducer(modal, modalCommand!))).toEqual({
      type: "close-inbox",
    });
  });
});

const NAVIGATION = {
  baseVersion: "1.1.0",
  effectiveVersion: "1.1.0+u0",
  l1s: [
    {
      id: "l1_tech",
      name: "03 技术与工程",
      icon: null,
      l2s: [{ id: "l2_coding", name: "编程与低代码", selfBuilt: false }],
    },
  ],
  formatTags: [],
};