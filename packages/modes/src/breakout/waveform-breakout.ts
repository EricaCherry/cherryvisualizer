import * as THREE from 'three';
import Matter from 'matter-js';
import {
  BAND_COUNT,
  clamp,
  lerp,
  type AudioFeatures,
  type ModeContext,
  type ModeManifest,
  type VisualizerMode,
} from '@cherry/core';
import { disposeObject } from '../shared/neon';

interface Brick {
  body: Matter.Body;
  mesh: THREE.Mesh;
  alive: boolean;
  bandIndex: number;
  baseColor: THREE.Color;
  flash: number;
}

/**
 * Audio Breakout (3D) — the music plays the game, rendered as a tangible,
 * lit scene with real materials and soft shadows (no glow).
 *   - the spectrum builds a brick wall that rises and falls like a 3D EQ;
 *   - the live waveform is a scope line that physically shoves the ball;
 *   - bass + RMS set ball speed, a beat kicks it and rebuilds the wall;
 *   - an auto-tracking paddle keeps the rally alive.
 *
 * Physics is Matter.js (2D); the 2D field is laid on the 3D ground plane:
 *   world = (px - VW/2,  height,  -py).
 */
export class WaveformBreakout implements VisualizerMode {
  static readonly manifest: ModeManifest = {
    id: 'game.breakout',
    name: 'Waveform Breakout',
    apiVersion: '1.0.0',
    category: 'arcade',
    backend: 'three',
    audioPorts: ['bass', 'rms', 'beat', 'bands', 'waveform'],
    deterministic: true,
    license: 'MIT',
    appeal: 5,
    difficulty: 'medium',
    description: 'A 3D Breakout where the spectrum builds the bricks and the waveform shoves the ball.',
  };
  readonly manifest = WaveformBreakout.manifest;

  // field (virtual units; height fixed, width follows aspect)
  private readonly VH = 90;
  private VW = 160;
  private aspect = 16 / 9;
  private readonly paddleYpos = 9; // physics y near the camera
  private readonly midY = 40; // waveform scope line (physics y)
  private readonly cols = 18;
  private readonly rows = 4;
  private readonly brickBaseH = 5;

  // physics tuning (units per fixed 1/60s step)
  private readonly baseSpeed = 0.95;
  private readonly ballRadius = 1.9;
  private readonly paddleW = 22;
  private readonly paddleDepth = 4;
  private readonly stepMs = 1000 / 60;

  private renderer!: THREE.WebGLRenderer;
  private scene = new THREE.Scene();
  private camera = new THREE.PerspectiveCamera(52, 16 / 9, 0.5, 600);
  private field = new THREE.Group();
  private keyLight!: THREE.DirectionalLight;

  private engine!: Matter.Engine;
  private world!: Matter.World;
  private ball!: Matter.Body;
  private ballMesh!: THREE.Mesh;
  private paddle!: Matter.Body;
  private paddleMesh!: THREE.Mesh;
  private bricks: Brick[] = [];
  private wave!: THREE.Line;
  private waveN = 200;

  private paddleX = 80;
  private kick = 1;
  private score = 0;

  init(ctx: ModeContext): void {
    this.renderer = ctx.three.renderer as THREE.WebGLRenderer;
    this.renderer.shadowMap.enabled = true;
    this.renderer.shadowMap.type = THREE.PCFSoftShadowMap;
    this.aspect = ctx.width / ctx.height;

    this.scene.background = new THREE.Color(0x0d1018);
    this.scene.fog = new THREE.Fog(0x0d1018, 120, 320);
    this.scene.add(this.field);

    // lights
    this.scene.add(new THREE.HemisphereLight(0x9fb4ff, 0x1a1d28, 0.7));
    const key = new THREE.DirectionalLight(0xffffff, 1.5);
    key.castShadow = true;
    key.shadow.mapSize.set(2048, 2048);
    key.shadow.bias = -0.0006;
    key.shadow.camera.near = 1;
    key.shadow.camera.far = 400;
    const sc = key.shadow.camera as THREE.OrthographicCamera;
    sc.left = -120;
    sc.right = 120;
    sc.top = 120;
    sc.bottom = -120;
    sc.updateProjectionMatrix();
    this.keyLight = key;
    this.scene.add(key, key.target);

    this.engine = Matter.Engine.create();
    this.engine.gravity.x = 0;
    this.engine.gravity.y = 0;
    this.world = this.engine.world;

    Matter.Events.on(this.engine, 'collisionStart', (e: Matter.IEventCollision<Matter.Engine>) => {
      for (const pair of e.pairs) {
        const labels = [pair.bodyA.label, pair.bodyB.label];
        if (!labels.includes('ball')) continue;
        const other = labels.find((l) => l.startsWith('brick:'));
        if (other) this.killBrick(parseInt(other.slice(6), 10));
      }
    });

    this.buildField();
  }

  resize(width: number, height: number): void {
    this.aspect = width / height;
    this.camera.aspect = this.aspect;
    this.camera.updateProjectionMatrix();
    this.buildField();
  }

  /** physics (px,py) → 3D world position on the ground plane. */
  private wx(px: number): number {
    return px - this.VW / 2;
  }
  private wz(py: number): number {
    return -py;
  }

  private buildField(): void {
    this.VW = this.VH * this.aspect;

    if (this.world) Matter.Composite.clear(this.world, false, true);
    disposeObject(this.field);
    this.field.clear();
    this.bricks = [];

    // camera framing
    this.camera.position.set(0, this.VH * 0.72, this.VH * 0.96);
    this.camera.lookAt(0, 3, this.wz(this.VH * 0.5));
    this.keyLight.position.set(this.VW * 0.25, this.VH * 1.2, this.VH * 0.4);
    this.keyLight.target.position.set(0, 0, this.wz(this.VH * 0.5));
    this.keyLight.target.updateMatrixWorld();

    // floor
    const floor = new THREE.Mesh(
      new THREE.PlaneGeometry(this.VW + 40, this.VH + 40),
      new THREE.MeshStandardMaterial({ color: 0x171b26, roughness: 0.95, metalness: 0.0 }),
    );
    floor.rotation.x = -Math.PI / 2;
    floor.position.set(0, 0, this.wz(this.VH / 2));
    floor.receiveShadow = true;
    this.field.add(floor);

    // side rails (framing)
    const railMat = new THREE.MeshStandardMaterial({ color: 0x2a3142, roughness: 0.6, metalness: 0.2 });
    for (const sx of [0, this.VW]) {
      const rail = new THREE.Mesh(new THREE.BoxGeometry(1.6, 4, this.VH), railMat);
      rail.position.set(this.wx(sx), 2, this.wz(this.VH / 2));
      rail.castShadow = true;
      rail.receiveShadow = true;
      this.field.add(rail);
    }

    // physics walls (invisible)
    const T = 12;
    const wall = (x: number, y: number, w: number, h: number) =>
      Matter.Bodies.rectangle(x, y, w, h, { isStatic: true, restitution: 1, label: 'wall' });
    Matter.Composite.add(this.world, [
      wall(this.VW / 2, this.VH + T / 2, this.VW + T * 2, T),
      wall(-T / 2, this.VH / 2, T, this.VH * 2),
      wall(this.VW + T / 2, this.VH / 2, T, this.VH * 2),
    ]);

    // paddle
    this.paddleX = this.VW / 2;
    this.paddle = Matter.Bodies.rectangle(this.paddleX, this.paddleYpos, this.paddleW, this.paddleDepth, {
      isStatic: true,
      restitution: 1.02,
      label: 'paddle',
    });
    Matter.Composite.add(this.world, this.paddle);
    this.paddleMesh = new THREE.Mesh(
      new THREE.BoxGeometry(this.paddleW, 3, this.paddleDepth),
      new THREE.MeshStandardMaterial({ color: 0x3f7bd6, roughness: 0.35, metalness: 0.25 }),
    );
    this.paddleMesh.castShadow = true;
    this.paddleMesh.receiveShadow = true;
    this.field.add(this.paddleMesh);

    // ball
    this.ball = Matter.Bodies.circle(this.VW / 2, this.VH / 2, this.ballRadius, {
      restitution: 1,
      friction: 0,
      frictionAir: 0,
      frictionStatic: 0,
      inertia: Infinity,
      label: 'ball',
    });
    Matter.Composite.add(this.world, this.ball);
    this.launchBall();
    this.ballMesh = new THREE.Mesh(
      new THREE.SphereGeometry(this.ballRadius, 28, 20),
      new THREE.MeshStandardMaterial({ color: 0xf4f6ff, roughness: 0.15, metalness: 0.35 }),
    );
    this.ballMesh.castShadow = true;
    this.field.add(this.ballMesh);

    // bricks
    const margin = this.VW * 0.07;
    const areaW = this.VW - margin * 2;
    const slot = areaW / this.cols;
    const brickW = slot * 0.86;
    const top = this.VH * 0.9;
    const bottom = this.VH * 0.55;
    const rowGap = (top - bottom) / this.rows;
    const brickDepth = rowGap * 0.7;
    for (let r = 0; r < this.rows; r++) {
      for (let c = 0; c < this.cols; c++) {
        const idx = this.bricks.length;
        const px = margin + (c + 0.5) * slot;
        const py = bottom + (r + 0.5) * rowGap;
        const bandIndex = Math.floor((c / this.cols) * BAND_COUNT);
        const baseColor = new THREE.Color().setHSL(0.0 + (c / this.cols) * 0.62, 0.55, 0.5);
        const body = Matter.Bodies.rectangle(px, py, brickW, brickDepth, {
          isStatic: true,
          restitution: 1,
          label: `brick:${idx}`,
        });
        Matter.Composite.add(this.world, body);
        const mesh = new THREE.Mesh(
          new THREE.BoxGeometry(brickW, this.brickBaseH, brickDepth),
          new THREE.MeshStandardMaterial({ color: baseColor.clone(), roughness: 0.5, metalness: 0.15 }),
        );
        mesh.castShadow = true;
        mesh.receiveShadow = true;
        mesh.position.set(this.wx(px), this.brickBaseH / 2, this.wz(py));
        this.field.add(mesh);
        this.bricks.push({ body, mesh, alive: true, bandIndex, baseColor, flash: 0 });
      }
    }

    // waveform scope line (clean accent, no glow)
    const geom = new THREE.BufferGeometry();
    geom.setAttribute('position', new THREE.BufferAttribute(new Float32Array(this.waveN * 3), 3));
    this.wave = new THREE.Line(
      geom,
      new THREE.LineBasicMaterial({ color: 0x5cc8ff, transparent: true, opacity: 0.85 }),
    );
    this.field.add(this.wave);
  }

  private launchBall(): void {
    const angle = (Math.random() * 0.6 + 0.2) * Math.PI * (Math.random() < 0.5 ? 1 : -1);
    Matter.Body.setPosition(this.ball, { x: this.VW / 2, y: this.VH * 0.55 });
    Matter.Body.setVelocity(this.ball, {
      x: Math.cos(angle) * this.baseSpeed,
      y: -Math.abs(Math.sin(angle)) * this.baseSpeed,
    });
  }

  private killBrick(idx: number): void {
    const b = this.bricks[idx];
    if (!b || !b.alive) return;
    b.alive = false;
    Matter.Body.set(b.body, 'isSensor', true);
    b.mesh.visible = false;
    this.score++;
  }

  private rebuildWall(): void {
    for (const b of this.bricks) {
      if (!b.alive) {
        b.alive = true;
        Matter.Body.set(b.body, 'isSensor', false);
        b.mesh.visible = true;
      }
      b.flash = 1;
    }
  }

  update(features: AudioFeatures, dt: number): void {
    if (features.beat) {
      this.rebuildWall();
      this.kick = 1.7;
    }
    this.kick = lerp(this.kick, 1, Math.min(1, dt * 2.5));
    this.keyLight.intensity = 1.5 + (features.beat ? 0.7 : 0) + features.bass * 0.3;

    // paddle auto-tracks the ball
    const targetX = clamp(this.ball.position.x, this.paddleW / 2, this.VW - this.paddleW / 2);
    this.paddleX = lerp(this.paddleX, targetX, Math.min(1, dt * 9));
    Matter.Body.setPosition(this.paddle, { x: this.paddleX, y: this.paddleYpos });

    // the waveform shoves the ball as it crosses the scope line
    if (Math.abs(this.ball.position.y - this.midY) < 3.0) {
      const wi = Math.floor((this.ball.position.x / this.VW) * features.waveform.length);
      const sample = features.waveform[clamp(wi, 0, features.waveform.length - 1)] || 0;
      const v = this.ball.velocity;
      Matter.Body.setVelocity(this.ball, { x: v.x, y: v.y + sample * 0.6 });
    }

    Matter.Engine.update(this.engine, this.stepMs);

    // normalize ball speed to an audio-driven target
    const target = this.baseSpeed * (1 + features.bass * 1.6 + features.rms * 0.4) * this.kick;
    const v = this.ball.velocity;
    let sp = Math.hypot(v.x, v.y);
    if (sp < 1e-3) {
      this.launchBall();
    } else {
      let vy = v.y;
      const minVy = target * 0.25;
      if (Math.abs(vy) < minVy) {
        vy = (vy < 0 ? -1 : 1) * minVy;
        sp = Math.hypot(v.x, vy);
      }
      const s = target / sp;
      Matter.Body.setVelocity(this.ball, { x: v.x * s, y: vy * s });
    }
    if (this.ball.position.y < -4) this.launchBall();

    // sync meshes
    this.ballMesh.position.set(this.wx(this.ball.position.x), this.ballRadius, this.wz(this.ball.position.y));
    this.ballMesh.scale.setScalar(1 + (features.beat ? 0.25 : 0) + features.rms * 0.15);
    this.paddleMesh.position.set(this.wx(this.paddle.position.x), 1.5, this.wz(this.paddle.position.y));

    for (const b of this.bricks) {
      if (!b.alive) continue;
      const energy = features.bands[b.bandIndex] ?? 0;
      b.flash = Math.max(0, b.flash - dt * 3);
      const h = 1 + energy * 2.4; // 3D equalizer rise
      b.mesh.scale.y = h;
      b.mesh.position.y = (this.brickBaseH * h) / 2;
      const mat = b.mesh.material as THREE.MeshStandardMaterial;
      mat.color.copy(b.baseColor).offsetHSL(0, 0, energy * 0.25 + b.flash * 0.3);
    }

    // waveform scope line across the field at midY
    const pos = this.wave.geometry.getAttribute('position') as THREE.BufferAttribute;
    for (let i = 0; i < this.waveN; i++) {
      const wi = Math.floor((i / this.waveN) * features.waveform.length);
      const px = (i / (this.waveN - 1)) * this.VW;
      pos.setXYZ(i, this.wx(px), 2 + (features.waveform[wi] || 0) * 6, this.wz(this.midY));
    }
    pos.needsUpdate = true;
  }

  render(): void {
    this.renderer.render(this.scene, this.camera);
  }

  dispose(): void {
    if (this.engine) {
      Matter.World.clear(this.world, false);
      Matter.Engine.clear(this.engine);
    }
    disposeObject(this.scene);
    this.scene.clear();
    this.bricks = [];
  }
}
