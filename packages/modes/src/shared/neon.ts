import * as THREE from 'three';

/**
 * Cheap, dependency-free "neon glow": a bright additive core plus a soft,
 * radial-gradient additive halo. On a near-black background this reads as bloom
 * without a post-processing pass — robust everywhere, including headless WebGL.
 * (Real bloom via the WebGPU RenderPipeline is a later-phase upgrade.)
 */

export interface GlowOptions {
  coreOpacity?: number;
  haloScale?: number;
  haloOpacity?: number;
  /** Use the soft radial halo (good for isolated shapes: ball, paddle). */
  soft?: boolean;
}

let softTex: THREE.Texture | null = null;
/** Lazily-built radial gradient (white center → transparent edge). */
function softGlowTexture(): THREE.Texture {
  if (softTex) return softTex;
  const s = 128;
  const cv = document.createElement('canvas');
  cv.width = cv.height = s;
  const g = cv.getContext('2d')!;
  const grd = g.createRadialGradient(s / 2, s / 2, 0, s / 2, s / 2, s / 2);
  grd.addColorStop(0, 'rgba(255,255,255,1)');
  grd.addColorStop(0.25, 'rgba(255,255,255,0.6)');
  grd.addColorStop(1, 'rgba(255,255,255,0)');
  g.fillStyle = grd;
  g.fillRect(0, 0, s, s);
  softTex = new THREE.CanvasTexture(cv);
  softTex.colorSpace = THREE.SRGBColorSpace;
  return softTex;
}

function additiveMaterial(color: THREE.ColorRepresentation, opacity: number): THREE.MeshBasicMaterial {
  return new THREE.MeshBasicMaterial({
    color,
    transparent: true,
    opacity,
    blending: THREE.AdditiveBlending,
    depthWrite: false,
    depthTest: false,
  });
}

function softMaterial(color: THREE.ColorRepresentation, opacity: number): THREE.MeshBasicMaterial {
  const m = additiveMaterial(color, opacity);
  m.map = softGlowTexture();
  return m;
}

/** A glowing rectangle (centered at its origin). */
export function glowRect(
  width: number,
  height: number,
  color: THREE.ColorRepresentation,
  opts: GlowOptions = {},
): THREE.Group {
  const { coreOpacity = 1, haloScale = opts.soft ? 3 : 2.4, haloOpacity = opts.soft ? 0.6 : 0.18, soft = false } = opts;
  const group = new THREE.Group();
  const halo = new THREE.Mesh(
    new THREE.PlaneGeometry(width * haloScale, height * haloScale),
    soft ? softMaterial(color, haloOpacity) : additiveMaterial(color, haloOpacity),
  );
  const core = new THREE.Mesh(new THREE.PlaneGeometry(width, height), additiveMaterial(color, coreOpacity));
  group.add(halo, core);
  return group;
}

/** A glowing disc with a soft halo. */
export function glowCircle(
  radius: number,
  color: THREE.ColorRepresentation,
  opts: GlowOptions = {},
): THREE.Group {
  const { coreOpacity = 1, haloScale = 4, haloOpacity = 0.85 } = opts;
  const group = new THREE.Group();
  const halo = new THREE.Mesh(
    new THREE.PlaneGeometry(radius * 2 * haloScale, radius * 2 * haloScale),
    softMaterial(color, haloOpacity),
  );
  const core = new THREE.Mesh(new THREE.CircleGeometry(radius, 32), additiveMaterial(color, coreOpacity));
  group.add(halo, core);
  return group;
}

/** Set the color on every material in a glow group. */
export function setGlowColor(group: THREE.Group, color: THREE.ColorRepresentation): void {
  group.traverse((o) => {
    const mesh = o as THREE.Mesh;
    if (mesh.isMesh) (mesh.material as THREE.MeshBasicMaterial).color.set(color);
  });
}

/** Set halo (child 0) and core (child 1) opacities — handy for pulsing. */
export function setGlowIntensity(group: THREE.Group, coreOpacity: number, haloOpacity: number): void {
  const halo = group.children[0] as THREE.Mesh | undefined;
  const core = group.children[1] as THREE.Mesh | undefined;
  if (halo) (halo.material as THREE.MeshBasicMaterial).opacity = haloOpacity;
  if (core) (core.material as THREE.MeshBasicMaterial).opacity = coreOpacity;
}

/** HSL → THREE.Color convenience (h,s,l in 0..1). */
export function hsl(h: number, s: number, l: number): THREE.Color {
  return new THREE.Color().setHSL(((h % 1) + 1) % 1, s, l);
}

/** Recursively dispose geometries and materials in a subtree. */
export function disposeObject(root: THREE.Object3D): void {
  root.traverse((o) => {
    const mesh = o as THREE.Mesh;
    if (mesh.isMesh) {
      mesh.geometry?.dispose();
      const m = mesh.material;
      if (Array.isArray(m)) m.forEach((mm) => mm.dispose());
      else m?.dispose();
    }
    const line = o as unknown as THREE.Line;
    if ((line as THREE.Line).isLine) {
      line.geometry?.dispose();
      (line.material as THREE.Material)?.dispose();
    }
  });
}
