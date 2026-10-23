import type { L3MediaChip, TaxonomyFormatTag } from "../hud-model";
import { PANEL_LEFT_PX } from "./ballLayout";
import { hudSurfaceStyle } from "./hudSurface";

interface HudL3BarProps {
  mediaChips: readonly L3MediaChip[];
  formatTags: readonly TaxonomyFormatTag[];
  hudOpacity: number;
  /**
   * 与 L3 筛选球顶部平行对齐的 top（按当前可见球列的槽位索引实时计算，
   * 见 HudShell 的 ballTopOffsetFor）。
   */
  top: number;
  onCollapse: () => void;
}

/**
 * L3 tag bar (#17 §5.6). This ticket only renders the structure and order
 * (media chips → format tags → feature tags); selecting pills filters the card
 * stream in #34. Keeping the visual layer here means the interaction model stays pure.
 *
 * Layout mirrors the L2 panel: the strip expands in place beside the ball column —
 * left edge just right of the balls, top parallel to the L3 filter ball, right edge
 * flush with the HUD content edge. It is absolutely positioned so it never pushes
 * the ball column down (the previous full-width block above the column is gone).
 */
export function HudL3Bar({ mediaChips, formatTags, hudOpacity, top, onCollapse }: HudL3BarProps) {
  return (
    <div
      data-hud-l3-bar
      data-hud-hitbox
      className="pixel-box absolute z-20 rounded-xl border border-stone-700/80 p-2.5"
      style={{ ...hudSurfaceStyle(hudOpacity), top, left: PANEL_LEFT_PX, right: 0 }}
    >
      <div className="mb-2 flex items-center justify-between gap-3 px-1">
        <div className="flex items-center gap-2 font-mono text-[10px] uppercase tracking-wider text-amber-300/80">
          <span>L3 标签筛选</span>
          <span className="text-stone-500 normal-case tracking-normal">结构与顺序</span>
        </div>
        <button
          type="button"
          data-hud-hitbox
          onClick={onCollapse}
          title="折叠 L3 标签条（恢复为球）"
          aria-label="折叠 L3 标签条"
          className="pixel-box-sm hud-focus-ring flex h-5 w-5 shrink-0 items-center justify-center rounded border border-stone-700 text-stone-400 transition-colors hover:border-amber-400 hover:text-amber-300"
        >
          <span aria-hidden="true" className="text-xs leading-none">
            ×
          </span>
        </button>
      </div>

      <div className="flex flex-wrap items-center gap-1.5">
        <span className="mr-0.5 shrink-0 font-mono text-[10px] text-amber-300/80">媒体</span>
        {mediaChips.map((chip) => (
          <span
            key={chip.id}
            title="筛选行为在 #34 接入"
            className="rounded border border-stone-700/80 px-2 py-0.5 font-mono text-[11px] text-stone-300"
          >
            {chip.name}
          </span>
        ))}

        <span className="mx-1 hidden h-3 w-px bg-stone-700 sm:block" />
        <span className="mr-0.5 shrink-0 font-mono text-[10px] text-amber-300/80">形态</span>
        {formatTags.map((tag) => (
          <span
            key={tag.id}
            title="筛选行为在 #34 接入"
            className="flex items-center gap-1.5 rounded border border-stone-700/80 px-2 py-0.5 font-mono text-[11px] text-stone-200"
          >
            <span
              aria-hidden="true"
              className="h-1.5 w-1.5 rounded-full"
              style={{ backgroundColor: tag.color ?? "#a8a29e" }}
            />
            {tag.name}
          </span>
        ))}
        {formatTags.length === 0 && (
          <span className="font-mono text-[11px] text-stone-500">形态标签未加载</span>
        )}

        <span className="mx-1 hidden h-3 w-px bg-stone-700 sm:block" />
        <span className="mr-0.5 shrink-0 font-mono text-[10px] text-amber-300/80">特征</span>
        <span className="font-mono text-[11px] text-stone-500">
          待标签治理接入（#34）
        </span>
      </div>
    </div>
  );
}
