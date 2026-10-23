import { useEffect, useRef, useState } from "react";
import { isIngestShortcut, type QuickInputState } from "../hud-model";
import { modalSurfaceStyle } from "./hudSurface";

interface HudQuickInputModalProps {
  quickInput: QuickInputState;
  hudOpacity: number;
  onSubmit: (text: string) => void;
  onClose: () => void;
}

/**
 * 快速录入弹窗（#28 / #17 §5.8）：Mod+Enter 入库、Esc 关闭、Tab 原生顺序。
 *
 * 整窗铺一层 backdrop 并标 data-hud-hitbox：弹窗内（以及窗口内空白处）的点击都落在
 * webview 里，不会穿透到桌面导致 HUD 失焦收起、打断录入。
 *
 * 预填规避（tauri#15436）：Windows 上预填内容的 textarea 首次聚焦可能冻结 TSF
 * 输入法，所以先聚焦空输入，下一帧再写入带入的搜索词。
 */
export function HudQuickInputModal({
  quickInput,
  hudOpacity,
  onSubmit,
  onClose,
}: HudQuickInputModalProps) {
  const [text, setText] = useState("");
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const seed = quickInput.seed;
  const submitting = quickInput.submitting;

  useEffect(() => {
    textareaRef.current?.focus();
    const timer = window.setTimeout(() => setText(seed), 0);
    return () => window.clearTimeout(timer);
  }, [seed]);

  const trimmed = text.trim();
  const looksLikeUrl = /^https?:\/\//i.test(trimmed);

  const submit = () => {
    if (submitting || !trimmed) return;
    onSubmit(text);
  };

  // Mod+Enter 在弹窗容器上监听（冒泡），不依赖焦点停在 textarea：#17 §5.8 要求
  // 弹窗内任意焦点位置都能入库；preventDefault 同时拦掉焦点在按钮上时 Enter 的点击。
  const handleKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (!isIngestShortcut(event.nativeEvent)) return;
    event.preventDefault();
    submit();
  };

  return (
    <div
      data-hud-hitbox
      className="fixed inset-0 z-40 flex items-center justify-center bg-stone-950/55 p-4"
      // 点击弹窗外空白处不关闭（避免误丢已输入内容），只把焦点送回输入框。
      onMouseDown={(event) => {
        event.preventDefault();
        textareaRef.current?.focus();
      }}
      onKeyDown={handleKeyDown}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="快速录入"
        data-hud-hitbox
        data-hud-quick-input
        className="pixel-box w-full max-w-xl rounded-xl border border-stone-700/90 p-4"
        style={modalSurfaceStyle(hudOpacity)}
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="mb-3 flex items-center justify-between gap-3">
          <h2 className="font-mono text-xs uppercase tracking-wider text-amber-300/90">
            快速录入
          </h2>
          <span className="font-mono text-[10px] text-stone-400">
            {looksLikeUrl ? "链接：先抓取正文再分类" : "纯文本：直接进入分类"}
          </span>
        </div>

        <textarea
          ref={textareaRef}
          value={text}
          rows={6}
          onChange={(event) => setText(event.target.value)}
          placeholder="粘贴 URL 或随手记…"
          aria-label="录入内容"
          className="w-full resize-none rounded-lg border border-stone-700/80 bg-stone-950/60 px-3 py-2 font-mono text-sm leading-relaxed text-stone-100 outline-none placeholder:text-stone-500 focus:border-amber-500/80"
        />

        <div className="mt-3 flex items-center justify-between gap-3">
          <span className="font-mono text-[10px] text-stone-500">
            {submitting ? "正在入库…" : "Mod+Enter 入库 · Esc 关闭"}
          </span>
          <div className="flex items-center gap-2">
            <button
              type="button"
              data-hud-hitbox
              onClick={onClose}
              disabled={submitting}
              className="pixel-box-sm hud-focus-ring rounded border border-stone-700 px-3 py-1 font-mono text-[11px] text-stone-300 transition-colors hover:border-stone-500 disabled:opacity-50"
            >
              取消
            </button>
            <button
              type="button"
              data-hud-hitbox
              onClick={submit}
              disabled={submitting || !trimmed}
              className="pixel-box-sm hud-focus-ring rounded border border-amber-500/80 bg-amber-500/15 px-3 py-1 font-mono text-[11px] text-amber-100 transition-colors hover:bg-amber-500/25 disabled:opacity-50"
            >
              入库
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}