import { Fragment } from "react";
import type { KeyHint } from "../hud-model";

interface HudKeyHintBarProps {
  hints: readonly KeyHint[];
}

/**
 * Placeholder for the context-aware bottom hint bar. T14 replaces the static
 * contents with per-focus-zone hints; the position and visual weight are already
 * the accepted #7 HUD layer's.
 */
export function HudKeyHintBar({ hints }: HudKeyHintBarProps) {
  return (
    <div className="pointer-events-none absolute inset-x-0 bottom-4 z-20 flex justify-center px-4">
      <div
        data-hud-key-hints=""
        data-hud-hitbox
        className="pixel-box-sm pointer-events-auto flex flex-wrap items-center gap-x-3 gap-y-1 rounded-full border border-stone-700/70 bg-stone-950/70 px-4 py-1.5 font-mono text-[10px] text-stone-300 backdrop-blur-md"
      >
        {hints.map((hint, index) => (
          <Fragment key={hint.keys.join("+") + hint.label}>
            {index > 0 && (
              <span aria-hidden="true" className="text-stone-600">
                ·
              </span>
            )}
            <span className="flex items-center gap-1">
              {hint.keys.map((key) => (
                <kbd
                  key={key}
                  className="rounded border border-stone-600/80 bg-stone-900/80 px-1.5 py-0.5 text-[10px] text-stone-200"
                >
                  {key}
                </kbd>
              ))}
              <span>{hint.label}</span>
            </span>
          </Fragment>
        ))}
      </div>
    </div>
  );
}