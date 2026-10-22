import { useState, useSyncExternalStore } from "react";
import { HudShell } from "./components/HudShell";
import { createHudStore } from "./hud-model";

export default function App() {
  const [store] = useState(() => createHudStore());
  const state = useSyncExternalStore(store.subscribe, store.getState, store.getState);

  return <HudShell state={state} dispatch={store.dispatch} />;
}
