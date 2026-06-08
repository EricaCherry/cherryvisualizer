import * as THREE from 'three';
import {
  WAVEFORM_SIZE,
  type AudioFeatures,
  type ModeContext,
  type ModeManifest,
  type VisualizerMode,
} from '@cherry/core';
import { hsl } from '../shared/neon';

/**
 * Lissajous Vectorscope — the waveform plotted as an XY phase figure (x = sample,
 * y = a quarter-buffer later), tracing the glowing loops a hardware vectorscope
 * draws. Exercises the `waveform` audio port.
 */
export class Lissajous implements VisualizerMode {
  static readonly manifest: ModeManifest = {
    id: 'scope.lissajous',
    name: 'Lissajous Vectorscope',
    apiVersion: '1.0.0',
    category: 'classic-spectral',
    backend: 'three',
    audioPorts: ['waveform', 'rms', 'centroid', 'beat'],
    deterministic: true,
    license: 'MIT',
    appeal: 4,
    difficulty: 'easy',
    description: 'Waveform XY phase scope tracing neon Lissajous loops.',
  };
  readonly manifest = Lissajous.manifest;

  private readonly m = 512; // points plotted
  private readonly phase = Math.floor(WAVEFORM_SIZE / 4);

  private renderer!: THREE.WebGLRenderer;
  private scene = new THREE.Scene();
  private camera = new THREE.OrthographicCamera(-1, 1, 1, -1, -1, 1);
  private line!: THREE.Line;
  private material!: THREE.LineBasicMaterial;
  private spin = 0;

  init(ctx: ModeContext): void {
    this.renderer = ctx.three.renderer as THREE.WebGLRenderer;
    const geom = new THREE.BufferGeometry();
    geom.setAttribute('position', new THREE.BufferAttribute(new Float32Array(this.m * 3), 3));
    this.material = new THREE.LineBasicMaterial({
      color: hsl(0.5, 0.9, 0.7),
      transparent: true,
      opacity: 0.9,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });
    this.line = new THREE.Line(geom, this.material);
    this.scene.add(this.line);
    this.resize(ctx.width, ctx.height);
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
    const w = features.waveform;
    const n = w.length;
    // auto-gain so the figure always fills the view, however quiet the signal
    let maxAbs = 1e-3;
    for (let i = 0; i < n; i++) {
      const a = Math.abs(w[i]);
      if (a > maxAbs) maxAbs = a;
    }
    const scale = 0.9 / maxAbs;
    const pos = this.line.geometry.getAttribute('position') as THREE.BufferAttribute;
    for (let i = 0; i < this.m; i++) {
      const a = Math.floor((i / this.m) * n);
      const x = w[a] * scale;
      const y = w[(a + this.phase) % n] * scale;
      pos.setXYZ(i, x, y, 0);
    }
    pos.needsUpdate = true;

    this.spin += dt * 0.1;
    this.line.rotation.z = this.spin;
    this.material.opacity = 0.6 + features.rms * 0.6 + (features.beat ? 0.3 : 0);
    this.material.color.setHSL((0.5 + features.centroid / 9000) % 1, 0.9, 0.7);
  }

  render(): void {
    this.renderer.render(this.scene, this.camera);
  }

  dispose(): void {
    this.line.geometry.dispose();
    this.material.dispose();
    this.scene.clear();
  }
}
