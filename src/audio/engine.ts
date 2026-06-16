/**
 * AudioEngine — a thin singleton wrapper around an HTMLAudioElement routed
 * through the Web Audio API:
 *
 *   <audio> → MediaElementSource → [6× BiquadFilter EQ] → Analyser → output
 *
 * The EQ filters give a real equalizer; the analyser feeds the visualizer.
 * It is a module-level singleton so React StrictMode's double-mount (dev)
 * never creates two MediaElementSources for the same element (which throws).
 */

export const EQ_BANDS = [60, 170, 350, 1000, 3500, 10000] as const;

class AudioEngine {
  readonly audio: HTMLAudioElement;
  private ctx: AudioContext | null = null;
  private analyser: AnalyserNode | null = null;
  private filters: BiquadFilterNode[] = [];
  private freqData = new Uint8Array(0);
  private wired = false;
  // Desired EQ gains (dB), kept independently of the graph so values set before
  // the graph exists are applied when it is built.
  private gains: number[] = new Array(EQ_BANDS.length).fill(0);

  constructor() {
    this.audio = new Audio();
    this.audio.crossOrigin = "anonymous";
    this.audio.preload = "auto";
  }

  /**
   * Build the Web Audio graph. Must run after a user gesture (browsers block
   * AudioContext otherwise). Safe to call repeatedly — only wires once.
   */
  ensureGraph() {
    if (this.wired) {
      void this.ctx?.resume();
      return;
    }
    const ctx = new AudioContext();
    const source = ctx.createMediaElementSource(this.audio);

    let node: AudioNode = source;
    this.filters = EQ_BANDS.map((freq, i) => {
      const f = ctx.createBiquadFilter();
      f.type =
        i === 0 ? "lowshelf" : i === EQ_BANDS.length - 1 ? "highshelf" : "peaking";
      f.frequency.value = freq;
      f.Q.value = 1.0;
      f.gain.value = this.gains[i] ?? 0;
      node.connect(f);
      node = f;
      return f;
    });

    const analyser = ctx.createAnalyser();
    analyser.fftSize = 256;
    analyser.smoothingTimeConstant = 0.8;
    node.connect(analyser);
    analyser.connect(ctx.destination);

    this.ctx = ctx;
    this.analyser = analyser;
    this.freqData = new Uint8Array(analyser.frequencyBinCount);
    this.wired = true;
  }

  /** Set the gain (dB, -12..12) of EQ band `index`. */
  setEq(index: number, gainDb: number) {
    this.gains[index] = gainDb;
    const f = this.filters[index];
    if (f) f.gain.value = gainDb;
  }

  resetEq() {
    this.gains.fill(0);
    this.filters.forEach((f) => (f.gain.value = 0));
  }

  /** Snapshot the current frequency spectrum (0..255 per bin). */
  getSpectrum(): Uint8Array {
    if (!this.analyser) return this.freqData;
    this.analyser.getByteFrequencyData(this.freqData);
    return this.freqData;
  }

  get hasGraph() {
    return this.wired;
  }
}

export const engine = new AudioEngine();
