import type { ProviderProfile } from "./types";

/** base URL / 模型 ID 的长度上限（与 aether-core::provider 保持一致）。 */
export const PROVIDER_BASE_URL_MAX = 2048;
export const PROVIDER_MODEL_MAX = 256;

/** 保存前规范化：去空白、去掉 base URL 末尾的 `/`（与 Rust 端一致）。 */
export function normalizeProviderProfile(profile: ProviderProfile): ProviderProfile {
  return {
    preset: profile.preset,
    baseUrl: profile.baseUrl.trim().replace(/\/+$/, ""),
    model: profile.model.trim(),
    messageType: profile.messageType,
  };
}

/** 两份供应商配置规范化后是否等价（保存/读取的轻微格式差异不算改动）。 */
export function providerProfilesEqual(
  left: ProviderProfile,
  right: ProviderProfile,
): boolean {
  const a = normalizeProviderProfile(left);
  const b = normalizeProviderProfile(right);
  return (
    a.preset === b.preset &&
    a.baseUrl === b.baseUrl &&
    a.model === b.model &&
    a.messageType === b.messageType
  );
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
