import * as THREE from 'three';
import {
  clamp,
  type AudioFeatures,
  type ModeContext,
  type ModeManifest,
  type VisualizerMode,
} from '@cherry/core';
import { glowCircle, hsl, disposeObject } from '../shared/neon';

/**
 * Synthwave Grid Drive — an endless neon grid scrolling toward a glowing sun on
 * the horizon. Speed rides RMS, the grid pulses with bass, beats flash it.
 * A 3D PerspectiveCamera mode: proves the same ABI hosts full 3D scenes.
 */
export class SynthwaveGrid implements VisualizerMode {
  static readonly manifest: ModeManifest = {
    id: 'scene.synthwave',
    name: 'Synthwave Grid Drive',
    apiVersion: '1.0.0',
    category: '3d',
    backend: 'three',
    audioPorts: ['bass', 'rms', 'beat', 'centroid'],
    deterministic: true,
    license: 'MIT',
    appeal: 5,
    difficulty: 'medium',
    description: 'Endless neon grid driving toward a synthwave sun; speed and glow ride the music.',
  };
  readonly manifest = SynthwaveGrid.manifest;

  private readonly depth = 200;
  private readonly cell = 4;

  private renderer!: THREE.WebGLRenderer;
  private scene = new THREE.Scene();
  private camera = new THREE.PerspectiveCamera(70, 16 / 9, 0.1, 400);
  private grid!: THREE.LineSegments;
  private gridMat!: THREE.LineBasicMaterial;
  private sun!: THREE.Group;
  private offset = 0;
  private flash = 0;

  init(ctx: ModeContext): void {
    this.renderer = ctx.three.renderer as THREE.WebGLRenderer;
    this.camera.position.set(0, 3, 0);
    this.camera.lookAt(0, 1.4, -40);

    const halfW = 60;
    const near = hsl(0.92, 1, 0.6);
    const far = hsl(0.74, 1, 0.12);
    const pts: number[] = [];
    const cols: number[] = [];
    const push = (x1: number, z1: number, x2: number, z2: number) => {
      for (const [, z] of [[x1, z1], [x2, z2]] as const) {
        const t = Math.abs(z) / this.depth;
        const c = near.clone().lerp(far, t);
        cols.push(c.r, c.g, c.b);
      }
      pts.push(x1, 0, z1, x2, 0, z2);
    };
    for (let x = -halfW; x <= halfW; x += this.cell) push(x, 0, x, -this.depth);
    for (let z = 0; z >= -this.depth; z -= this.cell) push(-halfW, z, halfW, z);

    const geom = new THREE.BufferGeometry();
    geom.setAttribute('position', new THREE.Float32BufferAttribute(pts, 3));
    geom.setAttribute('color', new THREE.Float32BufferAttribute(cols, 3));
    this.gridMat = new THREE.LineBasicMaterial({
      vertexColors: true,
      transparent: true,
      opacity: 0.9,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });
    this.grid = new THREE.LineSegments(geom, this.gridMat);
    this.scene.add(this.grid);

    this.sun = glowCircle(14, hsl(0.04, 1, 0.6), { haloScale: 2.4, haloOpacity: 0.7 });
    this.sun.position.set(0, 11, -this.depth * 0.82);
    this.scene.add(this.sun);

    this.resize(ctx.width, ctx.height);
  }

  resize(width: number, height: number): void {
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
  }

  update(features: AudioFeatures, dt: number): void {
    if (features.beat) this.flash = 1;
    this.flash = Math.max(0, this.flash - dt * 3);

    const speed = 18 * (1 + features.rms * 2 + features.bass * 1.5);
    this.offset = (this.offset + speed * dt) % this.cell;
    this.grid.position.z = this.offset;

    this.gridMat.opacity = clamp(0.55 + features.bass * 0.6 + this.flash * 0.5, 0, 1.4);
    const hue = 0.78 + features.centroid / 12000;
    this.gridMat.color.setHSL(((hue % 1) + 1) % 1, 1, 0.6);

    const s = 1 + features.bass * 0.25 + this.flash * 0.15;
    this.sun.scale.setScalar(s);
    this.camera.position.y = 3 + features.bass * 0.6;
    this.camera.lookAt(0, 1.4, -40);
  }

  render(): void {
    this.renderer.render(this.scene, this.camera);
  }

  dispose(): void {
    disposeObject(this.scene);
    this.scene.clear();
  }
}
