//! The shared visual language — art direction "Dusk Encom", now theme-driven.
//!
//! Color is mapped to ENERGY (never to element index, the rainbow-HSL tell):
//! quiet sits in a cool body family, loud lights up one warm hero, over a graded
//! filmic ground with a restrained vignette finish. The eight *roles*
//! are fixed; their *colors* come from the active [`Theme`], so switching theme
//! re-skins the whole app. Modes read roles through the accessor fns
//! ([`ink`], [`teal`], [`amber`], …) and pull energy color from [`grade`].

use std::cell::{Cell, RefCell};

use macroquad::prelude::*;

use crate::view::{self, View};

// ---- the palette roles + curated themes ------------------------------------

/// Eight roles: `ink`/`slate` = the near-black ground; `teal_deep`/`teal` = the
/// cool body family (quiet); `amber`/`amber_glow` = the warm hero (loud); `spec`
/// = the cream highlight (never pure white); `ember_shadow` = warm dirt for the
/// lower vignette. Role names are historical (the house theme is teal/amber);
/// other themes map other hues onto the same roles.
#[derive(Clone, Copy)]
pub struct Palette {
    pub ink: Color,
    pub slate: Color,
    pub teal_deep: Color,
    pub teal: Color,
    pub amber: Color,
    pub amber_glow: Color,
    pub spec: Color,
    pub ember_shadow: Color,
}

pub struct Theme {
    pub name: &'static str,
    pub palette: Palette,
}

fn rgb(hex: u32) -> Color {
    Color::new(
        ((hex >> 16) & 0xff) as f32 / 255.0,
        ((hex >> 8) & 0xff) as f32 / 255.0,
        (hex & 0xff) as f32 / 255.0,
        1.0,
    )
}

/// The curated themes. "Dusk Encom" is the house look; the rest map artistic
/// palettes from lospec.com/palette-list (SLSO8, Nyx8, Oil 6, Apollo) onto the
/// eight roles. Order: ink, slate, teal_deep, teal, amber, amber_glow, spec, ember.
pub fn themes() -> Vec<Theme> {
    #[allow(clippy::too_many_arguments)]
    fn t(
        name: &'static str,
        ink: u32,
        slate: u32,
        teal_deep: u32,
        teal: u32,
        amber: u32,
        amber_glow: u32,
        spec: u32,
        ember_shadow: u32,
    ) -> Theme {
        Theme {
            name,
            palette: Palette {
                ink: rgb(ink),
                slate: rgb(slate),
                teal_deep: rgb(teal_deep),
                teal: rgb(teal),
                amber: rgb(amber),
                amber_glow: rgb(amber_glow),
                spec: rgb(spec),
                ember_shadow: rgb(ember_shadow),
            },
        }
    }
    vec![
        t("Dusk Encom", 0x0b1014, 0x11181d, 0x16323a, 0x3f9aa0, 0xe08a3c, 0xf2b46a, 0xece3cf, 0x1a120c),
        t("Sunset", 0x0d2b45, 0x1b3a52, 0x2f4a60, 0x5e7e93, 0xe0894e, 0xffaa5e, 0xffecd6, 0x2a1810),
        t("Nyx", 0x08141e, 0x0f2a3f, 0x20394f, 0x4d6a80, 0xc98f6b, 0xf6d6bd, 0xffeede, 0x1c1014),
        t("Oil Dream", 0x191a2e, 0x272744, 0x3c3c63, 0x8b6d9c, 0xd28f7e, 0xf2d3ab, 0xfbf5ef, 0x241526),
        t("Forest", 0x090a14, 0x10141f, 0x19332d, 0x468232, 0xde9e41, 0xe8c170, 0xe7d5b3, 0x1a1208),
        t("Ember", 0x090a14, 0x241527, 0x5a2e2a, 0xbe772b, 0x73bed3, 0xa4dddb, 0xebede9, 0x341c27),
    ]
}

thread_local! {
    static ACTIVE: RefCell<Palette> = RefCell::new(themes()[0].palette);
    static THEME_IDX: Cell<usize> = const { Cell::new(0) };
}

/// The active palette (a snapshot — `Palette` is `Copy`).
pub fn active() -> Palette {
    ACTIVE.with(|c| *c.borrow())
}

pub fn current_theme() -> usize {
    THEME_IDX.with(|c| c.get())
}

/// Switch the active theme and rebake the palette-dependent textures.
pub fn set_theme(i: usize) {
    let ts = themes();
    let i = i.min(ts.len() - 1);
    ACTIVE.with(|c| *c.borrow_mut() = ts[i].palette);
    THEME_IDX.with(|c| c.set(i));
    invalidate_baked();
}

// Role accessors — modes read color through these so a theme switch re-skins all.
pub fn ink() -> Color {
    active().ink
}
pub fn slate() -> Color {
    active().slate
}
pub fn teal_deep() -> Color {
    active().teal_deep
}
pub fn teal() -> Color {
    active().teal
}
pub fn amber() -> Color {
    active().amber
}
pub fn amber_glow() -> Color {
    active().amber_glow
}
pub fn spec() -> Color {
    active().spec
}

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
/// active palette from cool/quiet (deep body) through the warm hero to the cream
/// highlight at the very hottest.
pub fn grade(t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let p = active();
    let mut c = mix(p.teal_deep, p.teal, smoothstep(0.0, 0.55, t));
    c = mix(c, p.amber, smoothstep(0.55, 0.90, t));
    c = mix(c, p.spec, smoothstep(0.90, 1.0, t));
    c
}

/// Value-based glow for the one hero element per mode — a soft halo that blends
/// toward the background (not white), a solid accent core, and a small cream
/// specular toward the key light. Replaces stacked additive copies.
pub fn glow_core(v: &View, x: f32, y: f32, r: f32, accent: Color) {
    let halo = mix(ink(), accent, 0.55);
    v.circle(x, y, r * 2.7, with_alpha(halo, 0.10));
    v.circle(x, y, r * 1.7, with_alpha(halo, 0.20));
    v.circle(x, y, r, accent);
    // specular toward the off-center key light (upper-left, y is up)
    v.circle(x - r * 0.30, y + r * 0.30, r * 0.42, with_alpha(spec(), 0.85));
}

// ---- baked textures (built once per theme, on first use) -------------------

thread_local! {
    static BACKDROP: RefCell<Option<Texture2D>> = const { RefCell::new(None) };
    static VIGNETTE: RefCell<Option<Texture2D>> = const { RefCell::new(None) };
}

fn invalidate_baked() {
    BACKDROP.with(|c| *c.borrow_mut() = None);
    VIGNETTE.with(|c| *c.borrow_mut() = None);
}

/// The graded background: an ink->slate vertical gradient with a faint
/// off-center key-light pool, so the dark has depth and a light direction.
/// Modes call this instead of `clear_background`.
pub fn backdrop() {
    clear_background(ink());
    backdrop_blend(1.0);
}

fn backdrop_tex() -> Texture2D {
    BACKDROP.with(|c| {
        if c.borrow().is_none() {
            *c.borrow_mut() = Some(build_backdrop());
        }
        c.borrow().as_ref().unwrap().clone()
    })
}

/// Draw the backdrop gradient at `alpha` over the current target *without*
/// clearing — the feedback buffer uses this to decay old frames toward the floor
/// each frame, which is what leaves motion trails.
pub fn backdrop_blend(alpha: f32) {
    let tex = backdrop_tex();
    let (w, h) = (view::screen_w(), view::screen_h());
    draw_texture_ex(&tex, 0.0, 0.0, with_alpha(WHITE, alpha), DrawTextureParams { dest_size: Some(vec2(w, h)), ..Default::default() });
}

/// The shared finish: just a soft vignette to frame the dark. Called last in the
/// pipeline, so play and export share one look. (No grain.)
pub fn finish() {
    let (w, h) = (view::screen_w(), view::screen_h());
    let vig = VIGNETTE.with(|c| {
        if c.borrow().is_none() {
            *c.borrow_mut() = Some(build_vignette());
        }
        c.borrow().as_ref().unwrap().clone()
    });
    draw_texture_ex(&vig, 0.0, 0.0, WHITE, DrawTextureParams { dest_size: Some(vec2(w, h)), ..Default::default() });
}

fn build_backdrop() -> Texture2D {
    let p = active();
    let (w, h) = (320u16, 180u16);
    let mut img = Image::gen_image_color(w, h, p.ink);
    for y in 0..h {
        for x in 0..w {
            let fy = y as f32 / (h - 1) as f32; // 0 top .. 1 bottom
            let base = mix(p.ink, p.slate, fy);
            // off-center key-light pool (upper-left third)
            let dx = x as f32 / w as f32 - 0.36;
            let dy = y as f32 / h as f32 - 0.32;
            let d = (dx * dx + dy * dy).sqrt();
            let pool = (1.0 - smoothstep(0.0, 0.5, d)) * 0.06;
            img.set_pixel(x as u32, y as u32, mix(base, p.teal_deep, pool));
        }
    }
    let tex = Texture2D::from_image(&img);
    tex.set_filter(FilterMode::Linear);
    tex
}

fn build_vignette() -> Texture2D {
    let p = active();
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
            let tint = mix(p.slate, p.ember_shadow, warm * 0.6);
            img.set_pixel(x as u32, y as u32, Color::new(tint.r, tint.g, tint.b, a));
        }
    }
    let tex = Texture2D::from_image(&img);
    tex.set_filter(FilterMode::Linear);
    tex
}

