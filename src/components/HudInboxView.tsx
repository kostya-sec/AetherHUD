import {
  failureReasonLabel,
  inboxStatusBadge,
  mediaTypeLabel,
  type InboxEntry,
  type InboxState,
} from "../hud-model";
import { modalSurfaceStyle } from "./hudSurface";

interface HudInboxViewProps {
  inbox: InboxState;
  hudOpacity: number;
  onClose: () => void;
  onLoadMore: () => void;
}

/**
 * 收件箱列表视图（#28）：管线视图（待处理 / 处理中 / 失败），created_at 倒序、
 * 50 条/页 keyset 分页。新条目入库即出现在顶部，pending/processing 显示「分类中」
 * 徽标，failed 显示「失败」与失败原因；分类完成（classified）的条目离开收件箱。
 *
 * 全窗 backdrop 标 data-hud-hitbox：视图打开时窗口内点击不会穿透到桌面导致 HUD
 * 失焦收起；点击 backdrop 或 × 返回 HUD（Esc 走全局键位分层）。
 */
export function HudInboxView({
  inbox,
  hudOpacity,
  onClose,
  onLoadMore,
}: HudInboxViewProps) {
  return (
    <div
      data-hud-hitbox
      data-hud-inbox
      className="fixed inset-0 z-40 bg-stone-950/60 p-4 pt-8"
      onClick={onClose}
    >
      <section
        role="dialog"
        aria-modal="true"
        aria-label="收件箱"
        data-hud-hitbox
        onClick={(event) => event.stopPropagation()}
        className="pixel-box mx-auto flex h-full w-full max-w-3xl flex-col rounded-xl border border-stone-700/90 p-4"
        style={modalSurfaceStyle(hudOpacity)}
      >
        <header className="mb-3 flex items-center justify-between gap-3 border-b border-stone-800/80 pb-2">
          <div className="flex items-baseline gap-3">
            <h2 className="font-mono text-xs uppercase tracking-wider text-amber-300/90">
              收件箱
            </h2>
            <span className="font-mono text-[10px] text-stone-500">
              管线视图 · 待处理 / 处理中 / 失败
            </span>
          </div>
          <div className="flex items-center gap-2">
            <span className="font-mono text-[10px] text-stone-400">
              已加载 {inbox.entries.length} 条
            </span>
            <button
              type="button"
              data-hud-hitbox
              onClick={onClose}
              title="返回 HUD（Esc）"
              aria-label="关闭收件箱"
              className="pixel-box-sm hud-focus-ring flex h-5 w-5 items-center justify-center rounded border border-stone-700 text-stone-400 transition-colors hover:border-amber-400 hover:text-amber-300"
            >
              <span aria-hidden="true" className="text-xs leading-none">
                ×
              </span>
            </button>
          </div>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto pr-1">
          {inbox.entries.length === 0 ? (
            <InboxEmpty loading={inbox.loading} />
          ) : (
            <ul className="flex flex-col gap-1.5">
              {inbox.entries.map((entry) => (
                <InboxRow key={entry.id} entry={entry} />
              ))}
            </ul>
          )}
        </div>

        <footer className="mt-3 flex items-center justify-between gap-3 border-t border-stone-800/80 pt-2">
          <span className="font-mono text-[10px] text-stone-500">
            50 条/页 · created_at 倒序
          </span>
          {inbox.nextCursor ? (
            <button
              type="button"
              data-hud-hitbox
              onClick={onLoadMore}
              disabled={inbox.loading}
              className="pixel-box-sm hud-focus-ring rounded border border-stone-700 px-3 py-1 font-mono text-[11px] text-stone-200 transition-colors hover:border-amber-500/70 hover:text-white disabled:opacity-50"
            >
              {inbox.loading ? "加载中…" : "加载更多"}
            </button>
          ) : (
            <span className="font-mono text-[10px] text-stone-500">已到末尾</span>
          )}
        </footer>
      </section>
    </div>
  );
}

function InboxEmpty({ loading }: { loading: boolean }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 text-center">
      <p className="font-mono text-xs text-stone-400">
        {loading ? "正在读取收件箱…" : "收件箱是空的"}
      </p>
      {!loading && (
        <p className="font-mono text-[10px] text-stone-500">
          唤出 HUD 后按 Mod+Enter 录入第一条；条目会先以「分类中」出现在这里。
        </p>
      )}
    </div>
  );
}

function InboxRow({ entry }: { entry: InboxEntry }) {
  const badge = inboxStatusBadge(entry.pipelineStatus);
  return (
    <li
      data-inbox-entry={entry.id}
      className="rounded border border-stone-800/80 bg-stone-950/40 px-3 py-2"
    >
      <div className="flex items-center gap-2">
        <span className="min-w-0 flex-1 truncate font-mono text-sm text-stone-100">
          {entry.title}
        </span>
        {badge && (
          <span
            className={`shrink-0 rounded border px-1.5 py-0.5 font-mono text-[10px] ${
              badge.tone === "pending"
                ? "border-amber-500/60 text-amber-300"
                : "border-rose-500/60 text-rose-300"
            }`}
          >
            {badge.label}
          </span>
        )}
      </div>
      <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-0.5 font-mono text-[10px] text-stone-500">
        <span>{mediaTypeLabel(entry.mediaType)}</span>
        <span>{formatCreatedAt(entry.createdAt)}</span>
        {entry.failureReason && (
          <span className="text-rose-300/90">
            {failureReasonLabel(entry.failureReason)}
          </span>
        )}
        {entry.url && <span className="max-w-[22rem] truncate">{entry.url}</span>}
        {!entry.url && <span>纯文本</span>}
      </div>
    </li>
  );
}

// 展示时区设置（display.timezone，默认 Asia/Shanghai）在 #46 设置页接入；v1 先固定默认值。
const timeFormatter = new Intl.DateTimeFormat("zh-CN", {
  timeZone: "Asia/Shanghai",
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit",
  hour12: false,
});

function formatCreatedAt(ms: number): string {
  try {
    return timeFormatter.format(new Date(ms));
  } catch {
    return String(ms);
  }
}