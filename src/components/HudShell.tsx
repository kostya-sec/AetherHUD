import { useEffect, useRef } from "react";
import {
  selectActiveL1Id,
  selectKeyHints,
  selectL2Panel,
  selectL3Bar,
  selectNotice,
  selectSearchExpanded,
  selectSearchFocusRequest,
  selectSearchQuery,
  selectVisibleBallSlots,
  type HudCommand,
  type HudState,
} from "../hud-model";
import { publishHitBoxes } from "../platform/hitboxes";
import { ballTopOffset } from "./ballLayout";
import { HudBallColumn } from "./HudBallColumn";
import { HudKeyHintBar } from "./HudKeyHintBar";
import { HudL2Panel } from "./HudL2Panel";
import { HudL3Bar } from "./HudL3Bar";
import { HudNoticeBar } from "./HudNoticeBar";
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
  const slots = selectVisibleBallSlots(state);
  const activeL1Id = selectActiveL1Id(state);
  const panel = selectL2Panel(state);
  const l3Bar = selectL3Bar(state);
  const notice = selectNotice(state);

  // L2 面板与被点 L1 球顶部平行对齐：同一套几何常数计算球顶偏移（#27 验收）。
  const activeBallIndex = activeL1Id
    ? slots.findIndex((slot) => slot.kind === "l1" && slot.l1Id === activeL1Id)
    : -1;
  const panelTop = activeBallIndex >= 0 ? ballTopOffset(activeBallIndex) : 0;

  // 点击搜索球（expand-search）时模型递增 focusRequest，渲染层据此把光标移进搜索框。
  // 唤出本身不聚焦（2026-10-23 需求变更：默认球列 + 搜索框收起），初始 focusRequest 为 0。
  // Focus synchronously in the effect; a hidden WKWebView/WebView2 may throttle rAF.
  useEffect(() => {
    if (!searchExpanded || focusRequest <= 0) return;
    searchInputRef.current?.focus();
  }, [searchExpanded, focusRequest]);

  // Click-through is per hitbox: publish the interactive surfaces whenever the layout
  // changes (search bar, L3 bar, L2 panel, notice strip, taxonomy arrival). The shell
  // re-reads the window origin on every poll, so a window move or DPI change needs no
  // re-publish.
  const navigationKey = state.navigation?.effectiveVersion ?? "none";
  const panelKey = panel ? `${panel.l1.id}:${panel.activeL2Id ?? "all"}` : "closed";
  useEffect(() => {
    publishHitBoxes();
  }, [searchExpanded, l3Bar.expanded, notice !== null, panelKey, navigationKey]);

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
          {notice && (
            <HudNoticeBar
              notice={notice}
              onDismiss={() => dispatch({ type: "dismiss-notice" })}
            />
          )}

          {searchExpanded && (
            <HudSearchBar
              inputRef={searchInputRef}
              query={selectSearchQuery(state)}
              hudOpacity={HUD_OPACITY}
              onQueryChange={(query) => dispatch({ type: "set-query", query })}
              onCollapse={() => dispatch({ type: "collapse-search" })}
            />
          )}

          {l3Bar.expanded && (
            <HudL3Bar
              mediaChips={l3Bar.mediaChips}
              formatTags={l3Bar.formatTags}
              hudOpacity={HUD_OPACITY}
              onCollapse={() => dispatch({ type: "collapse-l3-bar" })}
            />
          )}

          <div className="relative flex w-full items-start">
            <HudBallColumn
              slots={slots}
              hudOpacity={HUD_OPACITY}
              activeL1Id={activeL1Id}
              l3Expanded={l3Bar.expanded}
              onActivateSearch={() => dispatch({ type: "expand-search" })}
              onActivateL1={(l1Id) => dispatch({ type: "activate-l1", l1Id })}
              onActivateL3={() => dispatch({ type: "toggle-l3-bar" })}
            />

            {panel && (
              <HudL2Panel
                panel={panel}
                hudOpacity={HUD_OPACITY}
                top={panelTop}
                onSelectL2={(l2Id) => dispatch({ type: "activate-l2", l2Id })}
              />
            )}
          </div>
        </div>
      </div>

      <HudKeyHintBar hints={selectKeyHints(state)} />
    </div>
  );
}
