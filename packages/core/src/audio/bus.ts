import { createEmptyFeatures, type AudioFeatures } from './features';
import type { AudioDriver } from './driver';
import { RealtimeDriver } from './realtime-driver';

/**
 * The AudioFeatures bus. Owns the single per-frame snapshot and delegates to an
 * interchangeable driver. Modes never touch Web Audio — they read what the bus
 * produces. Defaults to a realtime driver.
 */
export class AudioBus {
  /** Reused per-frame snapshot. Do not retain across frames. */
  readonly features: AudioFeatures = createEmptyFeatures();

  private driver: AudioDriver;
  private frame = 0;
  private ctx: AudioContext | null = null;
  private currentSource: AudioNode | null = null;
  private sourceListeners = new Set<(c: AudioContext, s: AudioNode) => void>();

  constructor(driver?: AudioDriver) {
    this.driver = driver ?? new RealtimeDriver();
  }

  /** Swap the active driver (e.g. realtime ↔ deterministic for export). */
  setDriver(driver: AudioDriver): void {
    this.driver = driver;
  }

  get activeDriver(): AudioDriver {
    return this.driver;
  }

  /** Create or return the single shared AudioContext (shared with preset engines). */
  ensureContext(): AudioContext {
    if (!this.ctx) {
      const Ctor =
        window.AudioContext ||
        (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
      this.ctx = new Ctor();
    }
    return this.ctx;
  }

  /** The currently connected source node, or null. */
  get audioSource(): AudioNode | null {
    return this.currentSource;
  }

  /** Subscribe to source (re)connections; returns an unsubscribe fn. */
  onSource(cb: (c: AudioContext, s: AudioNode) => void): () => void {
    this.sourceListeners.add(cb);
    return () => this.sourceListeners.delete(cb);
  }

  /** Wire a Web Audio source into the (realtime) driver and notify listeners. */
  connectSource(audioContext: AudioContext, source: AudioNode): void {
    this.ctx = audioContext;
    this.currentSource = source;
    this.driver.connect?.(audioContext, source);
    for (const cb of this.sourceListeners) cb(audioContext, source);
  }

  /** True once a realtime source has been connected. */
  get isLive(): boolean {
    return this.driver.kind === 'realtime' && this.driver.currentTime !== null;
  }

  /** Advance one frame and return the freshly computed features. */
  update(dt: number): AudioFeatures {
    const f = this.features;
    f.frameIndex = this.frame++;
    f.dt = dt;
    f.sampleRate = this.driver.sampleRate;
    const clock = this.driver.currentTime;
    f.time = clock !== null ? clock : f.time + dt;
    f.beat = 0; // drivers raise it for exactly one frame
    this.driver.sample(f, dt);
    return f;
  }

  /** Reset the frame counter (e.g. when restarting a track). */
  reset(): void {
    this.frame = 0;
  }
}
