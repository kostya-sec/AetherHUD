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
  type ProviderProfile,
} from "../hud-model";
import {
  ABOUT_OPEN_EVENT,
  SETTINGS_OPEN_EVENT,
  clearCredential,
  connectSettingsEvents,
  loadCredentialStatuses,
  loadProviderProfile,
  normalizeProviderProfile,
  quitApp,
  saveCredential,
  saveProviderProfile,
  validateCredential,
  validateProviderProfile,
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

const providerProfile = (
  overrides: Partial<ProviderProfile> = {},
): ProviderProfile => ({
  preset: "custom",
  baseUrl: "https://llm.example.com/v1",
  model: "my-model",
  messageType: "responses",
  ...overrides,
});

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

  it("打开设置后读取活动供应商配置", async () => {
    invokeMock.mockResolvedValueOnce(providerProfile());
    const store = openedStore();

    await loadProviderProfile(store.dispatch);

    expect(invokeMock).toHaveBeenCalledWith("settings_provider_profile");
    expect(selectSettings(store.getState()).credentials.profile).toEqual(
      providerProfile(),
    );
    expect(selectSettings(store.getState()).credentials.profileMessage).toBeNull();
  });

  it("供应商配置读取失败给出可读提示，不阻塞凭证区", async () => {
    invokeMock.mockRejectedValueOnce("核心库未启动");
    const store = openedStore();

    await loadProviderProfile(store.dispatch);

    expect(selectSettings(store.getState()).credentials.profile).toBeNull();
    expect(selectSettings(store.getState()).credentials.profileMessage).toEqual({
      tone: "error",
      text: "读取供应商配置失败：核心库未启动",
    });
  });

  it("保存供应商配置：去空白与尾部斜杠、回读配置、提示成功", async () => {
    const saved = providerProfile({
      baseUrl: "https://llm.example.com/v1",
      model: "my-model",
    });
    invokeMock.mockResolvedValueOnce(saved);
    const store = openedStore();

    const ok = await saveProviderProfile(
      store.dispatch,
      providerProfile({
        baseUrl: "  https://llm.example.com/v1/  ",
        model: "  my-model  ",
      }),
    );

    expect(ok).toBe(true);
    expect(invokeMock).toHaveBeenCalledWith("settings_save_provider_profile", {
      profile: {
        preset: "custom",
        baseUrl: "https://llm.example.com/v1",
        model: "my-model",
        messageType: "responses",
      },
    });
    expect(selectSettings(store.getState()).credentials.profile).toEqual(saved);
    expect(selectSettings(store.getState()).credentials.profileBusy).toBe(false);
    expect(selectSettings(store.getState()).credentials.profileMessage?.tone).toBe("ok");
    expect(
      selectSettings(store.getState()).credentials.profileMessage?.text,
    ).toContain("my-model");
  });

  it("非法 Base URL 或空模型不发 IPC，只给可读错误", async () => {
    const store = openedStore();

    expect(
      await saveProviderProfile(
        store.dispatch,
        providerProfile({ baseUrl: "api.example.com/v1" }),
      ),
    ).toBe(false);
    expect(invokeMock).not.toHaveBeenCalled();
    expect(
      selectSettings(store.getState()).credentials.profileMessage?.text,
    ).toContain("http:// 或 https://");

    expect(
      await saveProviderProfile(store.dispatch, providerProfile({ model: "   " })),
    ).toBe(false);
    expect(invokeMock).not.toHaveBeenCalled();
    expect(
      selectSettings(store.getState()).credentials.profileMessage?.text,
    ).toContain("模型 ID 不能为空");

    expect(
      await saveProviderProfile(
        store.dispatch,
        providerProfile({ baseUrl: "   " }),
      ),
    ).toBe(false);
    expect(
      selectSettings(store.getState()).credentials.profileMessage?.text,
    ).toContain("Base URL 不能为空");
  });

  it("后端拒绝保存时给出可读原因并清空忙碌", async () => {
    invokeMock.mockRejectedValueOnce("Base URL 必须以 http:// 或 https:// 开头：x");
    const store = openedStore();

    const ok = await saveProviderProfile(store.dispatch, providerProfile());

    expect(ok).toBe(false);
    expect(selectSettings(store.getState()).credentials.profileBusy).toBe(false);
    expect(selectSettings(store.getState()).credentials.profileMessage).toEqual({
      tone: "error",
      text: "保存供应商配置失败：Base URL 必须以 http:// 或 https:// 开头：x",
    });
  });

  it("供应商配置的规范化与校验规则与 Rust 端一致", () => {
    expect(
      normalizeProviderProfile(
        providerProfile({
          baseUrl: "  https://llm.example.com/v1///  ",
          model: "  m  ",
        }),
      ),
    ).toEqual(providerProfile({ baseUrl: "https://llm.example.com/v1", model: "m" }));

    expect(validateProviderProfile(providerProfile())).toBeNull();
    expect(
      validateProviderProfile(providerProfile({ baseUrl: "ftp://example.com" })),
    ).toContain("http:// 或 https://");
    expect(
      validateProviderProfile(providerProfile({ model: "" })),
    ).toContain("模型 ID 不能为空");
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