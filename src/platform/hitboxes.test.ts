import { describe, expect, it, vi } from "vitest";
import {
  HITBOX_ATTRIBUTE,
  collectHitBoxes,
  createHitBoxPublisher,
  type HitBoxSchedule,
  type HitBoxSignalSources,
} from "./hitboxes";

interface FakeSources extends HitBoxSignalSources {
  fireLayout(): void;
  fireSummon(): void;
  layoutSubscribers(): number;
  summonSubscribers(): number;
}

function fakeSources(): FakeSources {
  const layout = new Set<() => void>();
  const summon = new Set<() => void>();
  return {
    subscribeLayout(onChange) {
      layout.add(onChange);
      return () => layout.delete(onChange);
    },
    subscribeSummon(onSummon) {
      summon.add(onSummon);
      return () => summon.delete(onSummon);
    },
    fireLayout() {
      for (const listener of [...layout]) listener();
    },
    fireSummon() {
      for (const listener of [...summon]) listener();
    },
    layoutSubscribers: () => layout.size,
    summonSubscribers: () => summon.size,
  };
}

function fakeScheduler(): { schedule: HitBoxSchedule; run(): void; pending(): number } {
  const tasks: Array<() => void> = [];
  return {
    schedule: (task) => tasks.push(task),
    run() {
      for (const task of tasks.splice(0)) task();
    },
    pending: () => tasks.length,
  };
}

/** A ParentNode stub: vitest runs in the node environment, so there is no DOM. */
function stubRoot(
  rects: ReadonlyArray<{ x: number; y: number; width: number; height: number }>,
): { root: ParentNode; selectors: string[] } {
  const selectors: string[] = [];
  const elements = rects.map((rect) => ({
    getBoundingClientRect: () => ({
      left: rect.x,
      top: rect.y,
      width: rect.width,
      height: rect.height,
    }),
  }));
  const root = {
    querySelectorAll: (selector: string) => {
      selectors.push(selector);
      return elements;
    },
  } as unknown as ParentNode;
  return { root, selectors };
}

const HITBOX_SELECTOR = "[" + HITBOX_ATTRIBUTE + "]";

describe("collectHitBoxes", () => {
  it("reports every tagged element in window-relative CSS pixels", () => {
    const { root, selectors } = stubRoot([
      { x: 36, y: 20, width: 48, height: 48 },
      { x: 36, y: 80, width: 48, height: 48 },
    ]);

    expect(collectHitBoxes(root)).toEqual([
      { x: 36, y: 20, width: 48, height: 48 },
      { x: 36, y: 80, width: 48, height: 48 },
    ]);
    expect(selectors[0]).toBe(HITBOX_SELECTOR);
  });

  it("skips collapsed controls so the shell never captures the cursor over nothing", () => {
    const { root } = stubRoot([
      { x: 10, y: 10, width: 0, height: 48 },
      { x: 10, y: 70, width: 48, height: 0 },
      { x: 10, y: 130, width: 48, height: 48 },
    ]);

    expect(collectHitBoxes(root)).toEqual([
      { x: 10, y: 130, width: 48, height: 48 },
    ]);
  });
});

describe("createHitBoxPublisher", () => {
  it("publishes on the next task, never inside the signal itself", () => {
    const sources = fakeSources();
    const scheduler = fakeScheduler();
    const publish = vi.fn();
    createHitBoxPublisher(publish, sources, scheduler.schedule);

    sources.fireLayout();
    expect(publish).not.toHaveBeenCalled();
    expect(scheduler.pending()).toBe(1);

    scheduler.run();
    expect(publish).toHaveBeenCalledTimes(1);
  });

  it("coalesces a burst of layout signals into one re-measure", () => {
    const sources = fakeSources();
    const scheduler = fakeScheduler();
    const publish = vi.fn();
    createHitBoxPublisher(publish, sources, scheduler.schedule);

    sources.fireLayout();
    sources.fireLayout();
    sources.fireSummon();
    sources.fireLayout();

    expect(scheduler.pending()).toBe(1);
    scheduler.run();
    expect(publish).toHaveBeenCalledTimes(1);
  });

  it("re-measures after the shell maps the window, even after the first publish", () => {
    // #29 回归：挂载时（窗口仍隐藏）发布的那一份矩形并不对应上屏后的视口。
    const sources = fakeSources();
    const scheduler = fakeScheduler();
    const publish = vi.fn();
    createHitBoxPublisher(publish, sources, scheduler.schedule);

    publish(); // 挂载时由 HudShell 的布局 effect 直接发布
    scheduler.run();
    expect(publish).toHaveBeenCalledTimes(1);

    sources.fireSummon(); // hud://summon：窗口已 show / 聚焦
    scheduler.run();
    expect(publish).toHaveBeenCalledTimes(2);

    sources.fireSummon(); // 之后的每次唤出同样重测
    scheduler.run();
    expect(publish).toHaveBeenCalledTimes(3);
  });

  it("re-measures when the viewport or the HUD content box changes", () => {
    const sources = fakeSources();
    const scheduler = fakeScheduler();
    const publish = vi.fn();
    const publisher = createHitBoxPublisher(publish, sources, scheduler.schedule);

    sources.fireLayout();
    scheduler.run();
    publisher.request(); // frontend_ready 返回：窗口刚上屏
    scheduler.run();
    sources.fireLayout(); // resize / ResizeObserver / 入场动画结束
    scheduler.run();

    expect(publish).toHaveBeenCalledTimes(3);
  });

  it("stops listening and drops a pending publish once disposed", () => {
    const sources = fakeSources();
    const scheduler = fakeScheduler();
    const publish = vi.fn();
    const publisher = createHitBoxPublisher(publish, sources, scheduler.schedule);

    sources.fireLayout();
    publisher.dispose();
    scheduler.run();
    expect(publish).not.toHaveBeenCalled();
    expect(sources.layoutSubscribers()).toBe(0);
    expect(sources.summonSubscribers()).toBe(0);

    publisher.request();
    sources.fireSummon();
    scheduler.run();
    expect(publish).not.toHaveBeenCalled();
  });
});
