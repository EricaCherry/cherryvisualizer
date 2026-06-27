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

fn rgb(hex: u32) -> Color {
    Color::new(
        ((hex >> 16) & 0xff) as f32 / 255.0,
        ((hex >> 8) & 0xff) as f32 / 255.0,
        (hex & 0xff) as f32 / 255.0,
        1.0,
    )
}

// ---- mapping any artist palette onto the 8 roles ---------------------------

fn lum(c: Color) -> f32 {
    0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b
}
/// Warm/cool axis: +1 red/orange, -1 cyan/blue, ~0 for greys.
fn warmth(c: Color) -> f32 {
    let x = c.r - 0.5 * (c.g + c.b);
    let y = 0.866_025_4 * (c.g - c.b);
    let mag = (x * x + y * y).sqrt();
    if mag < 1e-4 { 0.0 } else { x / mag }
}
fn sat(c: Color) -> f32 {
    let mx = c.r.max(c.g).max(c.b);
    let mn = c.r.min(c.g).min(c.b);
    if mx < 1e-4 { 0.0 } else { (mx - mn) / mx }
}
fn darken(c: Color, k: f32) -> Color {
    Color::new(c.r * k, c.g * k, c.b * k, 1.0)
}
fn lighten(c: Color, k: f32) -> Color {
    mix(c, WHITE, k)
}

/// Map an arbitrary 3..=48-color artist palette onto the eight fixed roles —
/// darkest→ink, lightest→cream spec, the coolest mids→body, the warmest/most
/// saturated mids→hero. Missing roles are synthesised so even a 3-color palette
/// yields a full, monotonic palette. Deterministic; never panics.
pub fn from_colors(cols: &[Color]) -> Palette {
    if cols.is_empty() {
        return active();
    }
    let mut c: Vec<Color> = cols.iter().copied().take(48).collect();
    c.sort_by(|a, b| lum(*a).total_cmp(&lum(*b)));
    let n = c.len();

    // Force a deep dark ground (keep the darkest hue, but pull it dark) so even
    // an all-bright palette still reads as a dark-room visualizer.
    let dark = |c: Color, target: f32| darken(c, (target / lum(c).max(0.02)).min(1.0));
    let ink = dark(c[0], 0.05);
    let slate = if n >= 2 { dark(c[1], 0.085) } else { lighten(ink, 0.12) };
    let bright = c[n - 1];
    let spec = mix(bright, Color::new(0.96, 0.93, 0.84, 1.0), 0.30); // lifted, never clinical white

    let lo = 2usize.min(n - 1);
    let hi = n.saturating_sub(1).max(lo);
    let mids: Vec<Color> = if hi > lo { c[lo..hi].to_vec() } else { c.clone() };

    let coolest = *mids.iter().min_by(|a, b| warmth(**a).total_cmp(&warmth(**b))).unwrap();
    let warmest = *mids.iter().max_by(|a, b| warmth(**a).total_cmp(&warmth(**b))).unwrap();
    let cool_lo = *mids.iter().min_by(|a, b| (warmth(**a) + lum(**a)).total_cmp(&(warmth(**b) + lum(**b)))).unwrap();

    // Cool BODY (teal_deep darker than teal). If the palette is all-warm, derive
    // a cool body by blending the ground toward teal.
    let (teal_deep, teal) = if warmth(coolest) > 0.25 {
        let synth = mix(slate, Color::new(0.25, 0.55, 0.58, 1.0), 0.45);
        (darken(synth, 0.7), synth)
    } else {
        let a = darken(cool_lo, 0.62);
        let b = if lum(coolest) > lum(a) + 0.05 { coolest } else { lighten(coolest, 0.25) };
        (a, b)
    };

    // Warm HERO (amber_glow brighter than amber). If the palette is all-cool,
    // derive amber from the brightest pushed warm.
    let warm = *mids
        .iter()
        .max_by(|a, b| (warmth(**a) * 0.6 + sat(**a) * 0.4).total_cmp(&(warmth(**b) * 0.6 + sat(**b) * 0.4)))
        .unwrap_or(&warmest);
    let amber = if warmth(warm) < 0.05 { mix(bright, Color::new(0.88, 0.55, 0.25, 1.0), 0.55) } else { warm };
    let amber_glow = lighten(amber, 0.28);

    let warm_dark = mids.iter().filter(|x| warmth(**x) > 0.1).min_by(|a, b| lum(**a).total_cmp(&lum(**b))).copied();
    let ember_shadow = darken(warm_dark.unwrap_or(mix(ink, amber, 0.35)), 0.7);

    Palette { ink, slate, teal_deep, teal, amber, amber_glow, spec, ember_shadow }
}

/// Build a full palette from four hand-set anchors (used by the Custom theme).
pub fn palette_from_anchors(ink: Color, body: Color, hero: Color, spec: Color) -> Palette {
    Palette {
        ink,
        slate: lighten(ink, 0.08),
        teal_deep: darken(body, 0.55),
        teal: body,
        amber: hero,
        amber_glow: lighten(hero, 0.28),
        spec,
        ember_shadow: darken(mix(ink, hero, 0.4), 0.7),
    }
}

/// The house palette — hand-tuned, the default theme.
fn dusk_encom() -> Palette {
    Palette {
        ink: rgb(0x0b1014),
        slate: rgb(0x11181d),
        teal_deep: rgb(0x16323a),
        teal: rgb(0x3f9aa0),
        amber: rgb(0xe08a3c),
        amber_glow: rgb(0xf2b46a),
        spec: rgb(0xece3cf),
        ember_shadow: rgb(0x1a120c),
    }
}

/// Artist palettes from lospec.com/palette-list (any color count — these run
/// 4..46), mapped onto the roles by [`from_colors`].
#[rustfmt::skip]
const LOSPEC: &[(&str, &[u32])] = &[
    ("Oil 6", &[0xfbf5ef,0xf2d3ab,0xc69fa5,0x8b6d9c,0x494d7e,0x272744]),
    ("Twilight 5", &[0xfbbbad,0xee8695,0x4a7a96,0x333f58,0x292831]),
    ("Kirokaze GB", &[0x332c50,0x46878f,0x94e344,0xe2f3e4]),
    ("Mist GB", &[0x2d1b00,0x1e606e,0x5ab9a8,0xc4f0c2]),
    ("Ice Cream GB", &[0x7c3f58,0xeb6b6f,0xf9a875,0xfff6d3]),
    ("Rustic GB", &[0x2c2137,0x764462,0xedb4a1,0xa96868]),
    ("2bit Demichrome", &[0x211e20,0x555568,0xa0a08b,0xe9efec]),
    ("Hollow", &[0x0f0f1b,0x565a75,0xc6b7be,0xfafbf6]),
    ("Kankei4", &[0xffffff,0xf42e1f,0x2f256b,0x060608]),
    ("Lava GB", &[0x051f39,0x4a2480,0xc53a9d,0xff8e80]),
    ("Moonlight GB", &[0x0f052d,0x203671,0x36868f,0x5fc75d]),
    ("SpaceHaze", &[0xf8e3c4,0xcc3495,0x6b1fb1,0x0b0630]),
    ("SLSO8", &[0x0d2b45,0x203c56,0x544e68,0x8d697a,0xd08159,0xffaa5e,0xffd4a3,0xffecd6]),
    ("Nyx8", &[0x08141e,0x0f2a3f,0x20394f,0xf6d6bd,0xc3a38a,0x997577,0x816271,0x4e495f]),
    ("Ammo 8", &[0x040c06,0x112318,0x1e3a29,0x305d42,0x4d8061,0x89a257,0xbedc7f,0xeeffcc]),
    ("FunkyFuture 8", &[0x2b0f54,0xab1f65,0xff4f69,0xfff7f8,0xff8142,0xffda45,0x3368dc,0x49e7ec]),
    ("Citrink", &[0xffffff,0xfcf660,0xb2d942,0x52c33f,0x166e7a,0x254d70,0x252446,0x201533]),
    ("Dreamscape8", &[0xc9cca1,0xcaa05a,0xae6a47,0x8b4049,0x543344,0x515262,0x63787d,0x8ea091]),
    ("PICO-8", &[0x000000,0x1d2b53,0x7e2553,0x008751,0xab5236,0x5f574f,0xc2c3c7,0xfff1e8,0xff004d,0xffa300,0xffec27,0x00e436,0x29adff,0x83769c,0xff77a8,0xffccaa]),
    ("Sweetie 16", &[0x1a1c2c,0x5d275d,0xb13e53,0xef7d57,0xffcd75,0xa7f070,0x38b764,0x257179,0x29366f,0x3b5dc9,0x41a6f6,0x73eff7,0xf4f4f4,0x94b0c2,0x566c86,0x333c57]),
    ("NA16", &[0x8c8fae,0x584563,0x3e2137,0x9a6348,0xd79b7d,0xf5edba,0xc0c741,0x647d34,0xe4943a,0x9d303b,0xd26471,0x70377f,0x7ec4c1,0x34859d,0x17434b,0x1f0e1c]),
    ("Endesga 16", &[0xe4a672,0xb86f50,0x743f39,0x3f2832,0x9e2835,0xe53b44,0xfb922b,0xffe762,0x63c64d,0x327345,0x193d3f,0x4f6781,0xafbfd2,0xffffff,0x2ce8f4,0x0484d1]),
    ("Bubblegum 16", &[0x16171a,0x7f0622,0xd62411,0xff8426,0xffd100,0xfafdff,0xff80a4,0xff2674,0x94216a,0x430067,0x234975,0x68aed4,0xbfff3c,0x10d275,0x007899,0x002859]),
    ("Vinik24", &[0x000000,0x6f6776,0x9a9a97,0xc5ccb8,0x8b5580,0xc38890,0xa593a5,0x666092,0x9a4f50,0xc28d75,0x7ca1c0,0x416aa3,0x8d6268,0xbe955c,0x68aca9,0x387080,0x6e6962,0x93a167,0x6eaa78,0x557064,0x9d9f7f,0x7e9e99,0x5d6872,0x433455]),
    ("Fantasy 24", &[0x1f240a,0x39571c,0xa58c27,0xefac28,0xefd8a1,0xab5c1c,0x183f39,0xef692f,0xefb775,0xa56243,0x773421,0x724113,0x2a1d0d,0x392a1c,0x684c3c,0x927e6a,0x276468,0xef3a0c,0x45230d,0x3c9f9c,0x9b1a0a,0x36170c,0x550f0a,0x300f0a]),
    ("Apollo", &[0x172038,0x253a5e,0x3c5e8b,0x4f8fba,0x73bed3,0xa4dddb,0x19332d,0x25562e,0x468232,0x75a743,0xa8ca58,0xd0da91,0x4d2b32,0x7a4841,0xad7757,0xc09473,0xd7b594,0xe7d5b3,0x884b2b,0xbe772b,0xde9e41,0xe8c170,0x090a14,0x10141f,0x202e37,0x394a50,0x577277,0x819796,0xa8b5b2,0xc7cfcc,0xebede9]),
];

thread_local! {
    static BASE: RefCell<Option<Vec<(&'static str, Palette)>>> = const { RefCell::new(None) };
    static ACTIVE: RefCell<Palette> = RefCell::new(dusk_encom());
    static THEME_IDX: Cell<usize> = const { Cell::new(0) };
    static CUSTOM: RefCell<Option<Palette>> = const { RefCell::new(None) };
}

/// The non-custom themes (house + Lospec), built once and cached.
fn base_themes() -> Vec<(&'static str, Palette)> {
    BASE.with(|b| {
        if b.borrow().is_none() {
            let mut v: Vec<(&'static str, Palette)> = vec![("Dusk Encom", dusk_encom())];
            for (name, hexes) in LOSPEC {
                let cols: Vec<Color> = hexes.iter().map(|&h| rgb(h)).collect();
                v.push((name, from_colors(&cols)));
            }
            *b.borrow_mut() = Some(v);
        }
        b.borrow().clone().unwrap()
    })
}

/// Total theme count, including the trailing live "Custom" slot.
pub fn theme_count() -> usize {
    base_themes().len() + 1
}

/// Theme names for the picker (last is "Custom").
pub fn theme_names() -> Vec<&'static str> {
    let mut n: Vec<&'static str> = base_themes().iter().map(|(s, _)| *s).collect();
    n.push("Custom");
    n
}

pub fn current_theme() -> usize {
    THEME_IDX.with(|c| c.get())
}

/// The active palette (a snapshot — `Palette` is `Copy`).
pub fn active() -> Palette {
    ACTIVE.with(|c| *c.borrow())
}

pub fn custom_palette() -> Palette {
    CUSTOM.with(|c| c.borrow().unwrap_or_else(dusk_encom))
}

/// Set the editable Custom palette (does not re-activate it).
pub fn set_custom(p: Palette) {
    CUSTOM.with(|c| *c.borrow_mut() = Some(p));
}

/// Switch the active theme and rebake the palette-dependent textures. The last
/// index selects the live Custom palette.
pub fn set_theme(i: usize) {
    let base = base_themes();
    let custom_idx = base.len();
    let i = i.min(custom_idx);
    let pal = if i == custom_idx { custom_palette() } else { base[i].1 };
    ACTIVE.with(|c| *c.borrow_mut() = pal);
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
    // A SOFT radial glow — concentric translucent rings fading outward. No hard
    // solid disc and no bright off-center "pupil"; those read as a solid eye/orb.
    v.circle(x, y, r * 2.6, with_alpha(accent, 0.06));
    v.circle(x, y, r * 1.9, with_alpha(accent, 0.11));
    v.circle(x, y, r * 1.3, with_alpha(accent, 0.20));
    v.circle(x, y, r * 0.85, with_alpha(accent, 0.34));
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

