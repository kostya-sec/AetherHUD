import { invoke } from "@tauri-apps/api/core";
import type { HudStore } from "../hud-model";

/** 壳侧 ingest_text 的返回（camelCase，见 aether-core::ingest::IngestOutcome）。 */
export interface IngestOutcome {
  readonly entryId: number;
  readonly duplicate: boolean;
  readonly stage: "fetch" | "classify" | null;
  readonly pipelineStatus: string;
  readonly title: string;
  readonly createdAt: number;
  readonly lastSeenAt: number;
}

type Dispatch = HudStore["dispatch"];

/** 提示条里的标题预览：URL 或长笔记不撑爆一行。 */
export function previewTitle(title: string, maxChars = 48): string {
  const chars = Array.from(title);
  if (chars.length <= maxChars) return title;
  return `${chars.slice(0, maxChars).join("")}…`;
}

/**
 * 提交快速录入（#28）：成功即关闭弹窗并提示（重复投递单独提示）；失败保留弹窗，
 * 用户可以直接改内容重试。入库与 pipeline_jobs 行由核心库同事务写入。
 */
export async function submitQuickInput(dispatch: Dispatch, text: string): Promise<void> {
  const trimmed = text.trim();
  if (!trimmed) return;

  dispatch({ type: "set-quick-input-submitting", submitting: true });
  try {
    const outcome = await invoke<IngestOutcome>("ingest_text", { text: trimmed });
    dispatch({ type: "close-quick-input" });
    dispatch({
      type: "show-notice",
      notice: outcome.duplicate
        ? {
            code: "entry_duplicate",
            detail: `重复投递，已更新最近出现时间：${previewTitle(outcome.title)}`,
          }
        : {
            code: "entry_ingested",
            detail: `已入库，正在分类：${previewTitle(outcome.title)}`,
          },
    });
  } catch (error) {
    dispatch({ type: "set-quick-input-submitting", submitting: false });
    dispatch({
      type: "show-notice",
      notice: {
        code: "ingest_failed",
        detail: `入库失败：${String(error)}`,
      },
    });
  }
}
