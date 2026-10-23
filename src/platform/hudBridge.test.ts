import { beforeEach, describe, expect, it, vi } from "vitest";

// The platform adapter talks to Tauri IPC; tests replace it with a spy.
const invokeMock = vi.hoisted(() => vi.fn(async () => undefined));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import {
  createDefaultHudState,
  createHudStore,
  hudReducer,
  selectInbox,
  selectQuickInput,
  selectSettings,
} from "../hud-model";
import { applyGlobalKey, applyKeyAction, dismissHud } from "./hudBridge";

const immediately = (task: () => void): void => task();

function fakeEvent() {
  return { key: "Escape", preventDefault: vi.fn(), stopPropagation: vi.fn() };
}

describe("HUD 全局键位动作", () => {
  beforeEach(() => {
    invokeMock.mockClear();
  });

  it("ignore 不动事件也不找 OS", () => {
    const event = fakeEvent();
    applyKeyAction("ignore", event, immediately);

    expect(event.preventDefault).not.toHaveBeenCalled();
    expect(event.stopPropagation).not.toHaveBeenCalled();
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("swallow 只吞事件，不触发收起", () => {
    const event = fakeEvent();
    applyKeyAction("swallow", event, immediately);

    expect(event.preventDefault).toHaveBeenCalledTimes(1);
    expect(event.stopPropagation).toHaveBeenCalledTimes(1);
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("dismiss 吞掉事件并请求外壳收起", () => {
    const event = fakeEvent();
    applyKeyAction("dismiss", event, immediately);

    expect(event.preventDefault).toHaveBeenCalledTimes(1);
    expect(event.stopPropagation).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith("request_dismiss");
  });

  it("dismiss 收起请求延后一拍（#17 §7 不能与同帧输入法事件竞争）", () => {
    const event = fakeEvent();
    const scheduled: Array<() => void> = [];
    applyKeyAction("dismiss", event, (task) => scheduled.push(task));

    expect(event.preventDefault).toHaveBeenCalledTimes(1);
    expect(invokeMock).not.toHaveBeenCalled();
    expect(scheduled).toHaveLength(1);

    scheduled[0]();
    expect(invokeMock).toHaveBeenCalledWith("request_dismiss");
  });

  it("Esc 收起走同一条 IPC（供浏览器内联调试复用）", () => {
    dismissHud();
    expect(invokeMock).toHaveBeenCalledWith("request_dismiss");
  });
});

describe("Esc 先关最上层，再收起 HUD", () => {
  const immediately = (task: () => void): void => task();
  const noComposition = { now: 0, compositionEndedAt: null };

  beforeEach(() => {
    invokeMock.mockClear();
  });

  it("快速录入弹窗打开时 Esc 只关弹窗，不收起 HUD", () => {
    const store = createHudStore(
      hudReducer(createDefaultHudState(), { type: "open-quick-input", seed: "rust" }),
    );
    const event = fakeEvent();

    applyGlobalKey(event, store, noComposition, immediately);

    expect(selectQuickInput(store.getState())).toBeNull();
    expect(event.preventDefault).toHaveBeenCalledTimes(1);
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("收件箱视图打开时 Esc 返回 HUD", () => {
    const store = createHudStore(
      hudReducer(createDefaultHudState(), { type: "open-inbox" }),
    );

    applyGlobalKey(fakeEvent(), store, noComposition, immediately);

    expect(selectInbox(store.getState()).open).toBe(false);
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("设置弹层打开时 Esc 只关弹层，回到 HUD 原处", () => {
    const store = createHudStore(
      hudReducer(createDefaultHudState(), { type: "open-settings" }),
    );

    applyGlobalKey(fakeEvent(), store, noComposition, immediately);

    expect(selectSettings(store.getState()).open).toBe(false);
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("没有浮层时 Esc 仍请求外壳收起", () => {
    const store = createHudStore(createDefaultHudState());

    applyGlobalKey(fakeEvent(), store, noComposition, immediately);

    expect(invokeMock).toHaveBeenCalledWith("request_dismiss");
  });

  it("输入法组合态 Esc 让给输入法，不关弹窗也不收起", () => {
    const store = createHudStore(
      hudReducer(createDefaultHudState(), { type: "open-quick-input", seed: "rust" }),
    );
    const event = { ...fakeEvent(), isComposing: true };

    applyGlobalKey(event, store, { now: 10, compositionEndedAt: null }, immediately);

    expect(selectQuickInput(store.getState())?.seed).toBe("rust");
    expect(event.preventDefault).not.toHaveBeenCalled();
    expect(invokeMock).not.toHaveBeenCalled();
  });
});