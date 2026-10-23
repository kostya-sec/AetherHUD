import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import { createHudStore, selectNotice, selectQuickInput } from "../hud-model";
import { previewTitle, submitQuickInput } from "./ingest";

const OUTCOME = {
  entryId: 7,
  duplicate: false,
  stage: "fetch",
  pipelineStatus: "pending",
  title: "https://example.com/a",
  createdAt: 1_000,
  lastSeenAt: 1_000,
};

function storeWithQuickInput(seed = "https://example.com/a") {
  const store = createHudStore();
  store.dispatch({ type: "open-quick-input", seed });
  return store;
}

describe("快速录入 IPC 适配", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("入库成功：关闭弹窗并提示已入库", async () => {
    invokeMock.mockResolvedValueOnce(OUTCOME);
    const store = storeWithQuickInput();

    await submitQuickInput(store.dispatch, "  https://example.com/a  ");

    expect(invokeMock).toHaveBeenCalledWith("ingest_text", {
      text: "https://example.com/a",
    });
    expect(selectQuickInput(store.getState())).toBeNull();
    expect(selectNotice(store.getState())?.code).toBe("entry_ingested");
    expect(selectNotice(store.getState())?.detail).toContain("正在分类");
  });

  it("重复投递：提示更新最近出现时间", async () => {
    invokeMock.mockResolvedValueOnce({ ...OUTCOME, duplicate: true, stage: null });
    const store = storeWithQuickInput();

    await submitQuickInput(store.dispatch, "https://example.com/a");

    expect(selectNotice(store.getState())?.code).toBe("entry_duplicate");
    expect(selectNotice(store.getState())?.detail).toContain("重复投递");
  });

  it("入库失败：保留弹窗、退出提交态并给出可读错误", async () => {
    invokeMock.mockRejectedValueOnce("数据库被锁");
    const store = storeWithQuickInput("随手记");

    await submitQuickInput(store.dispatch, "随手记");

    expect(selectQuickInput(store.getState())).toEqual({
      seed: "随手记",
      submitting: false,
    });
    expect(selectNotice(store.getState())?.code).toBe("ingest_failed");
    expect(selectNotice(store.getState())?.detail).toContain("数据库被锁");
  });

  it("空白内容不发 IPC", async () => {
    const store = storeWithQuickInput();
    await submitQuickInput(store.dispatch, "   ");
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("标题预览按 Unicode 字符截断", () => {
    expect(previewTitle("短标题")).toBe("短标题");
    expect(previewTitle("中".repeat(60))).toHaveLength(49);
    expect(previewTitle("中".repeat(60)).endsWith("…")).toBe(true);
  });
});
