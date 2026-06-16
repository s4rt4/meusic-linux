import { describe, it, expect } from "vitest";
import { MEDIA_ERR, formatPlaybackError } from "./playbackError";

describe("formatPlaybackError", () => {
  const base = {
    code: 4,
    mediaMessage:
      "PipelineStatus::DEMUXER_ERROR_COULD_NOT_OPEN: FFmpegDemuxer: open context failed",
    readyState: 0,
    networkState: 3,
    src: "C:\\Users\\Sarta\\Music\\Dopamine\\Asing\\01. LOST IN PARADISE (feat. AKLO).flac",
  };

  it("renders the real-world COULD_NOT_OPEN failure (the empty-MIME FLAC bug)", () => {
    const line = formatPlaybackError("error event", base);
    expect(line).toBe(
      'playback error event | MediaError 4 (SRC_NOT_SUPPORTED) | ' +
        'msg="PipelineStatus::DEMUXER_ERROR_COULD_NOT_OPEN: FFmpegDemuxer: open context failed" | ' +
        "readyState=0 networkState=3 | " +
        "src=C:\\Users\\Sarta\\Music\\Dopamine\\Asing\\01. LOST IN PARADISE (feat. AKLO).flac",
    );
  });

  it("maps every MediaError code to its name", () => {
    for (const [code, name] of Object.entries(MEDIA_ERR)) {
      const line = formatPlaybackError("x", { ...base, code: Number(code) });
      expect(line).toContain(`MediaError ${code} (${name})`);
    }
  });

  it("labels an unknown MediaError code with '?'", () => {
    const line = formatPlaybackError("x", { ...base, code: 99 });
    expect(line).toContain("MediaError 99 (?)");
  });

  it("includes the rejection reason from play()'s rejected promise", () => {
    const line = formatPlaybackError(
      "play() rejected",
      base,
      new Error("Failed to load because no supported source was found."),
    );
    expect(line).toContain(
      "reason=Failed to load because no supported source was found.",
    );
  });

  it("accepts a non-Error reason (string)", () => {
    const line = formatPlaybackError("play() rejected", base, "boom");
    expect(line).toContain("reason=boom");
  });

  it("omits MediaError, msg and reason segments when absent", () => {
    const line = formatPlaybackError("error event", {
      readyState: 4,
      networkState: 1,
      src: "ok.flac",
    });
    expect(line).toBe("playback error event | readyState=4 networkState=1 | src=ok.flac");
    expect(line).not.toContain("MediaError");
    expect(line).not.toContain("msg=");
    expect(line).not.toContain("reason=");
  });

  it("does not treat code 0 as a present error code", () => {
    // 0 is not a valid MediaError code; falsy → segment omitted.
    const line = formatPlaybackError("x", { ...base, code: 0 });
    expect(line).not.toContain("MediaError");
  });
});
