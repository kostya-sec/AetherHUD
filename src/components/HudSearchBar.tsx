import { Search, X } from "lucide-react";
import type { RefObject } from "react";
import { isIngestShortcut } from "../hud-model";
import { hudSurfaceStyle } from "./hudSurface";

interface HudSearchBarProps {
  inputRef: RefObject<HTMLInputElement | null>;
  query: string;
  hudOpacity: number;
  onQueryChange: (query: string) => void;
  onCollapse: () => void;
  /** Mod+Enter：打开快速录入弹窗并带入当前输入（#17 §5.2），搜索行为在 #30。 */
  onIngestShortcut: (query: string) => void;
}

/** Top search bar expanded from the search ball, copied from the #7 desktop-hud layer. */
export function HudSearchBar({
  inputRef,
  query,
  hudOpacity,
  onQueryChange,
  onCollapse,
  onIngestShortcut,
}: HudSearchBarProps) {
  return (
    <div
      data-hud-hitbox
      className="pixel-box flex w-full items-center gap-2.5 rounded-xl border border-stone-700/80 px-3 py-2 transition-[border-color,box-shadow] focus-within:border-amber-500/80 focus-within:shadow-[0_0_0_1px_rgba(245,158,11,0.3),0_0_18px_rgba(245,158,11,0.16)]"
      style={hudSurfaceStyle(hudOpacity)}
    >
      <Search className="h-4 w-4 shrink-0 text-amber-400" aria-hidden="true" />
      <input
        ref={inputRef}
        value={query}
        onChange={(event) => onQueryChange(event.target.value)}
        onKeyDown={(event) => {
          // 普通 Enter 留给搜索（#30）；Mod+Enter 显式入库。
          if (!isIngestShortcut(event.nativeEvent)) return;
          event.preventDefault();
          onIngestShortcut(query);
        }}
        placeholder="搜索（Mod+Enter 入库）…"
        aria-label="搜索"
        className="min-w-0 flex-1 rounded bg-transparent font-mono text-sm text-stone-100 outline-none placeholder:text-stone-500"
      />
      <button
        type="button"
        onClick={onCollapse}
        title="折叠搜索（恢复为球）"
        aria-label="折叠搜索"
        className="pixel-box-sm hud-focus-ring flex h-5 w-5 shrink-0 items-center justify-center rounded border border-stone-700 text-stone-400 transition-colors hover:border-amber-400 hover:text-amber-300"
      >
        <X className="h-3 w-3" aria-hidden="true" />
      </button>
    </div>
  );
}
