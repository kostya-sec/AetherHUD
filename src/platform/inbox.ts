import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { HudStore, InboxCursor, InboxPage } from "../hud-model";

/** Shell → webview: a new entry landed, or a duplicate refreshed last_seen_at. */
export const INBOX_UPDATED_EVENT = "hud://inbox-updated";

type Dispatch = HudStore["dispatch"];

/**
 * 打开收件箱后的第一页（50 条，created_at 倒序）。失败时退出 loading 并给出
 * 一次性提示条，不打断 HUD 其他操作。
 */
export async function loadInboxFirstPage(dispatch: Dispatch): Promise<void> {
  dispatch({ type: "set-inbox-loading", loading: true });
  try {
    const page = await invoke<InboxPage>("hud_inbox", { cursor: null });
    dispatch({ type: "replace-inbox", page });
  } catch (error) {
    dispatch({ type: "set-inbox-loading", loading: false });
    dispatch({
      type: "show-notice",
      notice: {
        code: "inbox_unavailable",
        detail: `读取收件箱失败：${String(error)}`,
      },
    });
  }
}

/** 加载更多：用上一页的 keyset 游标取下一页并追加。 */
export async function loadInboxNextPage(
  dispatch: Dispatch,
  cursor: InboxCursor | null,
): Promise<void> {
  if (!cursor) return;
  dispatch({ type: "set-inbox-loading", loading: true });
  try {
    const page = await invoke<InboxPage>("hud_inbox", { cursor });
    dispatch({ type: "append-inbox", page });
  } catch (error) {
    dispatch({ type: "set-inbox-loading", loading: false });
    dispatch({
      type: "show-notice",
      notice: {
        code: "inbox_unavailable",
        detail: `加载更多失败：${String(error)}`,
      },
    });
  }
}

/**
 * Module-level listen wrapper so tests can fake the Tauri event bridge.
 * Resolves only after the listener is registered.
 */
export async function connectInboxEvents(store: HudStore): Promise<() => void> {
  const settled = await Promise.allSettled([
    listen(INBOX_UPDATED_EVENT, () => store.dispatch({ type: "invalidate-inbox" })),
  ]);
  const unlisteners: UnlistenFn[] = settled.flatMap((result) =>
    result.status === "fulfilled" ? [result.value] : [],
  );
  return () => {
    for (const unlisten of unlisteners) unlisten();
    unlisteners.length = 0;
  };
}
