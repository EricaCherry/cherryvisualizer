import * as THREE from 'three';
import {
  type AudioFeatures,
  type ModeContext,
  type ModeManifest,
  type VisualizerMode,
} from '@cherry/core';
import { hsl } from '../shared/neon';

/**
 * Starfield Warp — flying through a star tunnel. Warp speed rides RMS, onsets
 * scatter a burst, and the star color shifts with spectral brightness.
 * A GPU THREE.Points field: the particle-backend pattern in miniature.
 */
export class StarfieldWarp implements VisualizerMode {
  static readonly manifest: ModeManifest = {
    id: 'particles.starfield',
    name: 'Starfield Warp',
    apiVersion: '1.0.0',
    category: 'particles',
    backend: 'three',
    audioPorts: ['rms', 'onset', 'beat', 'centroid'],
    deterministic: true,
    license: 'MIT',
    appeal: 4,
    difficulty: 'easy',
    description: 'Warp-speed starfield; speed rides loudness, onsets burst.',
  };
  readonly manifest = StarfieldWarp.manifest;

  private readonly count = 1600;
  private readonly depth = 240;
  private readonly spread = 90;

  private renderer!: THREE.WebGLRenderer;
  private scene = new THREE.Scene();
  private camera = new THREE.PerspectiveCamera(80, 16 / 9, 0.1, 400);
  private points!: THREE.Points;
  private material!: THREE.PointsMaterial;
  private positions!: Float32Array;
  private rng = Math.random;

  init(ctx: ModeContext): void {
    this.renderer = ctx.three.renderer as THREE.WebGLRenderer;
    this.rng = ctx.rng;
    this.camera.position.set(0, 0, 0);
    this.camera.lookAt(0, 0, -1);

    this.positions = new Float32Array(this.count * 3);
    for (let i = 0; i < this.count; i++) this.respawn(i, -this.rng() * this.depth);

    const geom = new THREE.BufferGeometry();
    geom.setAttribute('position', new THREE.BufferAttribute(this.positions, 3));
    this.material = new THREE.PointsMaterial({
      color: hsl(0.55, 0.8, 0.8),
      size: 1.4,
      sizeAttenuation: true,
      transparent: true,
      opacity: 0.95,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });
    this.points = new THREE.Points(geom, this.material);
    this.scene.add(this.points);
    this.resize(ctx.width, ctx.height);
  }

  private respawn(i: number, z: number): void {
    this.positions[i * 3] = (this.rng() - 0.5) * this.spread;
    this.positions[i * 3 + 1] = (this.rng() - 0.5) * this.spread;
    this.positions[i * 3 + 2] = z;
  }

  resize(width: number, height: number): void {
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
  }

  update(features: AudioFeatures, dt: number): void {
    const speed = 40 * (1 + features.rms * 5) + features.onset * 120;
    const burst = features.beat ? this.spread * 0.04 : 0;
    for (let i = 0; i < this.count; i++) {
      let z = this.positions[i * 3 + 2] + speed * dt;
      if (z > 1) {
        this.respawn(i, -this.depth);
        continue;
      }
      this.positions[i * 3 + 2] = z;
      if (burst) {
        this.positions[i * 3] *= 1 + burst * 0.01;
        this.positions[i * 3 + 1] *= 1 + burst * 0.01;
      }
    }
    (this.points.geometry.getAttribute('position') as THREE.BufferAttribute).needsUpdate = true;

    this.material.size = 1.2 + features.rms * 2.5 + (features.beat ? 1.5 : 0);
    this.material.color.setHSL((0.55 + features.centroid / 9000) % 1, 0.8, 0.8);
    this.points.rotation.z += dt * 0.05;
  }

  render(): void {
    this.renderer.render(this.scene, this.camera);
  }

  dispose(): void {
    this.points.geometry.dispose();
    this.material.dispose();
    this.scene.clear();
  }
}
