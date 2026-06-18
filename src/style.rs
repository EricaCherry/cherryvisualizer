//! The shared visual language — art direction "Dusk Encom".
//!
//! One committed palette, color mapped to ENERGY (never to element index, which
//! is the rainbow-HSL tell), a graded filmic background instead of flat slate,
//! and a restrained finish (vignette + fine grain) rather than additive neon.
//! Every mode pulls its color from [`grade`], paints its floor with
//! [`backdrop`], lights its hero with [`glow_core`], and ends with [`finish`].

use std::cell::RefCell;

use macroquad::prelude::*;

use crate::view::{self, View};

// ---- the master palette ----------------------------------------------------
// Deep, slightly-cool near-blacks hold 70-85% of the frame; chroma is spent on
// one cool body family (teal) and one warm hero (amber). Never pure #000/#fff.

/// #0b1014 — base clear + top of the bg gradient.
pub const INK: Color = Color::new(0.043, 0.063, 0.078, 1.0);
/// #11181d — bottom of the bg gradient, deepest fill, vignette shadow tint.
pub const SLATE: Color = Color::new(0.067, 0.094, 0.114, 1.0);
/// #16323a — cool/quiet end of the energy LUT, back rows, key-light pool.
pub const TEAL_DEEP: Color = Color::new(0.086, 0.196, 0.227, 1.0);
/// #3f9aa0 — the primary body accent: waveforms, structure, mid energy.
pub const TEAL: Color = Color::new(0.247, 0.604, 0.627, 1.0);
/// #e08a3c — the single reserved hero. Loud = amber, quiet = teal. <8% of pixels.
pub const AMBER: Color = Color::new(0.878, 0.541, 0.235, 1.0);
/// #f2b46a — the warm halo around amber + the bloom/halation tint.
pub const AMBER_GLOW: Color = Color::new(0.949, 0.706, 0.416, 1.0);
/// #ece3cf — rare warm near-white specular tip. Replaces all pure white.
pub const SPEC: Color = Color::new(0.925, 0.890, 0.812, 1.0);
/// #1a120c — warm dirt for the lower vignette corners; never an element color.
pub const EMBER_SHADOW: Color = Color::new(0.102, 0.071, 0.047, 1.0);

// ---- color helpers ---------------------------------------------------------

pub fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::new(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, a.a + (b.a - a.a) * t)
}

pub fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    if (b - a).abs() < 1e-6 {
        return if x < a { 0.0 } else { 1.0 };
    }
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn with_alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, a)
}

/// Deterministic hash -> 0..1, for seeded jitter (breaks mechanical grids).
pub fn hash01(n: i32) -> f32 {
    let mut x = n.wrapping_mul(374761393).wrapping_add(668265263) as u32;
    x = (x ^ (x >> 13)).wrapping_mul(1274126177);
    ((x ^ (x >> 16)) & 0xffff) as f32 / 65535.0
}

/// The energy ramp that replaces every `hsl(index)` call: `t` in 0..1 walks the
/// palette from cool/quiet to warm/loud. Quiet things are deep teal; loud things
/// light up amber; only the very hottest tip into the cream highlight.
pub fn grade(t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let mut c = mix(TEAL_DEEP, TEAL, smoothstep(0.0, 0.55, t));
    c = mix(c, AMBER, smoothstep(0.55, 0.90, t));
    c = mix(c, SPEC, smoothstep(0.90, 1.0, t));
    c
}

/// Value-based glow for the one hero element per mode — a soft halo that blends
/// toward the background (not white), a solid accent core, and a small cream
/// specular toward the key light. Replaces stacked additive copies.
pub fn glow_core(v: &View, x: f32, y: f32, r: f32, accent: Color) {
    let halo = mix(INK, accent, 0.55);
    v.circle(x, y, r * 2.7, with_alpha(halo, 0.10));
    v.circle(x, y, r * 1.7, with_alpha(halo, 0.20));
    v.circle(x, y, r, accent);
    // specular toward the off-center key light (upper-left, y is up)
    v.circle(x - r * 0.30, y + r * 0.30, r * 0.42, with_alpha(SPEC, 0.85));
}

// ---- baked textures (built once, on first use) -----------------------------

thread_local! {
    static BACKDROP: RefCell<Option<Texture2D>> = const { RefCell::new(None) };
    static VIGNETTE: RefCell<Option<Texture2D>> = const { RefCell::new(None) };
    static GRAIN: RefCell<Option<Vec<Texture2D>>> = const { RefCell::new(None) };
}

const GRAIN_TILES: usize = 4;

/// The graded background: an INK->SLATE vertical gradient with a faint
/// off-center key-light pool, so the dark has depth and a light direction.
/// Modes call this instead of `clear_background`.
pub fn backdrop() {
    clear_background(INK);
    let tex = BACKDROP.with(|c| {
        if c.borrow().is_none() {
            *c.borrow_mut() = Some(build_backdrop());
        }
        c.borrow().as_ref().unwrap().clone()
    });
    let (w, h) = (view::screen_w(), view::screen_h());
    draw_texture_ex(&tex, 0.0, 0.0, WHITE, DrawTextureParams { dest_size: Some(vec2(w, h)), ..Default::default() });
}

/// The shared filmic finish: a soft vignette then fine animated grain. Called
/// last in every mode's `draw`, so play and export share one look. `time` drives
/// the grain so it shimmers (and stays deterministic for exports).
pub fn finish(time: f32) {
    let (w, h) = (view::screen_w(), view::screen_h());

    let vig = VIGNETTE.with(|c| {
        if c.borrow().is_none() {
            *c.borrow_mut() = Some(build_vignette());
        }
        c.borrow().as_ref().unwrap().clone()
    });
    draw_texture_ex(&vig, 0.0, 0.0, WHITE, DrawTextureParams { dest_size: Some(vec2(w, h)), ..Default::default() });

    let grain = GRAIN.with(|c| {
        if c.borrow().is_none() {
            *c.borrow_mut() = Some(build_grain());
        }
        let tiles = c.borrow();
        let tiles = tiles.as_ref().unwrap();
        let idx = ((time * 16.0) as usize) % tiles.len();
        tiles[idx].clone()
    });
    draw_texture_ex(&grain, 0.0, 0.0, with_alpha(WHITE, 0.06), DrawTextureParams { dest_size: Some(vec2(w, h)), ..Default::default() });
}

fn build_backdrop() -> Texture2D {
    let (w, h) = (320u16, 180u16);
    let mut img = Image::gen_image_color(w, h, INK);
    for y in 0..h {
        for x in 0..w {
            let fy = y as f32 / (h - 1) as f32; // 0 top .. 1 bottom
            let base = mix(INK, SLATE, fy);
            // off-center key-light pool (upper-left third)
            let dx = x as f32 / w as f32 - 0.36;
            let dy = y as f32 / h as f32 - 0.32;
            let d = (dx * dx + dy * dy).sqrt();
            let pool = (1.0 - smoothstep(0.0, 0.5, d)) * 0.06;
            img.set_pixel(x as u32, y as u32, mix(base, TEAL_DEEP, pool));
        }
    }
    let tex = Texture2D::from_image(&img);
    tex.set_filter(FilterMode::Linear);
    tex
}

fn build_vignette() -> Texture2D {
    let n = 256u16;
    let mut img = Image::gen_image_color(n, n, Color::new(0.0, 0.0, 0.0, 0.0));
    for y in 0..n {
        for x in 0..n {
            let u = x as f32 / (n - 1) as f32;
            let v = y as f32 / (n - 1) as f32;
            // Off-center to match the backdrop key light, so there's one
            // coherent light direction and a protected bright center.
            let dx = u - 0.46;
            let dy = v - 0.42;
            let d = ((dx * dx + dy * dy).sqrt() / 0.72).min(1.0);
            let a = smoothstep(0.55, 1.0, d) * 0.62;
            // cool shadow generally; warm the lower corners to break symmetry
            let warm = smoothstep(0.6, 1.0, v) * smoothstep(0.5, 1.0, d);
            let tint = mix(SLATE, EMBER_SHADOW, warm * 0.6);
            img.set_pixel(x as u32, y as u32, Color::new(tint.r, tint.g, tint.b, a));
        }
    }
    let tex = Texture2D::from_image(&img);
    tex.set_filter(FilterMode::Linear);
    tex
}

fn build_grain() -> Vec<Texture2D> {
    // 16:9 tiles at near-frame resolution so a fullscreen blit is ~1px grain.
    // Built straight as RGBA bytes (from_rgba8) — far faster than set_pixel.
    let (w, h) = (1280usize, 720usize);
    let mut seed = 0x1234_5678u32;
    let mut rng = move || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 8) as f32 / 16_777_216.0
    };
    let mut tiles = Vec::with_capacity(GRAIN_TILES);
    for _ in 0..GRAIN_TILES {
        let mut bytes = vec![0u8; w * h * 4];
        for p in 0..w * h {
            let r = rng();
            // bipolar grain: bright cream + (weaker) dark ink specks, cubed so all
            // but the rare extremes are invisible and true blacks stay clean.
            let d = (r - 0.5).abs() * 2.0;
            let (col, k) = if r > 0.5 { (SPEC, 1.0) } else { (INK, 0.5) };
            let a = d * d * d * k;
            let o = p * 4;
            bytes[o] = (col.r * 255.0) as u8;
            bytes[o + 1] = (col.g * 255.0) as u8;
            bytes[o + 2] = (col.b * 255.0) as u8;
            bytes[o + 3] = (a * 255.0) as u8;
        }
        let tex = Texture2D::from_rgba8(w as u16, h as u16, &bytes);
        tex.set_filter(FilterMode::Nearest);
        tiles.push(tex);
    }
    tiles
}
