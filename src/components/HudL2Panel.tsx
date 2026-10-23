import type { L2PanelView } from "../hud-model";
import { PANEL_LEFT_PX } from "./ballLayout";
import { hudSurfaceStyle } from "./hudSurface";

interface HudL2PanelProps {
  panel: L2PanelView;
  hudOpacity: number;
  /** 与所点 L1 球顶部平行对齐（ballTopOffset 计算，见 HudShell）。 */
  top: number;
  onSelectL2: (l2Id: string | null) => void;
}

/**
 * Cascading L2 panel (#17 §5.5 / #7 prototype): opens in place beside the clicked
 * L1 ball, top-aligned with it and filling the remaining HUD width. The first item is
 * the synthetic "全部子类" (only keeps the L1 filter); the rest come from the effective
 * taxonomy in order, self-built nodes last.
 */
export function HudL2Panel({ panel, hudOpacity, top, onSelectL2 }: HudL2PanelProps) {
  return (
    <div
      data-hud-l2-panel
      data-hud-hitbox
      className="pixel-box absolute z-20 rounded-xl border border-stone-700/80 p-2.5"
      style={{ ...hudSurfaceStyle(hudOpacity), top, left: PANEL_LEFT_PX, right: 0 }}
    >
      <div className="mb-2 flex items-center justify-between gap-3 px-1 font-mono text-[10px] uppercase tracking-wider text-amber-300/80">
        <span className="truncate">{panel.l1.name} · L2</span>
        <span className="shrink-0 text-stone-500">
          {Math.max(panel.items.length - 1, 0)} 个子类
        </span>
      </div>

      <div className="flex flex-wrap gap-1.5">
        {panel.items.map((item) => {
          const selected = item.isAllChildren
            ? panel.activeL2Id === null
            : panel.activeL2Id === item.id;
          return (
            <button
              key={item.id}
              type="button"
              data-hud-hitbox
              data-l2-id={item.id}
              aria-pressed={selected}
              onClick={() => onSelectL2(item.isAllChildren ? null : item.id)}
              className={`flex items-center gap-1.5 rounded border px-2.5 py-1 font-mono text-[11px] transition-colors ${
                selected
                  ? "border-amber-400 bg-amber-500/15 text-amber-100"
                  : "border-stone-700/80 text-stone-200 hover:border-amber-500/70 hover:text-white"
              }`}
              title={item.selfBuilt ? "自建分类" : undefined}
            >
              {item.isAllChildren ? (
                <LayersGlyph />
              ) : (
                <span
                  aria-hidden="true"
                  className={`h-1.5 w-1.5 rounded-full ${item.selfBuilt ? "bg-amber-400" : "bg-stone-600"}`}
                />
              )}
              <span className="whitespace-nowrap">{item.name}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}

/** Tiny inline glyph so the panel needs no extra icon dependency surface. */
function LayersGlyph() {
  return (
    <span aria-hidden="true" className="font-bold text-amber-400">
      ≡
    </span>
  );
}
