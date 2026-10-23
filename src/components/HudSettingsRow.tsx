import type { SettingsRowItemId, SettingsRowView } from "../hud-model";
import { BALL_COLUMN_PADDING_PX, PANEL_LEFT_PX } from "./ballLayout";
import { hudSurfaceStyle } from "./hudSurface";

interface HudSettingsRowProps {
  view: SettingsRowView;
  hudOpacity: number;
  onSelect: (id: SettingsRowItemId) => void;
}

/**
 * 设置球展开的全局入口行（#24 §9 / #28）：收件箱、全部、标签管理、分类管理、
 * 封存箱、回收站、设置、关于。本票只接通「收件箱」，其余保持占位且不可误触发
 * （aria-disabled + 无 onClick），等 #29/#34/#35/#37/#46/#47 接入。
 *
 * 原地展开：左边缘贴球列右侧、底边与设置球底部对齐（设置球是球列最后一颗），
 * 所以入口行向上铺开，不会顶到窗口底部的键位提示条。
 */
export function HudSettingsRow({ view, hudOpacity, onSelect }: HudSettingsRowProps) {
  return (
    <div
      data-hud-settings-row
      data-hud-hitbox
      className="pixel-box absolute z-20 rounded-xl border border-stone-700/80 p-2.5"
      style={{
        ...hudSurfaceStyle(hudOpacity),
        left: PANEL_LEFT_PX,
        bottom: BALL_COLUMN_PADDING_PX,
        right: 0,
      }}
    >
      <div className="mb-2 flex items-center justify-between gap-3 px-1 font-mono text-[10px] uppercase tracking-wider text-amber-300/80">
        <span>全局入口</span>
        <span className="text-stone-500 normal-case tracking-normal">
          收件箱已接入 · 其余占位
        </span>
      </div>

      <div className="flex flex-wrap gap-1.5">
        {view.items.map((item) => (
          <button
            key={item.id}
            type="button"
            data-hud-hitbox
            data-settings-item={item.id}
            aria-disabled={item.enabled ? undefined : true}
            tabIndex={item.enabled ? undefined : -1}
            title={item.enabled ? item.label : `${item.label}（后续接入）`}
            onClick={item.enabled ? () => onSelect(item.id) : undefined}
            className={`rounded border px-2.5 py-1 font-mono text-[11px] transition-colors ${
              item.enabled
                ? "cursor-pointer border-stone-700/80 text-stone-200 hover:border-amber-500/70 hover:text-white"
                : "cursor-default border-stone-800/70 text-stone-500/80"
            }`}
          >
            {item.label}
          </button>
        ))}
      </div>
    </div>
  );
}
