/**
 * Framework-free HUD interaction vocabulary.
 *
 * The React layer may only read this state through selectors and change it through
 * commands; no HUD behavior lives in components.
 */

/** v1 HUD view states. T01 only exposes the default state (S0); later tickets add S1–S3. */
export type HudView = "default";

/**
 * Visibility phase driven by the OS shell (#26). `visible` means the shell has
 * summoned the HUD (including the slide-in animation, which must not block input);
 * `dismissing` starts when the shell begins sliding the HUD off screen, and the
 * keyboard router must swallow every key until the next summon.
 */
export type HudPhase = "visible" | "dismissing";

export type BallKind = "search" | "l3-filter" | "l1" | "quick-add" | "settings";

/** Effective-taxonomy L2 node, mirrored from `aether-core::navigation` (#27). */
export interface TaxonomyL2 {
  readonly id: string;
  readonly name: string;
  readonly selfBuilt: boolean;
}

export interface TaxonomyL1 {
  readonly id: string;
  readonly name: string;
  readonly icon: string | null;
  readonly l2s: readonly TaxonomyL2[];
}

export interface TaxonomyFormatTag {
  readonly id: string;
  readonly name: string;
  readonly color: string | null;
}

/** Effective taxonomy projection consumed by the HUD (ball column + L2 panel + L3 bar). */
export interface HudNavigation {
  readonly baseVersion: string;
  readonly effectiveVersion: string;
  readonly l1s: readonly TaxonomyL1[];
  readonly formatTags: readonly TaxonomyFormatTag[];
}

/** One slot in the pixel-ball column. L1 slots come from the effective taxonomy. */
export interface BallSlot {
  readonly id: string;
  readonly kind: BallKind;
  readonly label: string;
  readonly placeholder: boolean;
  readonly icon?: string | null;
  readonly l1Id?: string;
}

export interface SearchState {
  /** When true the search bar is expanded; when false the search orb is visible instead. */
  readonly expanded: boolean;
  readonly query: string;
  /** Monotonic token consumed by the renderer to move DOM focus into the search input. */
  readonly focusRequest: number;
}

/** 面板为 S2 的导航态：选中 L1 后原地展开；activeL2Id = null 表示「全部子类」。 */
export interface L2PanelState {
  readonly l1Id: string;
  readonly activeL2Id: string | null;
}

/**
 * L3 标签条本票只落结构与顺序（筛选行为在 #34）。
 *
 * `expanded` 只表示标签条的展开可见性（与 L2 面板同屏互斥），不是筛选选中态；
 * #34 的标签选中/筛选状态是独立模型，不受折叠标签条影响。
 */
export interface L3BarState {
  readonly expanded: boolean;
}

/**
 * 快速录入弹窗（#28）：seed = 打开时带入的搜索框内容（tauri#15436：先聚焦空
 * textarea，下一帧再预填）；submitting 防止重复入库。
 */
export interface QuickInputState {
  readonly seed: string;
  readonly submitting: boolean;
}

/** 设置球展开入口行（#24 §9 / #28）：未实现项占位但不可触发。 */
export type SettingsRowItemId =
  | "inbox"
  | "all"
  | "tags"
  | "taxonomy"
  | "archive"
  | "trash"
  | "settings"
  | "about";

export interface SettingsRowItem {
  readonly id: SettingsRowItemId;
  readonly label: string;
  readonly enabled: boolean;
}

export interface SettingsRowState {
  readonly expanded: boolean;
}

/**
 * 设置弹层的十个分区（#24 §8 / #29）：左侧导航顺序即本枚举顺序。
 * 本票只实现「凭证」，其余分区显示占位并由后续票接入。
 */
export type SettingsSectionId =
  | "general"
  | "appearance"
  | "classifier"
  | "network"
  | "credentials"
  | "ingestion"
  | "tags"
  | "taxonomy"
  | "data"
  | "about";

/** 凭证种类（与 aether-core::credentials::CredentialKind 同形）。 */
export type CredentialKind =
  | "classifierApiKey"
  | "jinaApiKey"
  | "feishuAppId"
  | "feishuAppSecret";

/** 三态凭证状态；界面只显示这三个字面量，永不显示密钥。 */
export type CredentialState = "configured" | "not_configured" | "unavailable";

/** 一项凭证的状态（与 aether-core::credentials::CredentialStatus 同形）。 */
export interface CredentialStatus {
  readonly kind: CredentialKind;
  readonly account: string;
  readonly label: string;
  readonly state: CredentialState;
  /** keychain://aetherhud/<account> 引用；只用于展示，不含密钥。 */
  readonly reference: string;
}

/** LLM 供应商预设（与 aether-core::provider::ProviderPreset 同形）。 */
export type ProviderPreset = "bailian" | "openai" | "deepseek" | "custom";

/** 消息格式 / API 协议（与 aether-core::provider::MessageType 同形）。 */
export type MessageType = "chat_completions" | "responses";

/** 活动供应商配置（与 aether-core::provider::ProviderProfile 同形，camelCase）。 */
export interface ProviderProfile {
  readonly preset: ProviderPreset;
  readonly baseUrl: string;
  readonly model: string;
  readonly messageType: MessageType;
}

/** 可点「验证」的三个连通性目标。 */
export type ValidationTarget = "classifier" | "jina" | "feishu";

/** 一次验证的结果；detail 已由核心库脱敏。 */
export interface CredentialValidationView {
  readonly target: ValidationTarget;
  readonly ok: boolean;
  readonly detail: string;
}

/** 凭证区的忙碌键：单项保存/清除用 CredentialKind，验证用 ValidationTarget。 */
export type CredentialBusyKey = CredentialKind | ValidationTarget;

/** 凭证区顶部的一次性反馈（保存/清除/读取失败）。 */
export interface CredentialMessage {
  readonly tone: "ok" | "error";
  readonly text: string;
}

/** 凭证区数据：供应商配置 + 状态 + 忙碌 + 验证结果 + 一次性反馈。 */
export interface CredentialsState {
  /** 活动供应商配置；null = 尚未从壳读到（打开设置时由平台层加载）。 */
  readonly profile: ProviderProfile | null;
  /** 保存供应商配置的忙碌标志（与凭证单项 busy 分开，互不阻塞）。 */
  readonly profileBusy: boolean;
  /** 供应商配置的一次性反馈（保存成功/校验失败/读取失败）。 */
  readonly profileMessage: CredentialMessage | null;
  readonly statuses: readonly CredentialStatus[];
  readonly loading: boolean;
  readonly busy: CredentialBusyKey | null;
  readonly validations: readonly CredentialValidationView[];
  readonly message: CredentialMessage | null;
}

/** 设置弹层（S0–S3 之外的浮层，和快速录入/收件箱同级）。 */
export interface SettingsState {
  readonly open: boolean;
  readonly section: SettingsSectionId;
  readonly credentials: CredentialsState;
}

/** 收件箱 keyset 游标（与 aether-core::inbox::InboxCursor 同形）。 */
export interface InboxCursor {
  readonly createdAt: number;
  readonly id: number;
}

export type PipelineStatus = "pending" | "processing" | "classified" | "failed";

/** 收件箱行（与 aether-core::inbox::InboxEntry 同形，camelCase）。 */
export interface InboxEntry {
  readonly id: number;
  readonly uid: string;
  readonly title: string;
  readonly url: string | null;
  readonly mediaType: string;
  readonly pipelineStatus: PipelineStatus;
  readonly failureReason: string | null;
  readonly createdAt: number;
  readonly lastSeenAt: number;
  readonly l1Id: string;
  readonly l2Id: string;
}

export interface InboxPage {
  readonly entries: readonly InboxEntry[];
  readonly nextCursor: InboxCursor | null;
}

/** 收件箱列表视图状态；revision 由 hud://inbox-updated 递增，驱动重载第一页。 */
export interface InboxState {
  readonly open: boolean;
  readonly entries: readonly InboxEntry[];
  readonly nextCursor: InboxCursor | null;
  readonly loading: boolean;
  readonly revision: number;
}

/** 启动期提示与入库反馈（aether-core 产出 code + detail；5 秒自动关闭由渲染层计时）。 */
export type HudNoticeCode =
  | "taxonomy_overlay_corrupt"
  | "core_unavailable"
  | "entry_ingested"
  | "entry_duplicate"
  | "ingest_failed"
  | "inbox_unavailable";

export interface HudNotice {
  readonly code: HudNoticeCode;
  readonly detail: string;
}

export interface KeyHint {
  readonly keys: readonly string[];
  readonly label: string;
}

export interface HudState {
  readonly view: HudView;
  readonly search: SearchState;
  /** 生效分类树投影；null = 壳尚未提供（浏览器调试/核心库启动失败）。 */
  readonly navigation: HudNavigation | null;
  readonly l2Panel: L2PanelState | null;
  readonly l3Bar: L3BarState;
  /** 快速录入弹窗；null = 未打开。 */
  readonly quickInput: QuickInputState | null;
  /** 设置球展开的入口行（收件箱/全部/…/关于）。 */
  readonly settingsRow: SettingsRowState;
  /** 设置弹层：十区导航 + 凭证区（#29）。 */
  readonly settings: SettingsState;
  /** 收件箱列表视图。 */
  readonly inbox: InboxState;
  readonly notice: HudNotice | null;
  readonly phase: HudPhase;
}

export type HudCommand =
  | { readonly type: "summon" }
  | { readonly type: "dismiss-start" }
  | { readonly type: "expand-search" }
  | { readonly type: "collapse-search" }
  | { readonly type: "set-query"; readonly query: string }
  | { readonly type: "set-navigation"; readonly navigation: HudNavigation }
  | { readonly type: "activate-l1"; readonly l1Id: string }
  | { readonly type: "activate-l2"; readonly l2Id: string | null }
  | { readonly type: "toggle-l3-bar" }
  | { readonly type: "collapse-l3-bar" }
  | { readonly type: "open-quick-input"; readonly seed: string }
  | { readonly type: "close-quick-input" }
  | { readonly type: "set-quick-input-submitting"; readonly submitting: boolean }
  | { readonly type: "toggle-settings-row" }
  | { readonly type: "collapse-settings-row" }
  | { readonly type: "open-settings"; readonly section?: SettingsSectionId }
  | { readonly type: "close-settings" }
  | { readonly type: "select-settings-section"; readonly section: SettingsSectionId }
  | { readonly type: "set-provider-profile"; readonly profile: ProviderProfile }
  | { readonly type: "set-provider-profile-busy"; readonly busy: boolean }
  | {
      readonly type: "set-provider-profile-message";
      readonly message: CredentialMessage | null;
    }
  | { readonly type: "set-credentials-loading"; readonly loading: boolean }
  | { readonly type: "set-credential-statuses"; readonly statuses: readonly CredentialStatus[] }
  | { readonly type: "update-credential-status"; readonly status: CredentialStatus }
  | { readonly type: "set-credential-busy"; readonly busy: CredentialBusyKey | null }
  | { readonly type: "set-credential-validation"; readonly validation: CredentialValidationView }
  | { readonly type: "set-credential-message"; readonly message: CredentialMessage | null }
  | { readonly type: "open-inbox" }
  | { readonly type: "close-inbox" }
  | { readonly type: "set-inbox-loading"; readonly loading: boolean }
  | { readonly type: "replace-inbox"; readonly page: InboxPage }
  | { readonly type: "append-inbox"; readonly page: InboxPage }
  | { readonly type: "invalidate-inbox" }
  | { readonly type: "show-notice"; readonly notice: HudNotice }
  | { readonly type: "dismiss-notice" };

export interface HudStore {
  getState(): HudState;
  dispatch(command: HudCommand): void;
  subscribe(listener: () => void): () => void;
}