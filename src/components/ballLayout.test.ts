import { describe, expect, it } from "vitest";
import type { BallSlot } from "../hud-model";
import { createDefaultHudState, hudReducer, selectVisibleBallSlots } from "../hud-model";
import {
  BALL_COLUMN_PADDING_PX,
  BALL_SIZE_PX,
  PANEL_LEFT_PX,
  ballTopOffset,
  ballTopOffsetFor,
} from "./ballLayout";

const isL3Filter = (slot: BallSlot): boolean => slot.kind === "l3-filter";

describe("球列几何（L2 面板与球顶对齐）", () => {
  it("第一颗球的顶部偏移就是列内边距", () => {
    expect(ballTopOffset(0)).toBe(BALL_COLUMN_PADDING_PX);
  });

  it("第 2 / 5 颗球的顶偏移是设计值（48+12 间距）", () => {
    expect(ballTopOffset(1)).toBe(64);
    expect(ballTopOffset(4)).toBe(244);
  });

  it("L2 面板左缘在球列右侧留出间隙", () => {
    expect(PANEL_LEFT_PX).toBeGreaterThan(BALL_SIZE_PX);
  });

  it("非法索引回退到列顶", () => {
    expect(ballTopOffset(-1)).toBe(BALL_COLUMN_PADDING_PX);
    expect(ballTopOffset(Number.NaN)).toBe(BALL_COLUMN_PADDING_PX);
  });
});

describe("展开条 top 按当前可见球列实时计算（不写死索引）", () => {
  it("搜索球可见时 L3 球是第 2 颗；搜索框展开后搜索球消失，L3 球升到第 1 颗", () => {
    const collapsed = createDefaultHudState();
    expect(ballTopOffsetFor(selectVisibleBallSlots(collapsed), isL3Filter)).toBe(
      ballTopOffset(1),
    );

    const expanded = hudReducer(collapsed, { type: "expand-search" });
    expect(ballTopOffsetFor(selectVisibleBallSlots(expanded), isL3Filter)).toBe(
      ballTopOffset(0),
    );
  });

  it("球列里找不到目标槽位时返回 null（渲染层据此不硬编码 top）", () => {
    expect(ballTopOffsetFor([], isL3Filter)).toBeNull();
  });
});
