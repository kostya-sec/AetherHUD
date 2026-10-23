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
import { publishHitBoxes } from "../platform/hitboxes";
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
 * hud-model through selectors and commands. OS/window wiring lives in src/platform.
 */
export function HudShell({ state, dispatch }: HudShellProps) {
  const searchInputRef = useRef<HTMLInputElement>(null);
  const searchExpanded = selectSearchExpanded(state);
  const focusRequest = selectSearchFocusRequest(state);

  // 点击搜索球（expand-search）时模型递增 focusRequest，渲染层据此把光标移进搜索框。
  // 唤出本身不聚焦（2026-10-23 需求变更：默认球列 + 搜索框收起），初始 focusRequest 为 0。
  // Focus synchronously in the effect; a hidden WKWebView/WebView2 may throttle rAF.
  useEffect(() => {
    if (!searchExpanded || focusRequest <= 0) return;
    searchInputRef.current?.focus();
  }, [searchExpanded, focusRequest]);

  // Click-through is per hitbox: publish the interactive surfaces whenever the layout
  // changes. The shell re-reads the window origin on every poll, so a window move or
  // DPI change needs no re-publish.
  useEffect(() => {
    publishHitBoxes();
  }, [searchExpanded]);

  useEffect(() => {
    const republish = () => publishHitBoxes();
    window.addEventListener("resize", republish);
    return () => window.removeEventListener("resize", republish);
  }, []);

  return (
    <div
      data-hud-phase={state.phase}
      className="relative h-screen w-screen overflow-hidden bg-transparent font-sans text-stone-100 selection:bg-amber-500/30"
      style={{
        textShadow: "0 1px 3px rgba(0,0,0,0.9), 0 0 6px rgba(0,0,0,0.45)",
      }}
    >
      <div className="relative mx-auto w-full max-w-5xl px-2 pt-4">
        <RainBackground density={RAIN_DENSITY} />

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
