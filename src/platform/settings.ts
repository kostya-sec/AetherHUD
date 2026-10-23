import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  providerPreset,
  type CredentialKind,
  type CredentialStatus,
  type HudStore,
  type ProviderProfile,
  type ValidationTarget,
} from "../hud-model";

/** Shell → webview: tray「设置…」；与 src-tauri/src/events.rs 保持同步。 */
export const SETTINGS_OPEN_EVENT = "hud://settings/open";
/** Shell → webview: tray「关于 / 检查更新」；本票先打开设置弹层的「关于」分区。 */
export const ABOUT_OPEN_EVENT = "hud://about/open";

type Dispatch = HudStore["dispatch"];

/** base URL / 模型 ID 的长度上限与 Rust 端保持一致。 */
const PROVIDER_BASE_URL_MAX = 2048;
const PROVIDER_MODEL_MAX = 256;

/** 保存前规范化：去空白、去掉 base URL 末尾的 `/`（与 Rust 端一致）。 */
export function normalizeProviderProfile(profile: ProviderProfile): ProviderProfile {
  return {
    preset: profile.preset,
    baseUrl: profile.baseUrl.trim().replace(/\/+$/, ""),
    model: profile.model.trim(),
    messageType: profile.messageType,
  };
}

/**
 * 供应商配置的前端校验（后端会再校验一次）：只给可读原因，不猜端点是否真实存在。
 * 返回 null 表示通过。
 */
export function validateProviderProfile(profile: ProviderProfile): string | null {
  const baseUrl = profile.baseUrl.trim();
  if (!baseUrl) {
    return "Base URL 不能为空（自定义供应商需填写完整端点，例如 https://api.example.com/v1）";
  }
  if (baseUrl.length > PROVIDER_BASE_URL_MAX) {
    return `Base URL 过长（最多 ${PROVIDER_BASE_URL_MAX} 个字符）`;
  }
  if (!/^https?:\/\//i.test(baseUrl)) {
    return `Base URL 必须以 http:// 或 https:// 开头：${baseUrl}`;
  }
  try {
    new URL(baseUrl);
  } catch {
    return `Base URL 格式不正确：${baseUrl}`;
  }
  const model = profile.model.trim();
  if (!model) {
    return "模型 ID 不能为空（例如 qwen-flash / gpt-4o-mini / deepseek-chat）";
  }
  if (model.length > PROVIDER_MODEL_MAX) {
    return `模型 ID 过长（最多 ${PROVIDER_MODEL_MAX} 个字符）`;
  }
  return null;
}

/**
 * 读取活动供应商配置（#45 前置）。读取失败只给一次性提示，设置页其余部分照常可用；
 * 界面在拿到配置前用百炼默认值当草稿。
 */
export async function loadProviderProfile(dispatch: Dispatch): Promise<void> {
  try {
    const profile = await invoke<ProviderProfile>("settings_provider_profile");
    dispatch({ type: "set-provider-profile", profile });
  } catch (error) {
    dispatch({
      type: "set-provider-profile-message",
      message: {
        tone: "error",
        text: `读取供应商配置失败：${String(error)}`,
      },
    });
  }
}

/**
 * 保存活动供应商配置：先做前端校验（可读错误），再经 IPC 落 settings。
 * 返回 true 表示保存成功（配置本身保留，不清空草稿）。
 */
export async function saveProviderProfile(
  dispatch: Dispatch,
  profile: ProviderProfile,
): Promise<boolean> {
  const normalized = normalizeProviderProfile(profile);
  const invalid = validateProviderProfile(normalized);
  if (invalid) {
    dispatch({
      type: "set-provider-profile-message",
      message: { tone: "error", text: invalid },
    });
    return false;
  }

  dispatch({ type: "set-provider-profile-busy", busy: true });
  try {
    const saved = await invoke<ProviderProfile>("settings_save_provider_profile", {
      profile: normalized,
    });
    dispatch({ type: "set-provider-profile", profile: saved });
    dispatch({
      type: "set-provider-profile-message",
      message: {
        tone: "ok",
        text: `供应商配置已保存（${providerPreset(saved.preset).label} · ${saved.model}）`,
      },
    });
    return true;
  } catch (error) {
    dispatch({
      type: "set-provider-profile-message",
      message: { tone: "error", text: `保存供应商配置失败：${String(error)}` },
    });
    return false;
  } finally {
    dispatch({ type: "set-provider-profile-busy", busy: false });
  }
}

/**
 * 读取凭证区四行状态（#29）。核心库不可用时清空状态并给出一次性提示，
 * 设置弹层其余分区仍可浏览——凭证读取失败不能让设置页崩溃。
 */
export async function loadCredentialStatuses(dispatch: Dispatch): Promise<void> {
  dispatch({ type: "set-credentials-loading", loading: true });
  try {
    const statuses = await invoke<CredentialStatus[]>("settings_credential_statuses");
    dispatch({ type: "set-credential-statuses", statuses });
  } catch (error) {
    dispatch({ type: "set-credential-statuses", statuses: [] });
    dispatch({
      type: "set-credential-message",
      message: {
        tone: "error",
        text: `读取凭证状态失败：${String(error)}`,
      },
    });
  }
}

/**
 * 保存一项凭证：密钥经 IPC 进系统钥匙串，界面只拿回状态与引用。
 * 返回 true 表示保存成功（调用方据此清空输入框）；失败时保留输入便于重试。
 */
export async function saveCredential(
  dispatch: Dispatch,
  kind: CredentialKind,
  secret: string,
): Promise<boolean> {
  const trimmed = secret.trim();
  if (!trimmed) return false;

  dispatch({ type: "set-credential-busy", busy: kind });
  try {
    const status = await invoke<CredentialStatus>("settings_save_credential", {
      kind,
      secret: trimmed,
    });
    dispatch({ type: "update-credential-status", status });
    dispatch({
      type: "set-credential-message",
      message: {
        tone: "ok",
        text: `${status.label} 已保存到系统钥匙串（数据库只存引用）`,
      },
    });
    return true;
  } catch (error) {
    dispatch({
      type: "set-credential-message",
      message: { tone: "error", text: `保存失败：${String(error)}` },
    });
    return false;
  } finally {
    dispatch({ type: "set-credential-busy", busy: null });
  }
}

/** 清除一项凭证（幂等）；钥匙串与引用一起删除。 */
export async function clearCredential(
  dispatch: Dispatch,
  kind: CredentialKind,
): Promise<boolean> {
  dispatch({ type: "set-credential-busy", busy: kind });
  try {
    const status = await invoke<CredentialStatus>("settings_clear_credential", { kind });
    dispatch({ type: "update-credential-status", status });
    dispatch({
      type: "set-credential-message",
      message: {
        tone: "ok",
        text: `${status.label} 已从系统钥匙串清除`,
      },
    });
    return true;
  } catch (error) {
    dispatch({
      type: "set-credential-message",
      message: { tone: "error", text: `清除失败：${String(error)}` },
    });
    return false;
  } finally {
    dispatch({ type: "set-credential-busy", busy: null });
  }
}

/**
 * 验证连通性：百炼（models）、Jina（一次读）、飞书（tenant_access_token）。
 * 结果（含可读失败原因）写进对应 target，弹层内联展示；错误串已由核心库脱敏。
 */
export async function validateCredential(
  dispatch: Dispatch,
  target: ValidationTarget,
): Promise<void> {
  dispatch({ type: "set-credential-busy", busy: target });
  try {
    const outcome = await invoke<{ ok: boolean; detail: string }>(
      "settings_validate_credential",
      { target },
    );
    dispatch({
      type: "set-credential-validation",
      validation: { target, ok: outcome.ok, detail: outcome.detail },
    });
  } catch (error) {
    dispatch({
      type: "set-credential-validation",
      validation: {
        target,
        ok: false,
        detail: `验证请求失败：${String(error)}`,
      },
    });
  } finally {
    dispatch({ type: "set-credential-busy", busy: null });
  }
}

/** 设置弹层底部「退出 AetherHUD」：托盘之外唯一的退出路径（#24 §10）。 */
export function quitApp(): void {
  try {
    void invoke("quit_app").catch(() => undefined);
  } catch {
    // Plain-browser dev has no Tauri IPC bridge.
  }
}

/**
 * 托盘「设置…」/「关于」入口（#24 §10）。壳先唤出 HUD 再发事件，所以这里只负责
 * 打开弹层：唤出事件已把 HUD 重置回 S0，设置弹层不会被重置清掉。
 */
export async function connectSettingsEvents(store: HudStore): Promise<() => void> {
  const settled = await Promise.allSettled([
    listen(SETTINGS_OPEN_EVENT, () =>
      store.dispatch({ type: "open-settings", section: "general" }),
    ),
    listen(ABOUT_OPEN_EVENT, () =>
      store.dispatch({ type: "open-settings", section: "about" }),
    ),
  ]);
  const unlisteners: UnlistenFn[] = settled.flatMap((result) =>
    result.status === "fulfilled" ? [result.value] : [],
  );
  return () => {
    for (const unlisten of unlisteners) unlisten();
    unlisteners.length = 0;
  };
}