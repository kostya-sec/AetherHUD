import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  normalizeProviderProfile,
  providerPreset,
  validateProviderProfile,
  type CredentialKind,
  type CredentialStatus,
  type HudStore,
  type ProviderProfile,
  type ValidationTarget,
} from "../hud-model";

// 纯函数留在 hud-model 供组件复用；这里转出保持既有调用方/测试的导入路径不变。
export { normalizeProviderProfile, validateProviderProfile };

/** Shell → webview: tray「设置…」；与 src-tauri/src/events.rs 保持同步。 */
export const SETTINGS_OPEN_EVENT = "hud://settings/open";
/** Shell → webview: tray「关于 / 检查更新」；本票先打开设置弹层的「关于」分区。 */
export const ABOUT_OPEN_EVENT = "hud://about/open";

type Dispatch = HudStore["dispatch"];

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
 * 凭证提示里的展示名：分类器 Key 的 core 固定 label 是「百炼 API Key」，换成
 * 其他供应商后与卡片里随活动供应商变化的展示名不一致，提示改用供应商无关的名字。
 */
function credentialMessageLabel(kind: CredentialKind, status: CredentialStatus): string {
  return kind === "classifierApiKey" ? "分类器 API Key" : status.label;
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
        text: `${credentialMessageLabel(kind, status)} 已保存到系统钥匙串（数据库只存引用）`,
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
        text: `${credentialMessageLabel(kind, status)} 已从系统钥匙串清除`,
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