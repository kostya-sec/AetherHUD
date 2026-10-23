import type { BallSlot } from "../hud-model";

/**
 * Ball column geometry (#27): the L2 panel must open in place with its top edge
 * parallel to the clicked L1 ball, so the panel top offset is derived from the
 * same constants the column renders with. Keep in sync with HudBallColumn.
 */
export const BALL_SIZE_PX = 48;
export const BALL_GAP_PX = 12;
export const BALL_COLUMN_PADDING_PX = 4;

/** Left edge of the expanded strip: beside the ball column with a small gap. */
export const PANEL_LEFT_PX = BALL_SIZE_PX + 16;

/** Top offset (CSS px) of the ball at index inside the ball column. */
export function ballTopOffset(index: number): number {
  if (!Number.isFinite(index) || index < 0) return BALL_COLUMN_PADDING_PX;
  return BALL_COLUMN_PADDING_PX + index * (BALL_SIZE_PX + BALL_GAP_PX);
}

/**
 * Top offset (CSS px) of the first slot matching `matches` in the **currently
 * visible** ball column.
 *
 * The column composition is dynamic: the search ball only exists while the search
 * box is collapsed, and L1 balls come from the effective taxonomy. The L3 bar must
 * therefore derive its top from the live slot list on every render (a hard-coded
 * index would drift as soon as the search box expands/collapses).
 *
 * Returns null when no slot matches, so callers never invent a top for a strip
 * that has nothing to align with.
 */
export function ballTopOffsetFor(
  slots: readonly BallSlot[],
  matches: (slot: BallSlot) => boolean,
): number | null {
  const index = slots.findIndex(matches);
  return index < 0 ? null : ballTopOffset(index);
}
