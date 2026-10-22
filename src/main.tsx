import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { revealWindowAfterFirstFrame } from "./platform/window";
import "./index.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

// The Tauri window is created hidden to avoid a transparent-window white flash.
// Reveal it from the platform adapter once the first render is committed.
revealWindowAfterFirstFrame();
