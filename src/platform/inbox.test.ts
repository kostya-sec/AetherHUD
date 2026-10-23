import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import {
  createDefaultHudState,
  createHudStore,
  hudReducer,
  selectInbox,
  selectNotice,
  type InboxEntry,
} from "../hud-model";
import { loadInboxFirstPage, loadInboxNextPage } from "./inbox";

const entry = (id: number): InboxEntry => ({
  id,
  uid: `u${id}`,
  title: `条目 ${id}`,
  url: null,
  mediaType: "article",
  pipelineStatus: "pending",
  failureReason: null,
  createdAt: 1_000 + id,
  lastSeenAt: 1_000 + id,
  l1Id: "l1_inbox",
  l2Id: "l2_manual_review",
});

function openedStore() {
  const store = createHudStore();
  store.dispatch({ type: "open-inbox" });
  return store;
}

describe("收件箱 IPC 适配", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("打开后取第一页并替换列表", async () => {
    invokeMock.mockResolvedValueOnce({
      entries: [entry(2), entry(1)],
      nextCursor: { createdAt: 1_001, id: 1 },
    });
    const store = openedStore();

    await loadInboxFirstPage(store.dispatch);

    expect(invokeMock).toHaveBeenCalledWith("hud_inbox", { cursor: null });
    expect(selectInbox(store.getState()).entries.map((item) => item.id)).toEqual([2, 1]);
    expect(selectInbox(store.getState()).loading).toBe(false);
  });

  it("加载更多用上一页游标追加；没有游标时不发 IPC", async () => {
    invokeMock.mockResolvedValueOnce({
      entries: [entry(1)],
      nextCursor: null,
    });
    const store = openedStore();

    await loadInboxNextPage(store.dispatch, { createdAt: 1_002, id: 2 });

    expect(invokeMock).toHaveBeenCalledWith("hud_inbox", {
      cursor: { createdAt: 1_002, id: 2 },
    });
    expect(selectInbox(store.getState()).entries.map((item) => item.id)).toEqual([1]);

    invokeMock.mockClear();
    await loadInboxNextPage(store.dispatch, null);
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("读取失败：退出 loading 并给出提示条", async () => {
    invokeMock.mockRejectedValueOnce("核心库未启动");
    const store = openedStore();

    await loadInboxFirstPage(store.dispatch);

    expect(selectInbox(store.getState()).loading).toBe(false);
    expect(selectNotice(store.getState())?.code).toBe("inbox_unavailable");
    expect(selectNotice(store.getState())?.detail).toContain("核心库未启动");
  });

  it("收件箱关闭时数据命令被 reducer 忽略（不会闪回旧列表）", () => {
    const closed = createDefaultHudState();
    const next = hudReducer(closed, {
      type: "replace-inbox",
      page: { entries: [entry(1)], nextCursor: null },
    });
    expect(next).toBe(closed);
    expect(selectInbox(closed).entries).toEqual([]);
  });
});
