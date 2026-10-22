import { useSyncExternalStore } from "react";
import { HudShell } from "./components/HudShell";
import type { HudStore } from "./hud-model";

export default function App({ store }: { store: HudStore }) {
  const state = useSyncExternalStore(store.subscribe, store.getState, store.getState);

  return <HudShell state={state} dispatch={store.dispatch} />;
}
