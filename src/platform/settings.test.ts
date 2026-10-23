import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.hoisted(() => vi.fn());
const listenMock = vi.hoisted(() => {
  const handlers = new Map<string, () => void>();
  return {
    handlers,
    listen: vi.fn(async (event: string, handler: () => void) => {
      handlers.set(event, handler);
      return () => handlers.delete(event);
    }),
  };
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: listenMock.listen }));

import {
  createHudStore,
  selectCredentialValidation,
  selectSettings,
  type CredentialStatus,
} from "../hud-model";
import {
  ABOUT_OPEN_EVENT,
  SETTINGS_OPEN_EVENT,
  clearCredential,
  connectSettingsEvents,
  loadCredentialStatuses,
  quitApp,
  saveCredential,
  validateCredential,
} from "./settings";

const configured: CredentialStatus = {
  kind: "classifierApiKey",
  account: "classifier.api_key",
  label: "百炼 API Key",
  state: "configured",
  reference: "keychain://aetherhud/classifier.api_key",
};

function openedStore() {
  const store = createHudStore();
  store.dispatch({ type: "open-settings" });
  return store;
}

describe("设置 IPC 适配（#29）", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    listenMock.handlers.clear();
  });

  it("打开设置后读取凭证状态并结束 loading", async () => {
    invokeMock.mockResolvedValueOnce([configured]);
    const store = openedStore();

    await loadCredentialStatuses(store.dispatch);

    expect(invokeMock).toHaveBeenCalledWith("settings_credential_statuses");
    expect(selectSettings(store.getState()).credentials.statuses).toEqual([configured]);
    expect(selectSettings(store.getState()).credentials.loading).toBe(false);
  });

  it("读取失败降级：给出可读提示而不是崩溃", async () => {
    invokeMock.mockRejectedValueOnce("核心库未启动");
    const store = openedStore();

    await loadCredentialStatuses(store.dispatch);

    expect(selectSettings(store.getState()).credentials.loading).toBe(false);
    expect(selectSettings(store.getState()).credentials.message).toEqual({
      tone: "error",
      text: "读取凭证状态失败：核心库未启动",
    });
  });

  it("保存会去掉首尾空白，成功后更新状态与提示并返回 true", async () => {
    invokeMock.mockResolvedValueOnce(configured);
    const store = openedStore();

    const saved = await saveCredential(store.dispatch, "classifierApiKey", "  sk-secret  ");

    expect(saved).toBe(true);
    expect(invokeMock).toHaveBeenCalledWith("settings_save_credential", {
      kind: "classifierApiKey",
      secret: "sk-secret",
    });
    expect(selectSettings(store.getState()).credentials.statuses).toEqual([configured]);
    expect(selectSettings(store.getState()).credentials.busy).toBeNull();
    expect(selectSettings(store.getState()).credentials.message?.tone).toBe("ok");
  });

  it("空白密钥不发 IPC；保存失败保留输入并给出可读原因", async () => {
    const store = openedStore();

    expect(await saveCredential(store.dispatch, "jinaApiKey", "   ")).toBe(false);
    expect(invokeMock).not.toHaveBeenCalled();

    invokeMock.mockRejectedValueOnce("无法访问系统钥匙串（jina.api_key）：拒绝访问");
    const saved = await saveCredential(store.dispatch, "jinaApiKey", "jina-key");

    expect(saved).toBe(false);
    expect(selectSettings(store.getState()).credentials.message?.tone).toBe("error");
    expect(selectSettings(store.getState()).credentials.message?.text).toContain("钥匙串");
    expect(selectSettings(store.getState()).credentials.busy).toBeNull();
  });

  it("清除后状态回到未配置", async () => {
    invokeMock.mockResolvedValueOnce({ ...configured, state: "not_configured" });
    const store = openedStore();

    const cleared = await clearCredential(store.dispatch, "classifierApiKey");

    expect(cleared).toBe(true);
    expect(invokeMock).toHaveBeenCalledWith("settings_clear_credential", {
      kind: "classifierApiKey",
    });
    expect(selectSettings(store.getState()).credentials.statuses[0]?.state).toBe(
      "not_configured",
    );
  });

  it("验证成功与失败都落到对应 target，忙碌键最后清空", async () => {
    invokeMock.mockResolvedValueOnce({
      ok: true,
      detail: "百炼连接正常（178 个模型）",
    });
    const store = openedStore();

    await validateCredential(store.dispatch, "classifier");

    expect(invokeMock).toHaveBeenCalledWith("settings_validate_credential", {
      target: "classifier",
    });
    expect(
      selectCredentialValidation(selectSettings(store.getState()).credentials, "classifier"),
    ).toEqual({ target: "classifier", ok: true, detail: "百炼连接正常（178 个模型）" });
    expect(selectSettings(store.getState()).credentials.busy).toBeNull();

    invokeMock.mockRejectedValueOnce("IPC 超时");
    await validateCredential(store.dispatch, "feishu");

    expect(
      selectCredentialValidation(selectSettings(store.getState()).credentials, "feishu")?.ok,
    ).toBe(false);
    expect(
      selectCredentialValidation(selectSettings(store.getState()).credentials, "feishu")?.detail,
    ).toContain("IPC 超时");
  });

  it("退出入口走 app.exit（托盘之外唯一退出路径）", async () => {
    invokeMock.mockResolvedValueOnce(undefined);

    quitApp();

    expect(invokeMock).toHaveBeenCalledWith("quit_app");
  });

  it("托盘「设置…」与「关于」事件打开设置弹层对应分区", async () => {
    const store = createHudStore();

    await connectSettingsEvents(store);

    listenMock.handlers.get(SETTINGS_OPEN_EVENT)?.();
    expect(selectSettings(store.getState()).open).toBe(true);
    expect(selectSettings(store.getState()).section).toBe("general");

    store.dispatch({ type: "close-settings" });
    listenMock.handlers.get(ABOUT_OPEN_EVENT)?.();
    expect(selectSettings(store.getState()).open).toBe(true);
    expect(selectSettings(store.getState()).section).toBe("about");
  });
});