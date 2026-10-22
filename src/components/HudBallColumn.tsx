import {
  useCallback,
  useEffect,
  useRef,
  type DragEvent as ReactDragEvent,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
} from "react";
import { Layers, ListFilter, Plus, Search, Sliders } from "lucide-react";
import type { BallKind, BallSlot } from "../hud-model";
import { startWindowDrag } from "../platform/windowDrag";
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
 * Pointer travel (CSS px) that turns a press on the column into a native window
 * drag. Below it the gesture stays a click, so the search ball still expands.
 */
const DRAG_THRESHOLD_PX = 4;

/**
 * Pixel-ball column from the #7 prototype. T01 renders placeholder slots for
 * everything except search; T03 replaces the L1 slots with the effective taxonomy.
 *
 * The whole column doubles as a window drag handle: the press is tracked on
 * `window` so the gesture survives the pointer leaving the ball while moving, and
 * a drag that crossed the threshold swallows the click it would otherwise fire on
 * the search ball.
 */
export function HudBallColumn({ slots, hudOpacity, onExpandSearch }: HudBallColumnProps) {
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
    // The release ending a drag must not reach the search ball's onClick.
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
      className="flex cursor-grab select-none flex-col items-center gap-3 py-1"
      onPointerDown={handlePointerDown}
      onClickCapture={handleClickCapture}
      onDragStart={handleDragStart}
    >
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
              data-hud-hitbox
              aria-label={slot.label}
              title={title}
              // Placeholders stay inert for clicks but must receive pointer events,
              // otherwise the column cannot start a drag from them.
              aria-disabled={interactive ? undefined : true}
              tabIndex={interactive ? undefined : -1}
              onClick={interactive ? onExpandSearch : undefined}
              style={hudSurfaceStyle(hudOpacity)}
              className={`pixel-box hud-focus-ring flex h-12 w-12 items-center justify-center rounded-full border-2 transition-colors active:cursor-grabbing ${
                interactive
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