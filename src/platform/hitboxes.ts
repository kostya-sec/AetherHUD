import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { SUMMON_EVENT } from "./hudBridge";

/**
 * Marks an element whose area must capture the cursor. Everything else in the HUD
 * window stays click-through, so a click beside the balls reaches the desktop (#26).
 */
export const HITBOX_ATTRIBUTE = "data-hud-hitbox";

/** Window-relative CSS pixel rectangle, the unit the shell converts to physical px. */
export interface CssRect {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

/**
 * Collects the hitboxes of every tagged element. Hidden or zero-sized elements are
 * skipped so the shell never captures the cursor over nothing.
 */
export function collectHitBoxes(root: ParentNode = document): CssRect[] {
  const rects: CssRect[] = [];
  for (const element of root.querySelectorAll<HTMLElement>(
    `[${HITBOX_ATTRIBUTE}]`,
  )) {
    const rect = element.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) continue;
    rects.push({
      x: rect.left,
      y: rect.top,
      width: rect.width,
      height: rect.height,
    });
  }
  return rects;
}

/**
 * Publishes the current hitboxes to the Rust shell. Called whenever the HUD layout
 * changes: the window itself never needs to re-report after a move or DPI change,
 * because the shell re-reads the window origin on every poll.
 */
export function publishHitBoxes(rects: CssRect[] = collectHitBoxes()): void {
  try {
    void invoke("set_hit_boxes", { rects }).catch(() => undefined);
  } catch {
    // Plain-browser dev has no Tauri IPC bridge.
  }
}

/**
 * Schedules a re-measure. A hidden WebView may suspend animation frames (wry/WebView2
 * occlusion behavior), so this must never be gated on `requestAnimationFrame`.
 */
export type HitBoxSchedule = (task: () => void) => void;

const nextTask: HitBoxSchedule = (task) => {
  setTimeout(task, 0);
};

/** The "the measured layout may be stale" signals the publisher listens to. */
export interface HitBoxSignalSources {
  /** Window resize, visibility change and the HUD's own entrance animation/transition. */
  subscribeLayout(onChange: () => void): () => void;
  /** The shell mapped/focused the HUD window (`hud://summon`). */
  subscribeSummon(onSummon: () => void): () => void;
}

export interface HitBoxPublisher {
  /** Coalesces a re-measure into a single publish on the next task. */
  request(): void;
  dispose(): void;
}

/**
 * Turns any number of "the layout may have changed" signals into one re-measure.
 *
 * The HUD window is created hidden and only mapped when the webview reports ready, so
 * the first measurement happens in a viewport that is not the one the user clicks on
 * (#29 回归：初次启动点击球列被判为点击外部). Re-measuring on the shell's summon event
 * and on every later layout signal keeps the shell's hitboxes equal to the live DOM.
 */
export function createHitBoxPublisher(
  publish: () => void,
  sources: HitBoxSignalSources,
  schedule: HitBoxSchedule = nextTask,
): HitBoxPublisher {
  let scheduled = false;
  let disposed = false;

  const run = (): void => {
    scheduled = false;
    if (disposed) return;
    publish();
  };
  const request = (): void => {
    if (disposed || scheduled) return;
    scheduled = true;
    schedule(run);
  };

  const unsubscribeLayout = sources.subscribeLayout(request);
  const unsubscribeSummon = sources.subscribeSummon(request);

  return {
    request,
    dispose(): void {
      if (disposed) return;
      disposed = true;
      scheduled = false;
      unsubscribeLayout();
      unsubscribeSummon();
    },
  };
}

/** DOM/viewport signals: a resize, a visibility change or a finished HUD animation. */
function subscribeLayoutSignals(onChange: () => void): () => void {
  if (typeof window === "undefined") return () => undefined;

  // Only the HUD container's own entrance animation matters; a per-control
  // transitionend (hover colors) never moves a hitbox.
  const handleAnimation = (event: Event): void => {
    const target = event.target;
    if (target instanceof Element && target.hasAttribute("data-hud-module")) {
      onChange();
    }
  };
  const handleViewport = (): void => {
    onChange();
  };

  const MODULE_EVENTS = ["animationend", "transitionend"] as const;
  const VIEWPORT_EVENTS = ["resize", "visibilitychange"] as const;
  for (const type of MODULE_EVENTS) window.addEventListener(type, handleAnimation);
  for (const type of VIEWPORT_EVENTS) window.addEventListener(type, handleViewport);

  // Content-box changes that never fire a window resize (late font/stylesheet, a
  // notice bar inserting itself) also invalidate the measured rectangles.
  let observer: ResizeObserver | null = null;
  const module = document.querySelector("[data-hud-module]") ?? document.body;
  if (module && typeof ResizeObserver !== "undefined") {
    observer = new ResizeObserver(handleViewport);
    observer.observe(module);
  }

  return () => {
    for (const type of MODULE_EVENTS) window.removeEventListener(type, handleAnimation);
    for (const type of VIEWPORT_EVENTS) window.removeEventListener(type, handleViewport);
    observer?.disconnect();
    observer = null;
  };
}

/** The shell event that says "the window is mapped and focused"; listeners are async. */
function subscribeSummonSignal(onSummon: () => void): () => void {
  let cancelled = false;
  let unlisten: UnlistenFn | null = null;
  void listen(SUMMON_EVENT, () => onSummon())
    .then((stop) => {
      if (cancelled) stop();
      else unlisten = stop;
    })
    .catch(() => undefined);
  return () => {
    cancelled = true;
    unlisten?.();
    unlisten = null;
  };
}

/**
 * Production wiring: re-measures on summon, resize, visibility and layout changes.
 * The caller re-requests once the shell finished mapping the window
 * (`frontend_ready`), because that is the moment the viewport becomes real.
 */
export function connectHitBoxPublishing(
  publish: () => void = () => publishHitBoxes(),
): HitBoxPublisher {
  return createHitBoxPublisher(publish, {
    subscribeLayout: subscribeLayoutSignals,
    subscribeSummon: subscribeSummonSignal,
  });
}
