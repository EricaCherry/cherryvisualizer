import * as THREE from 'three';
import {
  BAND_COUNT,
  clamp,
  type AudioFeatures,
  type ModeContext,
  type ModeManifest,
  type VisualizerMode,
} from '@cherry/core';
import { glowRect, glowCircle, hsl, setGlowColor, setGlowIntensity, disposeObject } from '../shared/neon';

/**
 * Circular Radial Bars — the spectrum wrapped into a mirrored starburst with a
 * pulsing core. Bands drive bar length, bass drives the core glow, the whole
 * ring rotates and flashes on the beat.
 */
export class RadialBars implements VisualizerMode {
  static readonly manifest: ModeManifest = {
    id: 'spectrum.radial',
    name: 'Circular Radial Bars',
    apiVersion: '1.0.0',
    category: 'classic-spectral',
    backend: 'three',
    audioPorts: ['bands', 'bass', 'beat', 'centroid'],
    deterministic: true,
    license: 'MIT',
    appeal: 5,
    difficulty: 'medium',
    description: 'Mirrored spectrum starburst with a bass-pulsed core.',
  };
  readonly manifest = RadialBars.manifest;

  private readonly n = BAND_COUNT * 2; // mirrored
  private readonly r0 = 0.22; // inner radius
  private readonly maxLen = 0.55;

  private renderer!: THREE.WebGLRenderer;
  private scene = new THREE.Scene();
  private camera = new THREE.OrthographicCamera(-1, 1, 1, -1, -1, 1);
  private ring = new THREE.Group();
  private bars: THREE.Group[] = [];
  private core!: THREE.Group;
  private spin = 0;

  init(ctx: ModeContext): void {
    this.renderer = ctx.three.renderer as THREE.WebGLRenderer;

    this.core = glowCircle(this.r0 * 0.7, hsl(0.95, 0.9, 0.6), { haloScale: 3.2, haloOpacity: 0.6 });
    this.scene.add(this.core);

    for (let i = 0; i < this.n; i++) {
      const hue = 0.58 - (this.bandFor(i) / BAND_COUNT) * 0.6;
      const bar = glowRect(0.018, 0.1, hsl(hue, 0.9, 0.55), { haloOpacity: 0.16 });
      this.bars.push(bar);
      this.ring.add(bar);
    }
    this.scene.add(this.ring);
    this.resize(ctx.width, ctx.height);
  }

  private bandFor(i: number): number {
    return i < BAND_COUNT ? i : this.n - 1 - i; // mirror
  }

  resize(width: number, height: number): void {
    const a = width / height;
    this.camera.left = -a;
    this.camera.right = a;
    this.camera.top = 1;
    this.camera.bottom = -1;
    this.camera.updateProjectionMatrix();
  }

  update(features: AudioFeatures, dt: number): void {
    this.spin += dt * (0.15 + features.bass * 0.5);
    this.ring.rotation.z = this.spin;

    for (let i = 0; i < this.n; i++) {
      const energy = features.bands[this.bandFor(i)] ?? 0;
      const len = clamp(0.02 + energy * this.maxLen, 0.02, this.maxLen);
      const ang = (i / this.n) * Math.PI * 2;
      const rMid = this.r0 + len / 2;
      const bar = this.bars[i];
      bar.position.set(Math.cos(ang) * rMid, Math.sin(ang) * rMid, 0);
      bar.rotation.z = ang + Math.PI / 2;
      bar.scale.set(1, len / 0.1, 1);
      setGlowIntensity(bar, clamp(0.5 + energy, 0, 1.5), 0.12 + energy * 0.3);
      if (features.beat) setGlowColor(bar, hsl(0.95 - (this.bandFor(i) / BAND_COUNT) * 0.5, 0.9, 0.62));
      else setGlowColor(bar, hsl(0.58 - (this.bandFor(i) / BAND_COUNT) * 0.6, 0.9, 0.55));
    }

    const cs = 1 + features.bass * 0.8 + (features.beat ? 0.5 : 0);
    this.core.scale.setScalar(cs);
    setGlowColor(this.core, hsl(0.95 + features.centroid / 12000, 0.9, 0.6));
  }

  render(): void {
    this.renderer.render(this.scene, this.camera);
  }

  dispose(): void {
    disposeObject(this.scene);
    this.scene.clear();
    this.bars = [];
  }
}
