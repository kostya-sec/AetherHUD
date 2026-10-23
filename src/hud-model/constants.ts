import type {
  BallSlot,
  CredentialKind,
  KeyHint,
  MessageType,
  ProviderPreset,
  ProviderProfile,
  SettingsRowItem,
  SettingsSectionId,
  TaxonomyL1,
} from "./types";

/**
 * Ball column order (#17 §5.4 / PRD v2 §8):
 * search orb (only while the search bar is collapsed) → L3 filter → five L1 balls
 * (from the effective taxonomy) → quick input → settings.
 */
export const SEARCH_BALL: BallSlot = {
  id: "search",
  kind: "search",
  label: "全局搜索",
  placeholder: false,
};

export const L3_FILTER_BALL: BallSlot = {
  id: "l3-filter",
  kind: "l3-filter",
  label: "L3 标签筛选",
  placeholder: false,
};

export const QUICK_ADD_BALL: BallSlot = {
  id: "quick-add",
  kind: "quick-add",
  label: "快速录入",
  placeholder: false,
};

export const SETTINGS_BALL: BallSlot = {
  id: "settings",
  kind: "settings",
  label: "全局设置入口",
  placeholder: false,
};

/**
 * Fallback slots while the shell has not delivered the effective taxonomy
 * (plain-browser dev, or a failed core bootstrap). Labels stay neutral: the effective
 * taxonomy is the single runtime source of L1 names (ADR-0001), so the fallback must
 * not duplicate official names that would drift on a taxonomy upgrade.
 */
export const PLACEHOLDER_L1_SLOTS: readonly BallSlot[] = [
  "分类加载中",
  "分类加载中",
  "分类加载中",
  "分类加载中",
  "分类加载中",
].map((label, index) => ({
  id: `l1-placeholder-${index + 1}`,
  kind: "l1" as const,
  label,
  placeholder: true,
}));

export function taxonomyBall(l1: TaxonomyL1): BallSlot {
  return {
    id: l1.id,
    kind: "l1",
    label: l1.name,
    placeholder: false,
    icon: l1.icon,
    l1Id: l1.id,
  };
}

export const SEARCH_KEY_HINTS: readonly KeyHint[] = [
  { keys: ["Esc"], label: "收起" },
  { keys: ["Tab"], label: "换区" },
  { keys: ["Enter"], label: "进结果" },
  { keys: ["Mod", "Enter"], label: "入库" },
  { keys: ["?"], label: "键位表" },
];

export const BALL_KEY_HINTS: readonly KeyHint[] = [
  { keys: ["↑", "↓"], label: "选择" },
  { keys: ["Enter"], label: "进入" },
  { keys: ["Tab"], label: "换区" },
  { keys: ["Esc"], label: "收起" },
];

// #27 只有鼠标交互（键盘五区模型在 #38）：提示条只声明真正可用的键，
// 避免先展示 ↑↓/←→/Space 这些尚未接线的键位。
export const L2_KEY_HINTS: readonly KeyHint[] = [{ keys: ["Esc"], label: "收起" }];

export const L3_KEY_HINTS: readonly KeyHint[] = [{ keys: ["Esc"], label: "收起" }];

/**
 * 设置球入口行（#28）：本票只有收件箱可进入，键盘导航在 #38 接线，
 * 所以提示条只声明真正可用的 Esc。
 */
export const SETTINGS_ROW_KEY_HINTS: readonly KeyHint[] = [
  { keys: ["Esc"], label: "收起入口行" },
];

/**
 * 设置球展开入口行（#24 §9 / #28）：本票接通「收件箱」与「设置」，
 * 其余入口保持占位，等 #34/#35/#37/#46/#47 接入。
 */
export const SETTINGS_ROW_ITEMS: readonly SettingsRowItem[] = [
  { id: "inbox", label: "收件箱", enabled: true },
  { id: "all", label: "全部", enabled: false },
  { id: "tags", label: "标签管理", enabled: false },
  { id: "taxonomy", label: "分类管理", enabled: false },
  { id: "archive", label: "封存箱", enabled: false },
  { id: "trash", label: "回收站", enabled: false },
  { id: "settings", label: "设置", enabled: true },
  { id: "about", label: "关于", enabled: false },
];

/** 凭证区四行顺序（与 aether-core::credentials::CredentialKind::ALL 一致）。 */
export const CREDENTIAL_KINDS: readonly CredentialKind[] = [
  "classifierApiKey",
  "jinaApiKey",
  "feishuAppId",
  "feishuAppSecret",
];

/** 供应商预设的默认值（与 aether-core::provider::ProviderPreset 保持一致）。 */
export interface ProviderPresetMeta {
  readonly id: ProviderPreset;
  readonly label: string;
  /** 一句话说明，帮助用户选对预设。 */
  readonly description: string;
  readonly baseUrl: string;
  readonly model: string;
  readonly messageType: MessageType;
}

/**
 * 供应商预设（#45 前置）：百炼默认，可切 OpenAI / DeepSeek / 自定义 OpenAI 兼容。
 * 预设只填默认值；同一时间只有一套活动配置，换供应商要重填 Key。
 */
export const PROVIDER_PRESETS: readonly ProviderPresetMeta[] = [
  {
    id: "bailian",
    label: "阿里百炼（DashScope）",
    description: "v1 默认；OpenAI 兼容端点，qwen-flash 成本低、速度快",
    baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    model: "qwen-flash",
    messageType: "chat_completions",
  },
  {
    id: "openai",
    label: "OpenAI",
    description: "官方 api.openai.com；gpt-4o-mini 适合短文本分类",
    baseUrl: "https://api.openai.com/v1",
    model: "gpt-4o-mini",
    messageType: "chat_completions",
  },
  {
    id: "deepseek",
    label: "DeepSeek",
    description: "OpenAI 兼容端点；deepseek-chat",
    baseUrl: "https://api.deepseek.com/v1",
    model: "deepseek-chat",
    messageType: "chat_completions",
  },
  {
    id: "custom",
    label: "自定义（OpenAI 兼容）",
    description: "自填 Base URL 与模型 ID；支持 Chat Completions 或 Responses",
    baseUrl: "",
    model: "",
    messageType: "chat_completions",
  },
];

/** 按 id 取预设元数据；未知 id 回落到百炼。 */
export function providerPreset(preset: ProviderPreset): ProviderPresetMeta {
  return PROVIDER_PRESETS.find((entry) => entry.id === preset) ?? PROVIDER_PRESETS[0];
}

/** 预设的初始草稿（切换预设时填入面板；custom 为空，由用户自己填）。 */
export function providerProfileDraft(preset: ProviderPreset): ProviderProfile {
  const meta = providerPreset(preset);
  return {
    preset: meta.id,
    baseUrl: meta.baseUrl,
    model: meta.model,
    messageType: meta.messageType,
  };
}

export interface MessageTypeMeta {
  readonly id: MessageType;
  readonly label: string;
  readonly description: string;
}

/** 两种消息类型（Chat Completions / Responses）；Chat Completions 是默认。 */
export const MESSAGE_TYPE_OPTIONS: readonly MessageTypeMeta[] = [
  {
    id: "chat_completions",
    label: "Chat Completions",
    description: "POST {base}/chat/completions，body.messages[]",
  },
  {
    id: "responses",
    label: "Responses",
    description: "POST {base}/responses，body.input[]，读 output_text",
  },
];

/** 设置弹层默认分区：通用（十区的第一项）。 */
export const DEFAULT_SETTINGS_SECTION: SettingsSectionId = "general";

export interface SettingsSectionMeta {
  readonly id: SettingsSectionId;
  readonly label: string;
  /** false = 本票只显示占位，后续票接入。 */
  readonly implemented: boolean;
  /** 分区职责一句话（占位页也显示，避免"空壳"）。 */
  readonly summary: string;
}

/**
 * 设置弹层左侧十区（#24 §8 / #29）。
 *
 * 本票只有「凭证」落地；其余分区一律占位。实现后从这里把 implemented 改为 true，
 * 占位页自然消失——顺序也是左侧导航顺序。
 */
export const SETTINGS_SECTIONS: readonly SettingsSectionMeta[] = [
  {
    id: "general",
    label: "通用",
    implemented: false,
    summary: "开机自启、在 Dock 中显示、显示时区、退出（#46）",
  },
  {
    id: "appearance",
    label: "外观与音效",
    implemented: false,
    summary: "主题色、HUD 透明度、雨幕密度、音效开关与音量（#46）",
  },
  {
    id: "classifier",
    label: "分类与分类器",
    implemented: false,
    summary: "provider / base URL / model / prompt 版本（只读）/ 测试连接（#45）",
  },
  {
    id: "network",
    label: "网络",
    implemented: false,
    summary: "代理三档（跟随系统 / 自定义 / 直连）与代理凭据（#45）",
  },
  {
    id: "credentials",
    label: "凭证",
    implemented: true,
    summary:
      "分类器供应商（预设 / 消息类型 / Base URL / 模型 / API Key 同一卡片）、Jina Key、飞书 App ID / App Secret（#29 / #45）",
  },
  {
    id: "ingestion",
    label: "摄入与同步",
    implemented: false,
    summary: "webhook 路径与端口、飞书 Base/Table/字段映射/开关（#42 / #44）",
  },
  {
    id: "tags",
    label: "标签管理",
    implemented: false,
    summary: "L3 标签合并、别名、常驻与隐藏（#35）",
  },
  {
    id: "taxonomy",
    label: "分类管理",
    implemented: false,
    summary: "自建节点停用/恢复/改名、覆盖层导出（#41）",
  },
  {
    id: "data",
    label: "数据",
    implemented: false,
    summary: "最近备份、立即备份、打开数据文件夹、清空回收站（#43）",
  },
  {
    id: "about",
    label: "关于",
    implemented: false,
    summary: "版本、检查更新、忽略版本、安装前必读、许可（#47）",
  },
];

/** L3 标签条的**结构顺序**（#17 §5.6）：媒体 chip → 形态标签 → 特征标签。 */
export const L3_MEDIA_CHIPS: readonly { readonly id: string; readonly name: string }[] = [
  { id: "link", name: "网页" },
  { id: "video", name: "视频" },
  { id: "article", name: "文章" },
];

/** 启动提示条的展示时长（#13：不抢焦点的一次性提示，5 秒后自动关闭）。 */
export const NOTICE_DURATION_MS = 5000;