//! A small world-space PBR-lite material for the 3D modes (Beat Surfer ribbon,
//! Rail Shooter corridor), with procedurally-baked normal + material maps.
//!
//! macroquad gives a custom material only two matrix uniforms — `Model`
//! (identity for our direct draws) and `Projection` (the COMBINED view·proj from
//! `Camera3D`) — so there is no view matrix and the camera is not at the origin
//! of any usable space. Lighting is therefore done in WORLD space: meshes are
//! already in world coords, we pass the world position + a real per-vertex world
//! normal through varyings, and feed the camera/light as our own uniforms. The
//! tangent frame for normal-mapping is reconstructed from the world normal + a
//! world-up reference (no dFdx, which isn't guaranteed under `#version 100`).
//!
//! The baked maps are NEUTRAL grey and tinted in-shader by the per-vertex color
//! (the theme grade), so a theme switch re-skins the lit surfaces for free and
//! the maps never need rebaking. Cook-Torrance GGX specular + a fresnel rim and
//! a beat-driven emissive make the surface throb with the music.

use std::cell::RefCell;

use macroquad::miniquad::TextureWrap;
use macroquad::prelude::*;

use crate::style::hash01;

const VERT: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
attribute vec4 normal;
varying highp vec3 wpos;
varying highp vec3 wnrm;
varying highp vec2 uv;
varying lowp vec4 tint;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    vec4 wp = Model * vec4(position, 1.0);
    wpos = wp.xyz;
    wnrm = normal.xyz;
    uv = texcoord;
    tint = color0 / 255.0;
    gl_Position = Projection * wp;
}
"#;

const FRAG: &str = r#"#version 100
precision highp float;
varying highp vec3 wpos;
varying highp vec3 wnrm;
varying highp vec2 uv;
varying lowp vec4 tint;
uniform sampler2D NormalTex;
uniform sampler2D MatTex;
uniform vec3 CamPos;
uniform vec3 LightDir;
uniform vec3 LightColor;
uniform vec3 Ambient;
uniform vec3 Horizon;
uniform vec2 MetalRough;   // x = metallic scale, y = roughness scale
uniform vec2 Tile;
uniform float Pulse;        // bass/beat 0..1 -> rim + emissive throb
void main() {
    vec2 tuv = uv * Tile;
    vec3 N = normalize(wnrm);
    // World-reference tangent frame (no precomputed tangents, no derivatives).
    vec3 up = abs(N.y) < 0.99 ? vec3(0.0, 1.0, 0.0) : vec3(1.0, 0.0, 0.0);
    vec3 T = normalize(cross(up, N));
    vec3 B = cross(N, T);
    vec3 nt = texture2D(NormalTex, tuv).xyz * 2.0 - 1.0;
    N = normalize(nt.x * T + nt.y * B + nt.z * N);

    vec3 mtl = texture2D(MatTex, tuv).rgb;          // r = grey, g = rough, b = metal
    vec3 albedo = tint.rgb * (0.45 + 0.55 * mtl.r);
    float rough = clamp(mtl.g * MetalRough.y, 0.05, 1.0);
    float metal = clamp(mtl.b * MetalRough.x, 0.0, 1.0);

    vec3 V = normalize(CamPos - wpos);
    vec3 L = normalize(-LightDir);
    vec3 H = normalize(L + V);
    float NdL = max(dot(N, L), 0.0);
    float NdV = max(dot(N, V), 1e-3);
    float NdH = max(dot(N, H), 0.0);
    float a = rough * rough;
    float a2 = a * a;
    float dn = NdH * NdH * (a2 - 1.0) + 1.0;
    float D = a2 / (3.14159 * dn * dn + 1e-5);       // GGX NDF
    float kg = (rough + 1.0);
    kg = kg * kg / 8.0;
    float G = (NdV / (NdV * (1.0 - kg) + kg)) * (NdL / (NdL * (1.0 - kg) + kg));
    vec3 F0 = mix(vec3(0.04), albedo, metal);
    vec3 F = F0 + (1.0 - F0) * pow(1.0 - max(dot(H, V), 0.0), 5.0);
    vec3 spec = (D * G) * F / (4.0 * NdV * NdL + 1e-3);
    vec3 diff = (1.0 - metal) * albedo * (1.0 - F);
    vec3 col = Ambient * albedo + (diff / 3.14159 + spec) * LightColor * NdL;

    // Fresnel rim + beat emissive — the audio pulse.
    float rim = pow(1.0 - NdV, 3.0);
    col += LightColor * rim * (0.12 + 0.5 * Pulse);
    col += albedo * Pulse * 0.16;

    // Distance fog toward the horizon (matches the modes' hand-rolled fog).
    float dist = length(CamPos - wpos);
    float fz = dist * 0.030;
    float fog = clamp(1.0 - exp(-fz * fz), 0.0, 1.0);
    col = mix(col, Horizon, fog);

    gl_FragColor = vec4(col, tint.a);
}
"#;

/// Which baked map set to bind.
#[derive(Clone, Copy)]
pub enum Surface {
    /// Greeble sci-fi panels — the Rail Shooter corridor.
    Panel,
    /// Sleek brushed grooves — the Beat Surfer ribbon.
    Ribbon,
}

/// World-space light + look parameters for one bound draw.
pub struct LitParams {
    pub cam: Vec3,
    pub light_dir: Vec3,
    pub light_color: Vec3,
    pub ambient: Vec3,
    pub horizon: Color,
    pub metal: f32,
    pub rough: f32,
    pub tile: Vec2,
    pub pulse: f32,
}

struct Lit {
    mat: Material,
    panel: (Texture2D, Texture2D),
    ribbon: (Texture2D, Texture2D),
}

thread_local! {
    static LIT: RefCell<Option<Lit>> = const { RefCell::new(None) };
}

fn v3(c: Color) -> Vec3 {
    vec3(c.r, c.g, c.b)
}

/// Bilinear value-noise sample at a frequency (tiles via wrapping integer hash).
fn vnoise(fx: f32, fy: f32, freq: f32, seed: i32) -> f32 {
    let val = |ix: i32, iy: i32| hash01(ix.wrapping_mul(73856093) ^ iy.wrapping_mul(19349663) ^ seed);
    let (gx, gy) = (fx * freq, fy * freq);
    let (x0, y0) = (gx.floor() as i32, gy.floor() as i32);
    let (tx, ty) = (gx - x0 as f32, gy - y0 as f32);
    let (sx, sy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
    let a = val(x0, y0);
    let b = val(x0 + 1, y0);
    let c = val(x0, y0 + 1);
    let d = val(x0 + 1, y0 + 1);
    (a + (b - a) * sx) * (1.0 - sy) + (c + (d - c) * sx) * sy
}

/// Bake a (normal, material) texture pair for one surface kind. The material map
/// packs r=grey-albedo, g=roughness, b=metalness. REPEAT wrap is enabled so the
/// maps tile along the ribbon/corridor.
fn bake(kind: Surface) -> (Texture2D, Texture2D) {
    let n = 256usize;
    let mut h = vec![0f32; n * n];
    for y in 0..n {
        for x in 0..n {
            let (fx, fy) = (x as f32 / n as f32, y as f32 / n as f32);
            h[y * n + x] = match kind {
                Surface::Panel => {
                    let cell = 32usize;
                    let seam = if x % cell < 2 || y % cell < 2 { 0.0 } else { 1.0 };
                    let grime = 0.55 * vnoise(fx, fy, 8.0, 7) + 0.3 * vnoise(fx, fy, 24.0, 11) + 0.15 * vnoise(fx, fy, 64.0, 19);
                    let (rx, ry) = ((x % cell) as i32 - 6, (y % cell) as i32 - 6);
                    let rivet = if rx * rx + ry * ry < 6 { 0.45 } else { 0.0 };
                    (0.15 + 0.55 * seam * grime + rivet).clamp(0.0, 1.0)
                }
                Surface::Ribbon => {
                    // Brushed lengthwise grooves + a fine tooth of noise.
                    let groove = 0.5 + 0.22 * (fx * std::f32::consts::TAU * 7.0).sin();
                    let fine = 0.16 * vnoise(fx, fy, 48.0, 23) + 0.1 * vnoise(fx, fy, 120.0, 31);
                    (groove * 0.8 + fine).clamp(0.0, 1.0)
                }
            };
        }
    }

    // Normal map: Sobel of the height field, encoded xyz -> RGB.
    let strength = match kind {
        Surface::Panel => 2.6,
        Surface::Ribbon => 1.4,
    };
    let at = |x: i32, y: i32| h[(y.rem_euclid(n as i32) as usize) * n + x.rem_euclid(n as i32) as usize];
    let mut nbuf = vec![0u8; n * n * 4];
    let mut mbuf = vec![0u8; n * n * 4];
    for y in 0..n as i32 {
        for x in 0..n as i32 {
            let dx = (at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1))
                - (at(x - 1, y - 1) + 2.0 * at(x - 1, y) + at(x - 1, y + 1));
            let dy = (at(x - 1, y + 1) + 2.0 * at(x, y + 1) + at(x + 1, y + 1))
                - (at(x - 1, y - 1) + 2.0 * at(x, y - 1) + at(x + 1, y - 1));
            let nrm = vec3(-dx * strength, -dy * strength, 1.0).normalize();
            let o = (y as usize * n + x as usize) * 4;
            nbuf[o] = ((nrm.x * 0.5 + 0.5) * 255.0) as u8;
            nbuf[o + 1] = ((nrm.y * 0.5 + 0.5) * 255.0) as u8;
            nbuf[o + 2] = ((nrm.z * 0.5 + 0.5) * 255.0) as u8;
            nbuf[o + 3] = 255;

            let hv = at(x, y);
            let (metal, rough) = match kind {
                Surface::Panel => {
                    let cell = 32i32;
                    let seam = (x % cell) < 2 || (y % cell) < 2;
                    let m = if seam { 0.85 } else { 0.18 + 0.3 * hv };
                    (m, (0.85 - 0.45 * m).clamp(0.1, 0.95))
                }
                Surface::Ribbon => (0.55 + 0.35 * hv, (0.4 - 0.25 * hv).clamp(0.06, 0.6)),
            };
            mbuf[o] = (hv * 255.0) as u8;
            mbuf[o + 1] = (rough * 255.0) as u8;
            mbuf[o + 2] = (metal * 255.0) as u8;
            mbuf[o + 3] = 255;
        }
    }

    let mk = |buf: &[u8]| {
        let t = Texture2D::from_rgba8(n as u16, n as u16, buf);
        t.set_filter(FilterMode::Linear);
        t
    };
    let (ntex, mtex) = (mk(&nbuf), mk(&mbuf));
    // Enable REPEAT so UV > 1 tiles cleanly (from_rgba8 hardcodes Clamp).
    unsafe {
        let gl = get_internal_gl();
        let ctx = gl.quad_context;
        for t in [&ntex, &mtex] {
            ctx.texture_set_wrap(t.raw_miniquad_id(), TextureWrap::Repeat, TextureWrap::Repeat);
        }
    }
    (ntex, mtex)
}

impl Lit {
    fn load() -> Lit {
        let mat = load_material(
            ShaderSource::Glsl { vertex: VERT, fragment: FRAG },
            MaterialParams {
                pipeline_params: PipelineParams {
                    // A custom material uses ITS OWN depth params (macroquad's are
                    // bypassed), so replicate the 3D pipeline or it won't z-sort.
                    depth_test: Comparison::LessOrEqual,
                    depth_write: true,
                    ..Default::default()
                },
                uniforms: vec![
                    UniformDesc::new("CamPos", UniformType::Float3),
                    UniformDesc::new("LightDir", UniformType::Float3),
                    UniformDesc::new("LightColor", UniformType::Float3),
                    UniformDesc::new("Ambient", UniformType::Float3),
                    UniformDesc::new("Horizon", UniformType::Float3),
                    UniformDesc::new("MetalRough", UniformType::Float2),
                    UniformDesc::new("Tile", UniformType::Float2),
                    UniformDesc::new("Pulse", UniformType::Float1),
                ],
                textures: vec!["NormalTex".into(), "MatTex".into()],
            },
        )
        .expect("PBR material failed to compile");
        Lit { mat, panel: bake(Surface::Panel), ribbon: bake(Surface::Ribbon) }
    }
}

/// Bind the lit material for the given surface with these light parameters.
/// MUST be paired with [`unbind`] before any 2D / wireframe / egui draws.
pub fn bind(surface: Surface, p: &LitParams) {
    LIT.with(|c| {
        let mut slot = c.borrow_mut();
        if slot.is_none() {
            *slot = Some(Lit::load());
        }
        let lit = slot.as_ref().unwrap();
        let (ntex, mtex) = match surface {
            Surface::Panel => &lit.panel,
            Surface::Ribbon => &lit.ribbon,
        };
        gl_use_material(&lit.mat);
        lit.mat.set_texture("NormalTex", ntex.clone());
        lit.mat.set_texture("MatTex", mtex.clone());
        lit.mat.set_uniform("CamPos", p.cam);
        lit.mat.set_uniform("LightDir", p.light_dir.normalize_or_zero());
        lit.mat.set_uniform("LightColor", p.light_color);
        lit.mat.set_uniform("Ambient", p.ambient);
        lit.mat.set_uniform("Horizon", v3(p.horizon));
        lit.mat.set_uniform("MetalRough", vec2(p.metal, p.rough));
        lit.mat.set_uniform("Tile", p.tile);
        lit.mat.set_uniform("Pulse", p.pulse.clamp(0.0, 1.5));
    });
}

/// Restore the default material. Call after the lit mesh draw, before wires/HUD.
pub fn unbind() {
    gl_use_default_material();
}
