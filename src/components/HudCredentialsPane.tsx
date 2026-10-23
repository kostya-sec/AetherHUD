import { useEffect, useRef, useState, type ReactNode } from "react";
import {
  CREDENTIAL_KINDS,
  MESSAGE_TYPE_OPTIONS,
  PROVIDER_PRESETS,
  providerPreset,
  providerProfileDraft,
  providerProfilesEqual,
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
  /** 保存活动供应商配置（base URL / model / 消息类型）；返回 true = 保存成功。 */
  onSaveProviderProfile: (profile: ProviderProfile) => Promise<boolean>;
}

/**
 * 设置弹层的「凭证」分区（#29 / #45 修复）：分类器供应商卡片内含 API Key 行，
 * Jina Reader 与飞书各自一组，逐项保存 / 清除 / 显示已配置状态，并验证连通性。
 * Keychain 写入与验证都在 core 层，组件只负责呈现。
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
  // 平台层读到（或保存回读）配置后同步草稿：用户已编辑过的草稿不被迟到的读取覆盖，
  // 只有「未编辑」或「保存成功后回读」时才接受服务端值。
  const userEditedProviderDraft = useRef(false);
  const savedProfileRef = useRef<ProviderProfile | null>(credentials.profile);
  useEffect(() => {
    const next = credentials.profile;
    if (!next) return;
    const previous = savedProfileRef.current;
    savedProfileRef.current = next;
    setProviderDraft((current) => {
      const untouched = previous === null || providerProfilesEqual(current, previous);
      if (userEditedProviderDraft.current && !untouched) return current;
      userEditedProviderDraft.current = false;
      return next;
    });
  }, [credentials.profile]);

  const changeProviderDraft = (profile: ProviderProfile) => {
    userEditedProviderDraft.current = true;
    setProviderDraft(profile);
  };

  const saveProviderDraft = async (): Promise<boolean> => {
    const saved = await onSaveProviderProfile(providerDraft);
    if (saved) userEditedProviderDraft.current = false;
    return saved;
  };

  const statusByKind = new Map(credentials.statuses.map((status) => [status.kind, status]));
  const rows = CREDENTIAL_KINDS.map((kind) => statusByKind.get(kind)).filter(
    (status): status is CredentialStatus => status !== undefined,
  );
  const savedProfile = credentials.profile;
  // Key 行的展示名随**已保存**的供应商变化（account 与引用固定）。
  const savedPreset = providerPreset(savedProfile?.preset ?? "bailian");

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
        savedProfile={savedProfile}
        busy={credentials.profileBusy}
        message={credentials.profileMessage}
        onChange={changeProviderDraft}
        onSave={() => void saveProviderDraft()}
        keyStatus={statusByKind.get("classifierApiKey") ?? null}
        keyLoading={credentials.loading}
        keyLabel={`${savedPreset.label} API Key`}
        keyDraft={drafts.classifierApiKey ?? ""}
        keyBusy={credentials.busy === "classifierApiKey"}
        onKeyDraftChange={(value) => setDraft("classifierApiKey", value)}
        onSaveKey={() => void save("classifierApiKey")}
        onClearKey={() => void onClearCredential("classifierApiKey")}
        validation={validationFor("classifier")}
        validating={credentials.busy === "classifier"}
        onValidate={() => void onValidate("classifier")}
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
  /** 已保存的活动供应商配置；null = 还没从壳读到或读取失败。 */
  savedProfile: ProviderProfile | null;
  busy: boolean;
  message: CredentialMessage | null;
  keyStatus: CredentialStatus | null;
  keyLoading: boolean;
  /** Key 行的展示名（随已保存供应商变化；account 固定 classifier.api_key）。 */
  keyLabel: string;
  keyDraft: string;
  keyBusy: boolean;
  validation: CredentialValidationView | null;
  validating: boolean;
  onChange: (profile: ProviderProfile) => void;
  onSave: () => void;
  onKeyDraftChange: (value: string) => void;
  onSaveKey: () => void;
  onClearKey: () => void;
  onValidate: () => void;
}

const PROVIDER_FIELD_CLASS =
  "rounded border border-stone-700/80 bg-stone-950/60 px-2 py-1 font-mono text-[11px] text-stone-100 outline-none placeholder:text-stone-600 focus:border-amber-500/80";

/**
 * 「分类器供应商」卡片（#45 前置 / #45 修复）：预设 + 消息类型 + Base URL + 模型，
 * 以及同一区块里的 API Key 行（保存到钥匙串 / 清除）与「验证连接」。
 *
 * 单一活动供应商：切换预设只填默认值，Key 仍共用 `classifier.api_key`（换供应商
 * 要重填 Key）。配置是非秘密 settings，保存后不清空草稿；Key 保存成功后由调用方
 * 清空输入框。
 *
 * 验证只使用**已保存**的配置：草稿有未保存改动 / 尚未保存配置 / 未配置 Key 时
 * 按钮禁用，并在卡片内给出可读原因（不会静默发起网络请求）。
 */
function ProviderCard({
  draft,
  savedProfile,
  busy,
  message,
  keyStatus,
  keyLoading,
  keyLabel,
  keyDraft,
  keyBusy,
  validation,
  validating,
  onChange,
  onSave,
  onKeyDraftChange,
  onSaveKey,
  onClearKey,
  onValidate,
}: ProviderCardProps) {
  const presetMeta = providerPreset(draft.preset);
  const messageTypeMeta =
    MESSAGE_TYPE_OPTIONS.find((option) => option.id === draft.messageType) ??
    MESSAGE_TYPE_OPTIONS[0];

  // 「验证」用已保存的 Base URL / 模型：配置完整、草稿无改动、Key 已配置才可点。
  const savedComplete =
    savedProfile !== null &&
    savedProfile.baseUrl.trim() !== "" &&
    savedProfile.model.trim() !== "";
  const draftDirty =
    savedProfile !== null && !providerProfilesEqual(draft, savedProfile);
  const keyConfigured = keyStatus?.state === "configured";
  const canValidate = savedComplete && keyConfigured && !draftDirty;
  const validationHint = !savedProfile
    ? "先保存供应商配置（Base URL 与模型），再点「验证连接」检查 Key、权限或代理。"
    : !savedComplete
      ? "供应商配置还不完整：补全 Base URL 与模型并保存后再验证。"
      : !keyConfigured
        ? "保存 API Key 后即可验证连接。"
        : "验证使用已保存的供应商配置与系统钥匙串里的 Key。";

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
          消息类型
          <select
            data-hud-hitbox
            data-provider-message-type
            value={draft.messageType}
            onChange={(event) =>
              onChange({ ...draft, messageType: event.target.value as MessageType })
            }
            aria-label="消息类型"
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

      <div className="mt-2 flex flex-col gap-1.5">
        {keyStatus ? (
          <CredentialRow
            status={keyStatus}
            label={keyLabel}
            submitOnEnter
            draft={keyDraft}
            busy={keyBusy}
            onDraftChange={onKeyDraftChange}
            onSave={onSaveKey}
            onClear={onClearKey}
          />
        ) : (
          <div
            data-credential-row="classifierApiKey"
            className="rounded border border-dashed border-stone-800/80 bg-stone-950/40 px-2.5 py-2 font-mono text-[10px] text-stone-500"
          >
            {keyLoading ? "正在读取钥匙串状态…" : "凭证状态不可用（核心库未启动）"}
          </div>
        )}
        <p className="font-mono text-[9px] leading-relaxed text-stone-600">
          API Key 只存系统钥匙串固定条目{" "}
          <span className="text-stone-500">classifier.api_key</span>
          ；换供应商需重填 Key。
        </p>
      </div>

      <div className="mt-2 flex flex-wrap items-center justify-end gap-1.5 border-t border-stone-800/70 pt-2">
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

      {draftDirty && (
        <p
          data-provider-dirty
          className="mt-2 rounded border border-amber-500/40 bg-amber-500/5 px-2 py-1 font-mono text-[10px] leading-relaxed text-amber-200"
        >
          ⚠ 草稿有未保存的改动：先点「保存供应商配置」，验证与分类才会使用新配置。
        </p>
      )}

      <ValidationLine validation={validation} idleHint={validationHint} />
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
  onValidate: () => void;
}

function CredentialGroup({
  title,
  hint,
  rows,
  validation,
  validating,
  canValidate,
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
      <ValidationLine validation={validation} />
    </div>
  );
}

interface CredentialRowProps {
  status: CredentialStatus;
  /** 界面展示名覆盖（分类器 Key 随活动供应商改名；account 仍固定）。 */
  label?: string;
  /** 回车即保存（仅分类器供应商卡片内的 Key 行启用；其余行靠按钮）。 */
  submitOnEnter?: boolean;
  draft: string;
  busy: boolean;
  onDraftChange: (value: string) => void;
  onSave: () => void;
  onClear: () => void;
}

function CredentialRow({
  status,
  label,
  submitOnEnter = false,
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
          onKeyDown={(event) => {
            // 分类器 Key 行支持回车保存；输入法组合中的回车不提交，忙碌/空值直接忽略。
            if (!submitOnEnter) return;
            if (event.nativeEvent.isComposing) return;
            if (event.key !== "Enter" || busy || !draft.trim()) return;
            event.preventDefault();
            onSave();
          }}
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