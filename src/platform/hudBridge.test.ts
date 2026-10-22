import { beforeEach, describe, expect, it, vi } from "vitest";

// The platform adapter talks to Tauri IPC; tests replace it with a spy.
const invokeMock = vi.hoisted(() => vi.fn(async () => undefined));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import { applyKeyAction, dismissHud } from "./hudBridge";

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
