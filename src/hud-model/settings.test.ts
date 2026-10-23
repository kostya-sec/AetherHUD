import { describe, expect, it } from "vitest";
import {
  createDefaultHudState,
  createHudStore,
  hudReducer,
  resolveEscapeCommand,
  selectCredentialValidation,
  selectSettings,
  selectSettingsOpen,
  selectSettingsRow,
  selectSettingsSection,
  MESSAGE_TYPE_OPTIONS,
  PROVIDER_PRESETS,
  providerProfileDraft,
  SETTINGS_ROW_ITEMS,
  SETTINGS_SECTIONS,
  type CredentialStatus,
  type CredentialValidationView,
  type ProviderProfile,
} from "./index";

const status = (overrides: Partial<CredentialStatus> = {}): CredentialStatus => ({
  kind: "classifierApiKey",
  account: "classifier.api_key",
  label: "百炼 API Key",
  state: "not_configured",
  reference: "keychain://aetherhud/classifier.api_key",
  ...overrides,
});

const validation = (
  overrides: Partial<CredentialValidationView> = {},
): CredentialValidationView => ({
  target: "classifier",
  ok: true,
  detail: "百炼连接正常（178 个模型）",
  ...overrides,
});

function openedSettings() {
  return hudReducer(createDefaultHudState(), { type: "open-settings" });
}

const providerProfile = (
  overrides: Partial<ProviderProfile> = {},
): ProviderProfile => ({
  preset: "openai",
  baseUrl: "https://api.openai.com/v1",
  model: "gpt-4o-mini",
  messageType: "chat_completions",
  ...overrides,
});

describe("设置弹层（#29）", () => {
  it("默认关闭，入口行的「设置」已接通", () => {
    const state = createDefaultHudState();
    expect(selectSettingsOpen(state)).toBe(false);
    expect(
      SETTINGS_ROW_ITEMS.find((item) => item.id === "settings")?.enabled,
    ).toBe(true);
  });

  it("十区顺序与规格一致，只有凭证区本票落地", () => {
    expect(SETTINGS_SECTIONS.map((section) => section.id)).toEqual([
      "general",
      "appearance",
      "classifier",
      "network",
      "credentials",
      "ingestion",
      "tags",
      "taxonomy",
      "data",
      "about",
    ]);
    expect(
      SETTINGS_SECTIONS.filter((section) => section.implemented).map(
        (section) => section.id,
      ),
    ).toEqual(["credentials"]);
  });

  it("从入口行打开设置：收起入口行与面板，默认通用分区并开始读取凭证状态", () => {
    let state = hudReducer(createDefaultHudState(), { type: "toggle-settings-row" });
    expect(selectSettingsRow(state).expanded).toBe(true);

    state = hudReducer(state, { type: "open-settings" });

    expect(selectSettingsOpen(state)).toBe(true);
    expect(selectSettingsRow(state).expanded).toBe(false);
    expect(selectSettingsSection(state)).toBe("general");
    expect(selectSettings(state).credentials.loading).toBe(true);
  });

  it("托盘/关于入口可以指定分区；已打开时只切换分区", () => {
    let state = hudReducer(createDefaultHudState(), {
      type: "open-settings",
      section: "about",
    });
    expect(selectSettingsSection(state)).toBe("about");

    state = hudReducer(state, { type: "select-settings-section", section: "credentials" });
    expect(selectSettingsSection(state)).toBe("credentials");
    // 打开状态下再发一次打开命令只切分区，不重置凭证区数据。
    state = hudReducer(state, { type: "select-settings-section", section: "general" });
    state = hudReducer(state, { type: "open-settings", section: "credentials" });
    expect(selectSettingsSection(state)).toBe("credentials");
  });

  it("Esc 先关闭设置弹层，回到 HUD 原处（再按一次才收起 HUD）", () => {
    const state = openedSettings();
    const command = resolveEscapeCommand(state);
    expect(command).toEqual({ type: "close-settings" });

    const closed = hudReducer(state, command!);
    expect(selectSettingsOpen(closed)).toBe(false);
    expect(resolveEscapeCommand(closed)).toBeNull();
  });

  it("唤出重置会关掉设置弹层", () => {
    const state = hudReducer(openedSettings(), {
      type: "set-credential-statuses",
      statuses: [status()],
    });
    const summoned = hudReducer(state, { type: "summon" });

    expect(selectSettingsOpen(summoned)).toBe(false);
    expect(selectSettings(summoned).credentials.statuses).toEqual([]);
  });

  it("收起动画期间冻结设置命令", () => {
    const state = openedSettings();
    const dismissing = hudReducer(state, { type: "dismiss-start" });

    expect(hudReducer(dismissing, { type: "select-settings-section", section: "network" })).toBe(
      dismissing,
    );
    expect(hudReducer(dismissing, { type: "close-settings" })).toBe(dismissing);
  });

  it("凭证状态只在弹层打开时写入，关闭后迟到的响应被忽略", () => {
    const closed = createDefaultHudState();
    expect(
      hudReducer(closed, { type: "set-credential-statuses", statuses: [status()] }),
    ).toBe(closed);

    const open = hudReducer(openedSettings(), {
      type: "set-credential-statuses",
      statuses: [status()],
    });
    expect(selectSettings(open).credentials.statuses).toHaveLength(1);
    expect(selectSettings(open).credentials.loading).toBe(false);
  });

  it("单项状态更新会清掉同一外部服务的过期验证结论，保留其他服务的", () => {
    let state = hudReducer(openedSettings(), {
      type: "set-credential-validation",
      validation: validation(),
    });
    state = hudReducer(state, {
      type: "set-credential-validation",
      validation: validation({ target: "jina", detail: "Jina Reader 可用（一次读取成功）" }),
    });

    state = hudReducer(state, {
      type: "update-credential-status",
      status: status({ state: "configured" }),
    });

    expect(selectCredentialValidation(selectSettings(state).credentials, "classifier")).toBeNull();
    expect(selectCredentialValidation(selectSettings(state).credentials, "jina")?.ok).toBe(true);
  });

  it("验证结果按 target 覆盖，忙碌与提示互不干扰", () => {
    let state = openedSettings();
    state = hudReducer(state, { type: "set-credential-busy", busy: "feishu" });
    expect(selectSettings(state).credentials.busy).toBe("feishu");

    state = hudReducer(state, {
      type: "set-credential-validation",
      validation: validation({ target: "feishu", ok: false, detail: "请先保存飞书 App ID 与 App Secret" }),
    });
    state = hudReducer(state, {
      type: "set-credential-validation",
      validation: validation({ target: "feishu", ok: true, detail: "飞书 tenant_access_token 获取成功（有效期 7200 秒）" }),
    });
    state = hudReducer(state, {
      type: "set-credential-message",
      message: { tone: "ok", text: "百炼 API Key 已保存到系统钥匙串" },
    });

    expect(selectCredentialValidation(selectSettings(state).credentials, "feishu")?.detail).toContain(
      "7200",
    );
    expect(selectSettings(state).credentials.message?.tone).toBe("ok");
  });

  it("供应商预设按百炼→OpenAI→DeepSeek→自定义排序，默认 Chat Completions", () => {
    expect(PROVIDER_PRESETS.map((preset) => preset.id)).toEqual([
      "bailian",
      "openai",
      "deepseek",
      "custom",
    ]);
    expect(PROVIDER_PRESETS[0]).toMatchObject({
      baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
      model: "qwen-flash",
      messageType: "chat_completions",
    });
    expect(PROVIDER_PRESETS[1]).toMatchObject({
      baseUrl: "https://api.openai.com/v1",
      model: "gpt-4o-mini",
    });
    expect(PROVIDER_PRESETS[3]).toMatchObject({
      baseUrl: "",
      model: "",
      messageType: "chat_completions",
    });
    expect(MESSAGE_TYPE_OPTIONS.map((option) => option.id)).toEqual([
      "chat_completions",
      "responses",
    ]);
    expect(providerProfileDraft("custom")).toEqual({
      preset: "custom",
      baseUrl: "",
      model: "",
      messageType: "chat_completions",
    });
  });

  it("供应商配置只在弹层打开时写入，关闭后迟到响应被忽略", () => {
    const closed = createDefaultHudState();
    expect(
      hudReducer(closed, { type: "set-provider-profile", profile: providerProfile() }),
    ).toBe(closed);
    expect(
      hudReducer(closed, {
        type: "set-provider-profile-message",
        message: { tone: "ok", text: "已保存" },
      }),
    ).toBe(closed);

    let state = openedSettings();
    state = hudReducer(state, {
      type: "set-provider-profile",
      profile: providerProfile(),
    });
    expect(selectSettings(state).credentials.profile).toEqual(providerProfile());

    state = hudReducer(state, { type: "set-provider-profile-busy", busy: true });
    expect(selectSettings(state).credentials.profileBusy).toBe(true);

    state = hudReducer(state, {
      type: "set-provider-profile-message",
      message: { tone: "ok", text: "供应商配置已保存" },
    });
    expect(selectSettings(state).credentials.profileMessage).toEqual({
      tone: "ok",
      text: "供应商配置已保存",
    });
  });

  it("重新打开设置会清掉供应商提示与忙碌，但保留上次读到的配置", () => {
    let state = openedSettings();
    state = hudReducer(state, {
      type: "set-provider-profile",
      profile: providerProfile(),
    });
    state = hudReducer(state, {
      type: "set-provider-profile-message",
      message: { tone: "error", text: "保存失败" },
    });
    state = hudReducer(state, { type: "set-provider-profile-busy", busy: true });
    state = hudReducer(state, { type: "close-settings" });
    expect(selectSettings(state).credentials.profileBusy).toBe(false);

    state = hudReducer(state, { type: "open-settings" });
    expect(selectSettings(state).credentials.profile).toEqual(providerProfile());
    expect(selectSettings(state).credentials.profileMessage).toBeNull();
  });

  it("唤出重置会清掉供应商配置，下次打开由平台层重新加载", () => {
    let state = openedSettings();
    state = hudReducer(state, {
      type: "set-provider-profile",
      profile: providerProfile(),
    });
    state = hudReducer(state, { type: "summon" });
    expect(selectSettings(state).credentials.profile).toBeNull();
  });

  it("供应商配置变化会作废旧的分类器验证结论，保留其他服务", () => {
    let state = hudReducer(openedSettings(), {
      type: "set-provider-profile",
      profile: {
        preset: "bailian",
        baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
        model: "qwen-flash",
        messageType: "chat_completions",
      },
    });
    state = hudReducer(state, {
      type: "set-credential-validation",
      validation: validation(),
    });
    state = hudReducer(state, {
      type: "set-credential-validation",
      validation: validation({
        target: "jina",
        detail: "Jina Reader 可用（一次读取成功）",
      }),
    });

    // 真正切换供应商（规范化后的等价配置不应触发作废）。
    state = hudReducer(state, {
      type: "set-provider-profile",
      profile: {
        preset: "deepseek",
        baseUrl: "https://api.deepseek.com/v1",
        model: "deepseek-chat",
        messageType: "chat_completions",
      },
    });

    expect(selectCredentialValidation(selectSettings(state).credentials, "classifier")).toBeNull();
    expect(
      selectCredentialValidation(selectSettings(state).credentials, "jina")?.ok,
    ).toBe(true);
  });

  it("回读等价供应商配置不改变状态（避免无谓作废验证）", () => {
    const profile = {
      preset: "bailian" as const,
      baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
      model: "qwen-flash",
      messageType: "chat_completions" as const,
    };
    const state = hudReducer(openedSettings(), {
      type: "set-provider-profile",
      profile,
    });

    expect(
      hudReducer(state, {
        type: "set-provider-profile",
        profile: {
          ...profile,
          baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1/",
        },
      }),
    ).toBe(state);
  });

  it("从托盘事件打开时先唤出重置、再打开指定分区（事件顺序）", () => {
    const store = createHudStore();
    store.dispatch({ type: "open-inbox" });
    store.dispatch({ type: "summon" });
    store.dispatch({ type: "open-settings", section: "about" });

    expect(selectSettingsOpen(store.getState())).toBe(true);
    expect(selectSettingsSection(store.getState())).toBe("about");
    expect(store.getState().inbox.open).toBe(false);
  });
});