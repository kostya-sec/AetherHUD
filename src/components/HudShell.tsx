import { useEffect, useRef } from "react";
import {
  selectActiveL1Id,
  selectInbox,
  selectKeyHints,
  selectL2Panel,
  selectL3Bar,
  selectNotice,
  selectQuickInput,
  selectSearchExpanded,
  selectSearchFocusRequest,
  selectSearchQuery,
  selectSettingsRow,
  selectVisibleBallSlots,
  type HudCommand,
  type HudState,
} from "../hud-model";
import { publishHitBoxes } from "../platform/hitboxes";
import { loadInboxFirstPage, loadInboxNextPage } from "../platform/inbox";
import { submitQuickInput } from "../platform/ingest";
import { ballTopOffset, ballTopOffsetFor } from "./ballLayout";
import { HudBallColumn } from "./HudBallColumn";
import { HudInboxView } from "./HudInboxView";
import { HudKeyHintBar } from "./HudKeyHintBar";
import { HudL2Panel } from "./HudL2Panel";
import { HudL3Bar } from "./HudL3Bar";
import { HudNoticeBar } from "./HudNoticeBar";
import { HudQuickInputModal } from "./HudQuickInputModal";
import { HudSearchBar } from "./HudSearchBar";
import { HudSettingsRow } from "./HudSettingsRow";
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
  const quickInput = selectQuickInput(state);
  const settingsRow = selectSettingsRow(state);
  const inbox = selectInbox(state);
  // 弹窗/列表视图打开时，底层内容 inert：Tab 不逃出弹窗、点击不误触球列。
  const overlayOpen = quickInput !== null || inbox.open;

  // L2 面板与被点 L1 球顶部平行对齐：同一套几何常数计算球顶偏移（#27 验收）。
  const activeBallIndex = activeL1Id
    ? slots.findIndex((slot) => slot.kind === "l1" && slot.l1Id === activeL1Id)
    : -1;
  const panelTop = activeBallIndex >= 0 ? ballTopOffset(activeBallIndex) : 0;

  // L3 条同样原地展开：top 取当前可见球列里 L3 筛选球的索引（搜索框展开/折叠会
  // 改变球列组成），不写死 top；两支同屏互斥由模型保证。
  const l3FilterTop = ballTopOffsetFor(slots, (slot) => slot.kind === "l3-filter");

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
  // 位置也要进依赖：搜索框展开/折叠会把 L3 球上移一颗，L3 条随之换 top。
  const l3BarKey = l3Bar.expanded ? `open:${l3FilterTop ?? "none"}` : "closed";
  useEffect(() => {
    publishHitBoxes();
  }, [
    searchExpanded,
    l3BarKey,
    notice !== null,
    panelKey,
    navigationKey,
    quickInput !== null,
    settingsRow.expanded,
    inbox.open,
  ]);

  // 收件箱打开/收到 inbox-updated 时重载第一页；翻页由「加载更多」显式触发。
  const inboxOpen = inbox.open;
  const inboxRevision = inbox.revision;
  useEffect(() => {
    if (!inboxOpen) return;
    void loadInboxFirstPage(dispatch);
  }, [inboxOpen, inboxRevision, dispatch]);

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
          inert={overlayOpen}
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
              onIngestShortcut={(seed) =>
                dispatch({ type: "open-quick-input", seed })
              }
            />
          )}

          {/* 展开条都挂在球列右侧的同一包含块里（L2 面板 / L3 条原地展开）。 */}
          <div className="relative flex w-full items-start">
            <HudBallColumn
              slots={slots}
              hudOpacity={HUD_OPACITY}
              activeL1Id={activeL1Id}
              l3Expanded={l3Bar.expanded}
              settingsExpanded={settingsRow.expanded}
              onActivateSearch={() => dispatch({ type: "expand-search" })}
              onActivateL1={(l1Id) => dispatch({ type: "activate-l1", l1Id })}
              onActivateL3={() => dispatch({ type: "toggle-l3-bar" })}
              onActivateQuickAdd={() =>
                dispatch({ type: "open-quick-input", seed: "" })
              }
              onActivateSettings={() => dispatch({ type: "toggle-settings-row" })}
            />

            {panel && (
              <HudL2Panel
                panel={panel}
                hudOpacity={HUD_OPACITY}
                top={panelTop}
                onSelectL2={(l2Id) => dispatch({ type: "activate-l2", l2Id })}
              />
            )}

            {l3Bar.expanded && l3FilterTop !== null && (
              <HudL3Bar
                mediaChips={l3Bar.mediaChips}
                formatTags={l3Bar.formatTags}
                hudOpacity={HUD_OPACITY}
                top={l3FilterTop}
                onCollapse={() => dispatch({ type: "collapse-l3-bar" })}
              />
            )}

            {settingsRow.expanded && (
              <HudSettingsRow
                view={settingsRow}
                hudOpacity={HUD_OPACITY}
                onSelect={(itemId) => {
                  // 本票只实现收件箱；其余入口 enabled=false，不会到这里。
                  if (itemId === "inbox") dispatch({ type: "open-inbox" });
                }}
              />
            )}
          </div>
        </div>
      </div>

      {!overlayOpen && <HudKeyHintBar hints={selectKeyHints(state)} />}

      {quickInput && (
        <HudQuickInputModal
          quickInput={quickInput}
          hudOpacity={HUD_OPACITY}
          onSubmit={(text) => void submitQuickInput(dispatch, text)}
          onClose={() => dispatch({ type: "close-quick-input" })}
        />
      )}

      {inbox.open && (
        <HudInboxView
          inbox={inbox}
          hudOpacity={HUD_OPACITY}
          onClose={() => dispatch({ type: "close-inbox" })}
          onLoadMore={() => void loadInboxNextPage(dispatch, inbox.nextCursor)}
        />
      )}
    </div>
  );
}
