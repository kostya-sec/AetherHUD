import { Layers, ListFilter, Plus, Search, Sliders } from "lucide-react";
import type { BallKind, BallSlot } from "../hud-model";
import { hudSurfaceStyle } from "./hudSurface";

interface HudBallColumnProps {
  slots: readonly BallSlot[];
  hudOpacity: number;
  onExpandSearch: () => void;
}

const BALL_ICONS: Record<BallKind, typeof Search> = {
  search: Search,
  "l3-filter": ListFilter,
  l1: Layers,
  "quick-add": Plus,
  settings: Sliders,
};

/**
 * Pixel-ball column from the #7 prototype. T01 renders placeholder slots for
 * everything except search; T03 replaces the L1 slots with the effective taxonomy.
 */
export function HudBallColumn({ slots, hudOpacity, onExpandSearch }: HudBallColumnProps) {
  return (
    <div className="flex flex-col items-center gap-3 py-1">
      {slots.map((slot) => {
        const Icon = BALL_ICONS[slot.kind];
        const interactive = slot.kind === "search";
        const title = interactive
          ? "展开全局搜索"
          : slot.placeholder
            ? `${slot.label}（后续接入）`
            : slot.label;

        return (
          <div
            key={slot.id}
            className="group relative"
            data-rain-floor={slot.kind === "settings" ? "" : undefined}
          >
            <button
              type="button"
              aria-label={slot.label}
              title={title}
              disabled={!interactive}
              tabIndex={interactive ? undefined : -1}
              onClick={interactive ? onExpandSearch : undefined}
              style={hudSurfaceStyle(hudOpacity)}
              className={`pixel-box hud-focus-ring flex h-12 w-12 items-center justify-center rounded-full border-2 transition-colors ${
                interactive
                  ? "cursor-pointer border-stone-700 text-stone-300 hover:border-amber-400 hover:text-white"
                  : "cursor-default border-stone-700/70 text-stone-400/80"
              }`}
            >
              <Icon className="h-5 w-5" aria-hidden="true" />
            </button>

            <div className="pixel-box-sm pointer-events-none absolute left-14 top-1/2 z-30 hidden -translate-y-1/2 whitespace-nowrap rounded border border-stone-600 bg-stone-950/95 px-2.5 py-1 font-mono text-xs text-stone-200 opacity-0 shadow-xl transition-opacity group-hover:opacity-100 md:block">
              {slot.label}
            </div>
          </div>
        );
      })}
    </div>
  );
}
