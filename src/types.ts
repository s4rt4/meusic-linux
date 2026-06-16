/** A single audio track, mirrors the `Track` struct returned by the Rust backend. */
export interface Track {
  path: string;
  title: string;
  artist: string;
  album: string;
  album_artist: string;
  track_no: number;
  duration: number; // seconds
  has_cover: boolean;
  format: string; // e.g. "FLAC", "MP3"
  bitrate: number; // kbps, 0 if unknown
}

/** A radio station. User-editable; persisted to the `stations` file store. */
export interface Station {
  id: string;
  name: string;
  url: string;
}

/** Top-level view: the local music player vs. the radio streaming view. */
export type AppMode = "music" | "radio";

/** Live stream info for the playing radio station (from the proxy's ICY parse). */
export interface RadioMeta {
  title: string | null; // current song (ICY StreamTitle)
  codec: string | null; // e.g. "MP3", "AAC"
  bitrate: number | null; // kbps
  name: string | null; // station name advertised by the server
}

/** `radio:meta` event payload from the Rust proxy. */
export interface RadioMetaEvent extends Partial<RadioMeta> {
  url: string; // upstream URL this metadata belongs to
}

/** `radio:error` event payload — a station that failed to connect. */
export interface RadioErrorEvent {
  url: string;
  message: string;
  permanent: boolean; // true = bad URL/auth (stop); false = transient (retry)
}

/** Connection lifecycle of the radio stream, for status display. */
export type RadioStatus =
  | "idle"
  | "connecting"
  | "playing"
  | "buffering"
  | "reconnecting"
  | "error";

export type RepeatMode = "off" | "all" | "one";

/** An RGB color triple used for the adaptive gradient background. */
export type RGB = [number, number, number];

export interface Palette {
  /** Dominant colors extracted from the current cover art, brightest first. */
  colors: RGB[];
}
