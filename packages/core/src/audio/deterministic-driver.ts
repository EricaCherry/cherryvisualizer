import type { AudioFeatures } from './features';
import type { AudioDriver } from './driver';
import { mulberry32 } from '../dsp';

/**
 * Phase 0 stub of the deterministic driver. The full version (Phase 6) decodes
 * a track with OfflineAudioContext and indexes precomputed features by frame so
 * exports are bit-identical. This stub synthesizes stable, audio-like features
 * purely from `frameIndex`, which is enough to:
 *   - give CI a repeatable signal (golden-frame tests),
 *   - let modes run with no microphone/file attached,
 *   - prove the bus is source-agnostic.
 */
export class DeterministicDriver implements AudioDriver {
  readonly kind = 'deterministic' as const;
  readonly sampleRate: number;

  private readonly fps: number;
  private readonly rng: () => number;
  private prevFlux = 0;

  constructor(fps = 60, sampleRate = 44100, seed = 0x1234) {
    this.fps = fps;
    this.sampleRate = sampleRate;
    this.rng = mulberry32(seed);
  }

  get currentTime(): number | null {
    return null; // no audio clock; let the bus advance time by dt
  }

  sample(out: AudioFeatures, _dt: number): void {
    const t = out.frameIndex / this.fps;

    // A simple synthetic groove: a kick every ~0.5s plus melodic shimmer.
    const beatPeriod = 0.5; // 120 BPM
    const phase = (t % beatPeriod) / beatPeriod;
    const kick = Math.pow(1 - phase, 3); // sharp attack, decay over the beat

    out.bass = Math.min(1, kick * 0.9 + 0.05);
    out.mid = 0.3 + 0.25 * (0.5 + 0.5 * Math.sin(t * 5.0));
    out.treble = 0.2 + 0.2 * (0.5 + 0.5 * Math.sin(t * 11.0 + 1.0));
    out.rms = Math.min(1, out.bass * 0.6 + out.mid * 0.3 + out.treble * 0.2);
    out.peak = Math.min(1, out.rms * 1.3);
    out.loudnessDb = out.rms > 1e-6 ? 20 * Math.log10(out.rms) : -90;
    out.centroid = 1200 + 800 * out.treble;

    for (let i = 0; i < out.bands.length; i++) {
      const f = i / out.bands.length;
      const env = Math.exp(-f * 2.5); // bass-weighted
      out.bands[i] = Math.min(
        1,
        env * (out.bass * 0.8 + 0.2) + 0.15 * Math.sin(t * 4 + i),
      );
    }
    for (let i = 0; i < out.spectrum.length; i++) {
      out.spectrum[i] = out.bands[Math.floor((i / out.spectrum.length) * out.bands.length)];
    }
    for (let i = 0; i < out.waveform.length; i++) {
      const x = (i / out.waveform.length) * Math.PI * 2;
      // a few detuned harmonics so scope/Lissajous modes trace real figures
      const s =
        Math.sin(x + t * 6.0) * 0.5 +
        Math.sin(x * 2 + t * 4.0) * 0.3 +
        Math.sin(x * 3 + t * 9.0) * 0.2;
      out.waveform[i] = s * (0.45 + out.rms * 0.55);
    }

    const flux = Math.max(0, out.bass - this.prevFlux);
    this.prevFlux = out.bass;
    out.flux = flux;
    out.onset = Math.min(1, flux * 4);

    const beatNow = phase < 1 / this.fps / beatPeriod ? 1 : 0;
    out.beat = beatNow as 0 | 1;
    if (beatNow) out.beatCount++;
    out.bpm = 120;
    out.beatPhase = phase;

    void this.rng; // reserved for future jitter; keeps determinism explicit
  }
}
