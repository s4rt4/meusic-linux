import type { RGB } from "../types";

/** Playback state the main window broadcasts to the tray mini-player. */
export interface MiniState {
  hasTrack: boolean;
  title: string;
  artist: string;
  coverUrl: string | null;
  isPlaying: boolean;
  position: number;
  duration: number;
  volume: number;
  accent: RGB;
}

/** Commands the mini-player sends back to the main window. */
export type MiniCmd =
  | { action: "toggle" }
  | { action: "next" }
  | { action: "prev" }
  | { action: "seek"; value: number }
  | { action: "volume"; value: number }
  | { action: "show-main" };

// Cross-window event names.
export const MP_STATE = "mp:state"; // main -> mini (state broadcast)
export const MP_CMD = "mp:cmd"; // mini -> main (control)
export const MP_REQUEST = "mp:request"; // mini -> main (ask for current state)
