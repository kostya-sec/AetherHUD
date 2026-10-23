import { beforeEach, describe, expect, it, vi } from "vitest";

// The platform adapter talks to Tauri IPC; tests replace it with a spy.
const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import { createDefaultHudState, createHudStore, selectNotice } from "../hud-model";
import { loadHudNavigation, showStartupNotice, takeHudNotices } from "./navigation";

const NAVIGATION = {
  baseVersion: "1.1.0",
  effectiveVersion: "1.1.0+u1",
  l1s: [],
  formatTags: [],
};

describe("导航数据 IPC 适配", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("读取核心库的生效分类树投影", async () => {
    invokeMock.mockResolvedValueOnce(NAVIGATION);
    await expect(loadHudNavigation()).resolves.toEqual(NAVIGATION);
    expect(invokeMock).toHaveBeenCalledWith("hud_navigation");
  });

  it("IPC 不可用（浏览器调试/核心库未启动）时返回 null，不抛错", async () => {
    invokeMock.mockRejectedValueOnce(new Error("no bridge"));
    await expect(loadHudNavigation()).resolves.toBeNull();
  });

  it("取走核心库提示，并在唤出后把第一条放进模型（带中文 detail）", async () => {
    invokeMock.mockResolvedValueOnce([
      {
        code: "taxonomy_overlay_corrupt",
        detail: "分类树覆盖层损坏，已备份为 taxonomy.overlay.corrupt-1.json",
      },
    ]);
    const notices = await takeHudNotices();
    expect(notices).toHaveLength(1);

    const store = createHudStore(createDefaultHudState());
    store.dispatch({ type: "summon" });
    invokeMock.mockResolvedValueOnce([
      ...notices,
      { code: "core_unavailable", detail: "第二条" },
    ]);
    await showStartupNotice(store);

    expect(selectNotice(store.getState())?.code).toBe("taxonomy_overlay_corrupt");
    expect(selectNotice(store.getState())?.detail).toContain("分类树覆盖层损坏");
  });

  it("没有提示时不写入状态", async () => {
    invokeMock.mockResolvedValueOnce([]);
    const store = createHudStore(createDefaultHudState());
    await showStartupNotice(store);
    expect(selectNotice(store.getState())).toBeNull();
  });
});
