import { Search, X } from "lucide-react";
import type { RefObject } from "react";
import { hudSurfaceStyle } from "./hudSurface";

interface HudSearchBarProps {
  inputRef: RefObject<HTMLInputElement | null>;
  query: string;
  hudOpacity: number;
  onQueryChange: (query: string) => void;
  onCollapse: () => void;
}

/** Top search bar expanded from the search ball, copied from the #7 desktop-hud layer. */
export function HudSearchBar({
  inputRef,
  query,
  hudOpacity,
  onQueryChange,
  onCollapse,
}: HudSearchBarProps) {
  return (
    <div
      className="pixel-box flex w-full items-center gap-2.5 rounded-xl border border-stone-700/80 px-3 py-2 transition-[border-color,box-shadow] focus-within:border-amber-500/80 focus-within:shadow-[0_0_0_1px_rgba(245,158,11,0.3),0_0_18px_rgba(245,158,11,0.16)]"
      style={hudSurfaceStyle(hudOpacity)}
    >
      <Search className="h-4 w-4 shrink-0 text-amber-400" aria-hidden="true" />
      <input
        ref={inputRef}
        value={query}
        onChange={(event) => onQueryChange(event.target.value)}
        placeholder="搜索…"
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
