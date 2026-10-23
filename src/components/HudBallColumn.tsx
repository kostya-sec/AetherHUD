import {
  useCallback,
  useEffect,
  useRef,
  type DragEvent as ReactDragEvent,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
} from "react";
import {
  Inbox,
  Layers,
  ListFilter,
  Palette,
  Plus,
  Search,
  Sliders,
  Terminal,
  TrendingUp,
  Zap,
  type LucideIcon,
} from "lucide-react";
import type { BallKind, BallSlot } from "../hud-model";
import { startWindowDrag } from "../platform/windowDrag";
import { BALL_COLUMN_PADDING_PX, BALL_GAP_PX, BALL_SIZE_PX } from "./ballLayout";
import { hudSurfaceStyle } from "./hudSurface";

interface HudBallColumnProps {
  slots: readonly BallSlot[];
  hudOpacity: number;
  /** 当前展开的 L1（L2 面板打开时高亮对应球）。 */
  activeL1Id: string | null;
  /** L3 标签条展开时高亮 L3 筛选球。 */
  l3Expanded: boolean;
  onActivateSearch: () => void;
  onActivateL1: (l1Id: string) => void;
  onActivateL3: () => void;
}

const KIND_ICONS: Record<BallKind, LucideIcon> = {
  search: Search,
  "l3-filter": ListFilter,
  l1: Layers,
  "quick-add": Plus,
  settings: Sliders,
};

/** taxonomy.json 的 icon 字段 → lucide 图标；未知名称回退到 L1 通用图标。 */
const TAXONOMY_ICONS: Record<string, LucideIcon> = {
  inbox: Inbox,
  palette: Palette,
  zap: Zap,
  terminal: Terminal,
  "trending-up": TrendingUp,
};

function iconFor(slot: BallSlot): LucideIcon {
  if (slot.kind === "l1" && slot.icon) {
    return TAXONOMY_ICONS[slot.icon] ?? KIND_ICONS.l1;
  }
  return KIND_ICONS[slot.kind];
}

/**
 * Pointer travel (CSS px) that turns a press on the column into a native window
 * drag. Below it the gesture stays a click, so balls still expand.
 */
const DRAG_THRESHOLD_PX = 4;

/**
 * Pixel-ball column (#17 §5.4): L3 filter → five L1 balls (effective taxonomy) →
 * quick input → settings, with the search ball only while the search box is collapsed.
 * Clicking an L1 ball expands its L2 panel in place; clicking the L3 ball expands the
 * tag bar. The whole column doubles as a window drag handle.
 */
export function HudBallColumn({
  slots,
  hudOpacity,
  activeL1Id,
  l3Expanded,
  onActivateSearch,
  onActivateL1,
  onActivateL3,
}: HudBallColumnProps) {
  const suppressClickRef = useRef(false);
  const endPressRef = useRef<(() => void) | null>(null);

  const endPress = useCallback(() => {
    const stopTracking = endPressRef.current;
    endPressRef.current = null;
    stopTracking?.();
  }, []);

  // An unmount mid-press (ESC dismiss) must not leak window listeners.
  useEffect(() => endPress, [endPress]);

  const handlePointerDown = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>) => {
      // Every new press starts a fresh click judgement; the previous drag is over.
      suppressClickRef.current = false;
      if (event.button !== 0 || !event.isPrimary) return;

      const { pointerId, clientX: startX, clientY: startY } = event;
      endPress(); // Defensive: drop listeners from a press that never ended cleanly.

      const handlePointerMove = (moveEvent: PointerEvent) => {
        if (moveEvent.pointerId !== pointerId) return;
        // The release may have happened outside the window (no pointerup here).
        if ((moveEvent.buttons & 1) === 0) {
          endPress();
          return;
        }
        const dx = moveEvent.clientX - startX;
        const dy = moveEvent.clientY - startY;
        if (Math.hypot(dx, dy) < DRAG_THRESHOLD_PX) return;
        // Threshold crossed: this gesture is a window move, never a click.
        suppressClickRef.current = true;
        endPress();
        startWindowDrag();
      };

      const handlePointerEnd = (endEvent: PointerEvent) => {
        if (endEvent.pointerId !== pointerId) return;
        endPress();
      };

      window.addEventListener("pointermove", handlePointerMove);
      window.addEventListener("pointerup", handlePointerEnd);
      window.addEventListener("pointercancel", handlePointerEnd);
      endPressRef.current = () => {
        window.removeEventListener("pointermove", handlePointerMove);
        window.removeEventListener("pointerup", handlePointerEnd);
        window.removeEventListener("pointercancel", handlePointerEnd);
      };
    },
    [endPress],
  );

  const handleClickCapture = useCallback((event: ReactMouseEvent<HTMLDivElement>) => {
    if (!suppressClickRef.current) return;
    // The release ending a drag must not reach a ball's onClick.
    suppressClickRef.current = false;
    event.preventDefault();
    event.stopPropagation();
  }, []);

  const handleDragStart = useCallback((event: ReactDragEvent<HTMLDivElement>) => {
    // A native HTML5 drag would steal the gesture from the window move loop.
    event.preventDefault();
  }, []);

  return (
    <div
      className="flex shrink-0 cursor-grab select-none flex-col items-center"
      style={{
        gap: BALL_GAP_PX,
        paddingTop: BALL_COLUMN_PADDING_PX,
        paddingBottom: BALL_COLUMN_PADDING_PX,
      }}
      onPointerDown={handlePointerDown}
      onClickCapture={handleClickCapture}
      onDragStart={handleDragStart}
    >
      {slots.map((slot) => {
        const Icon = iconFor(slot);
        const interactive =
          !slot.placeholder &&
          (slot.kind === "search" || slot.kind === "l1" || slot.kind === "l3-filter");
        const selected =
          (slot.kind === "l1" && slot.l1Id !== undefined && slot.l1Id === activeL1Id) ||
          (slot.kind === "l3-filter" && l3Expanded);
        const title = slot.placeholder ? `${slot.label}（后续接入）` : slot.label;

        const handleClick = () => {
          if (!interactive) return;
          if (slot.kind === "search") onActivateSearch();
          else if (slot.kind === "l3-filter") onActivateL3();
          else if (slot.kind === "l1" && slot.l1Id) onActivateL1(slot.l1Id);
        };

        return (
          <div
            key={slot.id}
            className="group relative"
            data-rain-floor={slot.kind === "settings" ? "" : undefined}
          >
            <button
              type="button"
              data-hud-hitbox
              data-ball-kind={slot.kind}
              data-ball-id={slot.id}
              aria-label={slot.label}
              aria-pressed={interactive ? selected : undefined}
              title={title}
              // Placeholders stay inert for clicks but must receive pointer events,
              // otherwise the column cannot start a drag from them.
              aria-disabled={interactive ? undefined : true}
              tabIndex={interactive ? undefined : -1}
              onClick={interactive ? handleClick : undefined}
              style={{
                ...hudSurfaceStyle(hudOpacity),
                width: BALL_SIZE_PX,
                height: BALL_SIZE_PX,
              }}
              className={`pixel-box hud-focus-ring flex items-center justify-center rounded-full border-2 transition-colors active:cursor-grabbing ${
                selected
                  ? "border-amber-400 bg-amber-500/15 text-amber-200 shadow-[0_0_14px_rgba(245,158,11,0.35)]"
                  : interactive
                    ? "cursor-grab border-stone-700 text-stone-300 hover:border-amber-400 hover:text-white"
                    : "cursor-grab border-stone-700/70 text-stone-400/80"
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
