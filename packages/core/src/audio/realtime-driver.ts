import type { AudioFeatures } from './features';
import type { AudioDriver } from './driver';
import {
  BeatDetector,
  bandEnergy,
  computeLogBands,
  resampleInto,
} from '../dsp';

/**
 * Realtime audio features from a Web Audio AnalyserNode. Hand-rolled DSP keeps
 * @cherry/core dependency-free; Meyda (mfcc/chroma) can augment this later
 * behind the same AudioDriver interface.
 */
export class RealtimeDriver implements AudioDriver {
  readonly kind = 'realtime' as const;

  private ctx: AudioContext | null = null;
  private analyser: AnalyserNode | null = null;
  // ArrayBuffer-backed (not Shared) so the Web Audio analyser APIs accept them.
  private freq: Uint8Array<ArrayBuffer> = new Uint8Array(0);
  private time: Float32Array<ArrayBuffer> = new Float32Array(0);
  private prevMag: Float32Array = new Float32Array(0);
  private readonly beat = new BeatDetector();

  connect(audioContext: AudioContext, source: AudioNode): void {
    const analyser = audioContext.createAnalyser();
    analyser.fftSize = 4096; // sub-100Hz resolution; separates low bass notes
    analyser.smoothingTimeConstant = 0; // we smooth deliberately downstream
    source.connect(analyser);

    this.ctx = audioContext;
    this.analyser = analyser;
    const bins = analyser.frequencyBinCount;
    this.freq = new Uint8Array(bins);
    this.time = new Float32Array(analyser.fftSize);
    this.prevMag = new Float32Array(bins);
  }

  get currentTime(): number | null {
    return this.ctx ? this.ctx.currentTime : null;
  }

  get sampleRate(): number {
    return this.ctx ? this.ctx.sampleRate : 44100;
  }

  sample(out: AudioFeatures, _dt: number): void {
    const an = this.analyser;
    const ctx = this.ctx;
    if (!an || !ctx) return;

    an.getByteFrequencyData(this.freq);
    an.getFloatTimeDomainData(this.time);

    const nyquist = ctx.sampleRate / 2;
    const bins = this.freq.length;

    // Band energies.
    computeLogBands(this.freq, out.bands, nyquist);
    out.bass = bandEnergy(this.freq, 20, 250, nyquist);
    out.mid = bandEnergy(this.freq, 250, 2000, nyquist);
    out.treble = bandEnergy(this.freq, 2000, 11000, nyquist);

    // Time-domain RMS / peak.
    let sumSq = 0;
    let peak = 0;
    for (let i = 0; i < this.time.length; i++) {
      const v = this.time[i];
      sumSq += v * v;
      const a = v < 0 ? -v : v;
      if (a > peak) peak = a;
    }
    const rms = Math.sqrt(sumSq / (this.time.length || 1));
    out.rms = rms;
    out.peak = peak;
    out.loudnessDb = rms > 1e-6 ? 20 * Math.log10(rms) : -90;

    // Spectral centroid + flux.
    let magSum = 0;
    let weighted = 0;
    let flux = 0;
    for (let i = 0; i < bins; i++) {
      const m = this.freq[i] / 255;
      magSum += m;
      weighted += m * ((i / bins) * nyquist);
      const d = m - this.prevMag[i];
      if (d > 0) flux += d;
      this.prevMag[i] = m;
    }
    out.centroid = magSum > 1e-6 ? weighted / magSum : 0;
    out.flux = flux / bins;

    // Vectors for modes that want raw data.
    resampleInto(this.freq, out.spectrum, 1 / 255);
    resampleInto(this.time, out.waveform, 1);

    // Rhythm — bias the onset energy toward the low end where beats live.
    const energy = out.bass * 0.6 + rms * 0.4;
    const res = this.beat.update(energy, out.flux, out.time);
    out.beat = res.beat;
    out.onset = res.onset;
    out.bpm = res.bpm;
    out.beatPhase = res.beatPhase;
    if (res.beat) out.beatCount++;
  }
}
