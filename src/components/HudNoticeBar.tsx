import { useEffect } from "react";
import { NOTICE_DURATION_MS, type HudNotice } from "../hud-model";

interface HudNoticeBarProps {
  notice: HudNotice;
  onDismiss: () => void;
}

/**
 * One-shot startup notice strip (#13 mechanism, consumed by #27 for a corrupt
 * taxonomy overlay). It must not steal focus; it disappears after 5 seconds.
 */
export function HudNoticeBar({ notice, onDismiss }: HudNoticeBarProps) {
  useEffect(() => {
    const timer = window.setTimeout(onDismiss, NOTICE_DURATION_MS);
    return () => window.clearTimeout(timer);
    // 重新挂载/换提示时重置计时；onDismiss 是渲染层包装，不参与依赖。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [notice]);

  return (
    <div
      role="status"
      aria-live="polite"
      data-hud-notice
      className="pixel-box-sm w-full rounded-lg border border-amber-500/60 bg-stone-950/90 px-3 py-2 font-mono text-[11px] text-amber-100 shadow-[0_0_18px_rgba(245,158,11,0.18)]"
    >
      {notice.detail}
    </div>
  );
}
