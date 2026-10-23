import { useEffect, useState, type ReactNode } from "react";
import {
  CREDENTIAL_KINDS,
  MESSAGE_TYPE_OPTIONS,
  PROVIDER_PRESETS,
  providerPreset,
  providerProfileDraft,
  selectCredentialValidation,
  type CredentialKind,
  type CredentialMessage,
  type CredentialState,
  type CredentialStatus,
  type CredentialValidationView,
  type CredentialsState,
  type MessageType,
  type ProviderProfile,
  type ValidationTarget,
} from "../hud-model";

interface HudCredentialsPaneProps {
  credentials: CredentialsState;
  /** 返回 true = 保存成功（调用方据此清空该行输入框）。 */
  onSaveCredential: (kind: CredentialKind, secret: string) => Promise<boolean>;
  onClearCredential: (kind: CredentialKind) => Promise<boolean>;
  onValidate: (target: ValidationTarget) => Promise<void>;
  /** 保存活动供应商配置（base URL / model / 消息格式）；返回 true = 保存成功。 */
  onSaveProviderProfile: (profile: ProviderProfile) => Promise<boolean>;
}

/**
 * 设置弹层的「凭证」分区（#29）：逐项保存 / 清除 / 显示已配置状态，并验证
 * 百炼、Jina、飞书的连通性。Keychain 写入与验证都在 core 层，组件只负责呈现。
 *
 * 每组末尾显示一次验证结论；保存/清除后同一外部服务的旧结论由 reducer 作废。
 */
export function HudCredentialsPane({
  credentials,
  onSaveCredential,
  onClearCredential,
  onValidate,
  onSaveProviderProfile,
}: HudCredentialsPaneProps) {
  const [drafts, setDrafts] = useState<Partial<Record<CredentialKind, string>>>({});
  // 供应商配置是本地草稿：配置本身要保留，保存成功后不清空（与密钥输入框不同）。
  const [providerDraft, setProviderDraft] = useState<ProviderProfile>(() =>
    credentials.profile ?? providerProfileDraft("bailian"),
  );
  // 平台层读到（或保存回读）配置后同步草稿；未保存时保留用户正在编辑的内容。
  useEffect(() => {
    if (credentials.profile) setProviderDraft(credentials.profile);
  }, [credentials.profile]);

  const statusByKind = new Map(credentials.statuses.map((status) => [status.kind, status]));
  const rows = CREDENTIAL_KINDS.map((kind) => statusByKind.get(kind)).filter(
    (status): status is CredentialStatus => status !== undefined,
  );

  const setDraft = (kind: CredentialKind, value: string) => {
    setDrafts((current) => ({ ...current, [kind]: value }));
  };

  const save = async (kind: CredentialKind) => {
    const secret = (drafts[kind] ?? "").trim();
    if (!secret) return;
    if (await onSaveCredential(kind, secret)) {
      setDraft(kind, "");
    }
  };

  const row = (kind: CredentialKind, label?: string) => {
    const status = statusByKind.get(kind);
    if (!status) return null;
    return (
      <CredentialRow
        key={kind}
        status={status}
        label={label}
        draft={drafts[kind] ?? ""}
        busy={credentials.busy === kind}
        onDraftChange={(value) => setDraft(kind, value)}
        onSave={() => void save(kind)}
        onClear={() => void onClearCredential(kind)}
      />
    );
  };

  const validationFor = (target: ValidationTarget): CredentialValidationView | null =>
    selectCredentialValidation(credentials, target);

  const savedProfile = credentials.profile;
  const savedPreset = providerPreset(savedProfile?.preset ?? "bailian");
  const classifierHint = savedProfile?.baseUrl
    ? `GET ${savedProfile.baseUrl}/models`
    : "保存供应商配置后按活动 Base URL 验证";

  return (
    <div data-settings-credentials className="flex flex-col gap-3">
      {credentials.message && (
        <p
          data-credential-message={credentials.message.tone}
          className={`rounded border px-2.5 py-1.5 font-mono text-[11px] ${
            credentials.message.tone === "ok"
              ? "border-emerald-500/50 bg-emerald-500/10 text-emerald-200"
              : "border-rose-500/50 bg-rose-500/10 text-rose-200"
          }`}
        >
          {credentials.message.text}
        </p>
      )}

      <ProviderCard
        draft={providerDraft}
        busy={credentials.profileBusy}
        message={credentials.profileMessage}
        onChange={setProviderDraft}
        onSave={() => void onSaveProviderProfile(providerDraft)}
      />

      {rows.length === 0 ? (
        <div className="flex h-40 items-center justify-center rounded border border-dashed border-stone-700/70">
          <p className="font-mono text-xs text-stone-400">
            {credentials.loading ? "正在读取凭证状态…" : "凭证状态不可用（核心库未启动）"}
          </p>
        </div>
      ) : (
        <>
          <CredentialGroup
            title={`分类器密钥 · ${savedPreset.label}`}
            hint={classifierHint}
            rows={row("classifierApiKey", `${savedPreset.label} API Key`)}
            validation={validationFor("classifier")}
            validating={credentials.busy === "classifier"}
            canValidate={statusByKind.get("classifierApiKey")?.state === "configured"}
            idleHint={
              savedProfile
                ? "验证会使用活动供应商的 Base URL 与模型。"
                : "先保存供应商配置，再点「验证连接」检查 Key、权限或代理。"
            }
            onValidate={() => void onValidate("classifier")}
          />
          <CredentialGroup
            title="Jina Reader"
            hint="GET r.jina.ai 一次读（本机通常需要代理）"
            rows={row("jinaApiKey")}
            validation={validationFor("jina")}
            validating={credentials.busy === "jina"}
            canValidate={statusByKind.get("jinaApiKey")?.state === "configured"}
            onValidate={() => void onValidate("jina")}
          />
          <CredentialGroup
            title="飞书开放平台"
            hint="POST tenant_access_token/internal"
            rows={
              <>
                {row("feishuAppId")}
                {row("feishuAppSecret")}
              </>
            }
            validation={validationFor("feishu")}
            validating={credentials.busy === "feishu"}
            canValidate={
              statusByKind.get("feishuAppId")?.state === "configured" &&
              statusByKind.get("feishuAppSecret")?.state === "configured"
            }
            onValidate={() => void onValidate("feishu")}
          />
          <YouTubePlaceholder />
          <p className="font-mono text-[10px] leading-relaxed text-stone-500">
            飞书 Base app_token 与表格字段映射在「摄入与同步」分区（#44）；代理密码在「网络」分区（#45）。
          </p>
        </>
      )}
    </div>
  );
}

interface ProviderCardProps {
  draft: ProviderProfile;
  busy: boolean;
  message: CredentialMessage | null;
  onChange: (profile: ProviderProfile) => void;
  onSave: () => void;
}

const PROVIDER_FIELD_CLASS =
  "rounded border border-stone-700/80 bg-stone-950/60 px-2 py-1 font-mono text-[11px] text-stone-100 outline-none placeholder:text-stone-600 focus:border-amber-500/80";

/**
 * 「分类器供应商」卡片（#45 前置）：预设 + Base URL + 模型 + 消息格式。
 *
 * 单一活动供应商：切换预设只填默认值，Key 仍共用 `classifier.api_key`（换供应商
 * 要重填 Key）。配置是非秘密 settings，保存后不清空草稿。
 */
function ProviderCard({ draft, busy, message, onChange, onSave }: ProviderCardProps) {
  const presetMeta = providerPreset(draft.preset);
  const messageTypeMeta =
    MESSAGE_TYPE_OPTIONS.find((option) => option.id === draft.messageType) ??
    MESSAGE_TYPE_OPTIONS[0];
  return (
    <div
      data-provider-card
      className="rounded-lg border border-amber-500/25 bg-stone-950/30 p-3"
    >
      <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-baseline gap-2">
          <h4 className="font-mono text-[11px] text-amber-200/90">分类器供应商</h4>
          <span className="font-mono text-[9px] text-stone-600">
            同一时间只启用一套配置；换供应商需重填 Key
          </span>
        </div>
        <button
          type="button"
          data-hud-hitbox
          data-provider-save
          onClick={onSave}
          disabled={busy}
          className="pixel-box-sm hud-focus-ring rounded border border-amber-500/70 bg-amber-500/10 px-2.5 py-1 font-mono text-[10px] text-amber-100 transition-colors hover:bg-amber-500/20 disabled:opacity-50"
        >
          {busy ? "保存中…" : "保存供应商配置"}
        </button>
      </div>

      <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
        <label className="flex min-w-0 flex-col gap-1 font-mono text-[10px] text-stone-400">
          供应商预设
          <select
            data-hud-hitbox
            data-provider-preset
            value={draft.preset}
            onChange={(event) => onChange(providerProfileDraft(event.target.value as ProviderProfile["preset"]))}
            aria-label="供应商预设"
            className={PROVIDER_FIELD_CLASS}
          >
            {PROVIDER_PRESETS.map((preset) => (
              <option key={preset.id} value={preset.id}>
                {preset.label}
              </option>
            ))}
          </select>
        </label>

        <label className="flex min-w-0 flex-col gap-1 font-mono text-[10px] text-stone-400">
          消息格式
          <select
            data-hud-hitbox
            data-provider-message-type
            value={draft.messageType}
            onChange={(event) =>
              onChange({ ...draft, messageType: event.target.value as MessageType })
            }
            aria-label="消息格式"
            className={PROVIDER_FIELD_CLASS}
          >
            {MESSAGE_TYPE_OPTIONS.map((option) => (
              <option key={option.id} value={option.id}>
                {option.label}
              </option>
            ))}
          </select>
        </label>

        <label className="flex min-w-0 flex-col gap-1 font-mono text-[10px] text-stone-400 sm:col-span-2">
          Base URL
          <input
            type="url"
            value={draft.baseUrl}
            data-hud-hitbox
            data-provider-base-url
            onChange={(event) => onChange({ ...draft, baseUrl: event.target.value })}
            placeholder="https://api.example.com/v1"
            spellCheck={false}
            aria-label="供应商 Base URL"
            className={PROVIDER_FIELD_CLASS}
          />
        </label>

        <label className="flex min-w-0 flex-col gap-1 font-mono text-[10px] text-stone-400">
          模型 ID
          <input
            type="text"
            value={draft.model}
            data-hud-hitbox
            data-provider-model
            onChange={(event) => onChange({ ...draft, model: event.target.value })}
            placeholder="qwen-flash"
            spellCheck={false}
            aria-label="供应商模型 ID"
            className={PROVIDER_FIELD_CLASS}
          />
        </label>

        <p className="self-end font-mono text-[9px] leading-relaxed text-stone-600">
          {presetMeta.description}
          <br />
          {messageTypeMeta.description}
        </p>
      </div>

      {message && (
        <p
          data-provider-message={message.tone}
          className={`mt-2 rounded border px-2 py-1 font-mono text-[10px] leading-relaxed ${
            message.tone === "ok"
              ? "border-emerald-500/40 bg-emerald-500/5 text-emerald-200"
              : "border-rose-500/40 bg-rose-500/5 text-rose-200"
          }`}
        >
          {message.tone === "ok" ? "✓ " : "✗ "}
          {message.text}
        </p>
      )}

      <p className="mt-2 font-mono text-[9px] leading-relaxed text-stone-600">
        Key 存在系统钥匙串的固定条目 <span className="text-stone-500">classifier.api_key</span>
        ：换供应商后请重新保存 Key，再验证连接。
      </p>
    </div>
  );
}

/** #24 §8 的 YouTube Key 占位：v1 不配置，视频元数据走 oEmbed。 */
function YouTubePlaceholder() {
  return (
    <div
      data-credential-placeholder="youtube"
      className="flex flex-wrap items-center justify-between gap-2 rounded border border-dashed border-stone-800/80 px-2.5 py-2"
    >
      <div className="flex items-center gap-2">
        <span className="font-mono text-[11px] text-stone-400">YouTube Data API Key</span>
        <span className="rounded border border-stone-700/70 px-1.5 py-0.5 font-mono text-[9px] text-stone-500">
          占位
        </span>
      </div>
      <span className="font-mono text-[10px] text-stone-600">
        v1 不配置，视频元数据走 oEmbed
      </span>
    </div>
  );
}

const CREDENTIAL_STATE_BADGE: Record<
  CredentialState,
  { label: string; className: string }
> = {
  configured: { label: "已配置", className: "border-emerald-500/60 text-emerald-300" },
  not_configured: { label: "未配置", className: "border-stone-600/80 text-stone-400" },
  unavailable: { label: "钥匙串不可用", className: "border-rose-500/60 text-rose-300" },
};

interface CredentialGroupProps {
  title: string;
  hint: string;
  rows: ReactNode;
  validation: CredentialValidationView | null;
  validating: boolean;
  canValidate: boolean;
  /** 未验证过时的引导语（分类器组提示先保存供应商配置）。 */
  idleHint?: string;
  onValidate: () => void;
}

function CredentialGroup({
  title,
  hint,
  rows,
  validation,
  validating,
  canValidate,
  idleHint,
  onValidate,
}: CredentialGroupProps) {
  return (
    <div className="rounded-lg border border-stone-800/80 bg-stone-950/30 p-3">
      <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-baseline gap-2">
          <h4 className="font-mono text-[11px] text-stone-200">{title}</h4>
          <span className="font-mono text-[9px] text-stone-600">{hint}</span>
        </div>
        <button
          type="button"
          data-hud-hitbox
          data-credential-action="validate"
          onClick={onValidate}
          disabled={!canValidate || validating}
          className="pixel-box-sm hud-focus-ring rounded border border-stone-700 px-2.5 py-1 font-mono text-[10px] text-stone-200 transition-colors hover:border-amber-500/70 hover:text-white disabled:opacity-50"
        >
          {validating ? "验证中…" : "验证连接"}
        </button>
      </div>
      <div className="flex flex-col gap-2">{rows}</div>
      <ValidationLine validation={validation} idleHint={idleHint} />
    </div>
  );
}

interface CredentialRowProps {
  status: CredentialStatus;
  /** 界面展示名覆盖（分类器 Key 随活动供应商改名；account 仍固定）。 */
  label?: string;
  draft: string;
  busy: boolean;
  onDraftChange: (value: string) => void;
  onSave: () => void;
  onClear: () => void;
}

function CredentialRow({
  status,
  label,
  draft,
  busy,
  onDraftChange,
  onSave,
  onClear,
}: CredentialRowProps) {
  const displayLabel = label ?? status.label;
  const badge = CREDENTIAL_STATE_BADGE[status.state];
  const referenceLabel =
    status.state === "configured" ? status.reference : `将存于 ${status.reference}`;
  return (
    <div
      data-credential-row={status.kind}
      className="flex flex-wrap items-center justify-between gap-2 rounded border border-stone-800/80 bg-stone-950/40 px-2.5 py-2"
    >
      <div className="min-w-0">
        <div className="flex items-center gap-2">
          <span className="font-mono text-[11px] text-stone-100">{displayLabel}</span>
          <span
            data-credential-state={status.state}
            className={`rounded border px-1.5 py-0.5 font-mono text-[9px] ${badge.className}`}
          >
            {badge.label}
          </span>
        </div>
        <div
          className="mt-0.5 max-w-[20rem] truncate font-mono text-[9px] text-stone-500"
          title={status.reference}
        >
          {referenceLabel}
        </div>
      </div>
      <div className="flex items-center gap-1.5">
        <input
          type="password"
          value={draft}
          data-hud-hitbox
          data-credential-input={status.kind}
          onChange={(event) => onDraftChange(event.target.value)}
          placeholder={
            status.state === "configured" ? "已配置；输入新值可替换" : "输入后保存到系统钥匙串"
          }
          aria-label={`${displayLabel} 输入`}
          className="w-44 rounded border border-stone-700/80 bg-stone-950/60 px-2 py-1 font-mono text-[11px] text-stone-100 outline-none placeholder:text-stone-600 focus:border-amber-500/80"
        />
        <button
          type="button"
          data-hud-hitbox
          data-credential-action="save"
          onClick={onSave}
          disabled={busy || !draft.trim()}
          className="pixel-box-sm hud-focus-ring rounded border border-amber-500/70 bg-amber-500/10 px-2.5 py-1 font-mono text-[10px] text-amber-100 transition-colors hover:bg-amber-500/20 disabled:opacity-50"
        >
          {busy ? "保存中…" : "保存"}
        </button>
        <button
          type="button"
          data-hud-hitbox
          data-credential-action="clear"
          onClick={onClear}
          disabled={busy || status.state === "not_configured"}
          className="pixel-box-sm hud-focus-ring rounded border border-stone-700 px-2.5 py-1 font-mono text-[10px] text-stone-300 transition-colors hover:border-rose-500/70 hover:text-rose-200 disabled:opacity-50"
        >
          清除
        </button>
      </div>
    </div>
  );
}

function ValidationLine({
  validation,
  idleHint,
}: {
  validation: CredentialValidationView | null;
  idleHint?: string;
}) {
  if (!validation) {
    return (
      <p className="mt-2 font-mono text-[10px] text-stone-600">
        {idleHint ?? "保存后点「验证连接」检查 Key、权限或代理。"}
      </p>
    );
  }
  return (
    <p
      data-credential-validation={validation.ok ? "ok" : "error"}
      className={`mt-2 rounded border px-2 py-1 font-mono text-[10px] leading-relaxed ${
        validation.ok
          ? "border-emerald-500/40 bg-emerald-500/5 text-emerald-200"
          : "border-rose-500/40 bg-rose-500/5 text-rose-200"
      }`}
    >
      {validation.ok ? "✓ " : "✗ "}
      {validation.detail}
    </p>
  );
}