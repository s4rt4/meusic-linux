import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import App from "./App";
import { MiniPlayer } from "./components/MiniPlayer";
import "./index.css";

// Forward uncaught frontend errors to the Rust log file for crash monitoring.
const logEvent = (level: string, message: string) =>
  invoke("log_event", { level, message }).catch(() => {});
window.addEventListener("error", (e) =>
  logEvent("ERROR", `${e.message} @ ${e.filename}:${e.lineno}:${e.colno}`)
);
window.addEventListener("unhandledrejection", (e) =>
  logEvent("REJECT", String((e.reason && (e.reason.stack || e.reason.message)) ?? e.reason))
);

// The same bundle drives both windows; render by window label.
// Guard so a label lookup failure falls back to the main UI rather than a blank window.
let isMini = false;
try {
  isMini = getCurrentWindow().label === "miniplayer";
} catch {
  isMini = false;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{isMini ? <MiniPlayer /> : <App />}</React.StrictMode>,
);
