import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef } from "react";
import {
  selectKeyHints,
  selectSearchExpanded,
  selectSearchFocusRequest,
  selectSearchQuery,
  selectVisibleBallSlots,
  type HudCommand,
  type HudState,
} from "../hud-model";
import { HudBallColumn } from "./HudBallColumn";
import { HudKeyHintBar } from "./HudKeyHintBar";
import { HudSearchBar } from "./HudSearchBar";
import { RainBackground } from "./RainBackground";

/** Prototype defaults from #7; the settings layer owns these later (T22). */
const HUD_OPACITY = 0.65;
const RAIN_DENSITY = 0.8;

interface HudShellProps {
  state: HudState;
  dispatch: (command: HudCommand) => void;
}

/**
 * Pure presentation shell: every piece of HUD behavior comes from the framework-free
 * hud-model through selectors and commands.
 */
export function HudShell({ state, dispatch }: HudShellProps) {
  const searchInputRef = useRef<HTMLInputElement>(null);
  const searchExpanded = selectSearchExpanded(state);
  const focusRequest = selectSearchFocusRequest(state);

  // 唤出即展开并聚焦搜索框（#16/#17）；focusRequest 是模型给渲染层的单调令牌。
  useEffect(() => {
    if (!searchExpanded) return;
    const frame = requestAnimationFrame(() => searchInputRef.current?.focus());
    return () => cancelAnimationFrame(frame);
  }, [searchExpanded, focusRequest]);

  // The Tauri window starts hidden to avoid the transparent-window white flash;
  // the shell reveals it after the first render. In a plain browser this is a no-op.
  useEffect(() => {
    const frame = requestAnimationFrame(() => {
      try {
        void invoke("frontend_ready").catch(() => undefined);
      } catch {
        // Plain-browser dev has no Tauri IPC bridge.
      }
    });
    return () => cancelAnimationFrame(frame);
  }, []);

  return (
    <div
      className="relative h-screen w-screen overflow-hidden bg-transparent font-sans text-stone-100 selection:bg-amber-500/30"
      style={{
        textShadow: "0 1px 3px rgba(0,0,0,0.9), 0 0 6px rgba(0,0,0,0.45)",
      }}
    >
      <div className="relative mx-auto w-full max-w-5xl px-2 pt-4">
        <RainBackground opacity={RAIN_DENSITY} />

        <div
          data-hud-module
          className="hud-rise relative z-10 flex flex-col items-start gap-3"
        >
          {searchExpanded && (
            <HudSearchBar
              inputRef={searchInputRef}
              query={selectSearchQuery(state)}
              hudOpacity={HUD_OPACITY}
              onQueryChange={(query) => dispatch({ type: "set-query", query })}
              onCollapse={() => dispatch({ type: "collapse-search" })}
            />
          )}

          <HudBallColumn
            slots={selectVisibleBallSlots(state)}
            hudOpacity={HUD_OPACITY}
            onExpandSearch={() => dispatch({ type: "expand-search" })}
          />
        </div>
      </div>

      <HudKeyHintBar hints={selectKeyHints(state)} />
    </div>
  );
}
