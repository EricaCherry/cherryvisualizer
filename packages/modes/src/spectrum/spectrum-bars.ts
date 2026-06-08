import * as THREE from 'three';
import {
  BAND_COUNT,
  type AudioFeatures,
  type ModeContext,
  type ModeManifest,
  type VisualizerMode,
} from '@cherry/core';
import { glowRect, hsl, setGlowColor, disposeObject } from '../shared/neon';

/**
 * Classic log-frequency neon spectrum bars with peak-hold. The simplest mode in
 * the repo — its job is to prove a second backend:'three' mode loads cleanly
 * through the same ABI as the flagship.
 */
export class SpectrumBars implements VisualizerMode {
  static readonly manifest: ModeManifest = {
    id: 'spectrum.bars',
    name: 'Spectrum Bars',
    apiVersion: '1.0.0',
    category: 'classic-spectral',
    backend: 'three',
    audioPorts: ['bands', 'beat', 'rms'],
    deterministic: true,
    license: 'MIT',
    appeal: 4,
    difficulty: 'easy',
    description: 'Log-frequency neon spectrum bars with peak-hold.',
  };
  readonly manifest = SpectrumBars.manifest;

  private renderer!: THREE.WebGLRenderer;
  private scene = new THREE.Scene();
  private camera = new THREE.OrthographicCamera(0, 1, 1, 0, -1, 1);
  private readonly n = BAND_COUNT;
  private bars: THREE.Group[] = [];
  private caps: THREE.Group[] = [];
  private peaks = new Float32Array(BAND_COUNT);
  private aspect = 16 / 9;

  init(ctx: ModeContext): void {
    this.renderer = ctx.three.renderer as THREE.WebGLRenderer;
    this.aspect = ctx.width / ctx.height;

    for (let i = 0; i < this.n; i++) {
      const color = hsl(0.55 - (i / this.n) * 0.55, 0.9, 0.55);
      const bar = glowRect(1, 1, color, { haloOpacity: 0.12 });
      const cap = glowRect(1, 1, hsl(0.55 - (i / this.n) * 0.55, 0.4, 0.85), {
        haloOpacity: 0.3,
      });
      this.bars.push(bar);
      this.caps.push(cap);
      this.scene.add(bar, cap);
    }
    this.layout();
  }

  resize(width: number, height: number): void {
    this.aspect = width / height;
    this.layout();
  }

  private layout(): void {
    this.camera.left = 0;
    this.camera.right = this.aspect;
    this.camera.top = 1;
    this.camera.bottom = 0;
    this.camera.updateProjectionMatrix();

    const slot = this.aspect / this.n;
    const barW = slot * 0.7;
    for (let i = 0; i < this.n; i++) {
      const x = (i + 0.5) * slot;
      this.bars[i].position.x = x;
      this.bars[i].scale.x = barW;
      this.caps[i].position.x = x;
      this.caps[i].scale.set(barW, 0.012, 1);
    }
  }

  update(features: AudioFeatures, dt: number): void {
    const beatBoost = features.beat ? 1.15 : 1;
    for (let i = 0; i < this.n; i++) {
      const h = Math.max(0.004, Math.min(0.95, features.bands[i] * 0.95 * beatBoost));
      this.bars[i].scale.y = h;
      this.bars[i].position.y = h / 2;

      // peak-hold with gravity fall
      if (h > this.peaks[i]) this.peaks[i] = h;
      else this.peaks[i] = Math.max(h, this.peaks[i] - dt * 0.6);
      this.caps[i].position.y = this.peaks[i];

      if (features.beat) setGlowColor(this.bars[i], hsl(0.95 - (i / this.n) * 0.55, 0.9, 0.6));
      else setGlowColor(this.bars[i], hsl(0.55 - (i / this.n) * 0.55, 0.9, 0.55));
    }
  }

  render(): void {
    this.renderer.render(this.scene, this.camera);
  }

  dispose(): void {
    disposeObject(this.scene);
    this.scene.clear();
    this.bars = [];
    this.caps = [];
  }
}
