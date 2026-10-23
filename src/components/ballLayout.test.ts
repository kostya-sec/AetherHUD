import { describe, expect, it } from "vitest";
import {
  BALL_COLUMN_PADDING_PX,
  BALL_SIZE_PX,
  PANEL_LEFT_PX,
  ballTopOffset,
} from "./ballLayout";

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
