//! World-space helpers and the shared palette.
//!
//! Every mode works in a fixed 16x9 world with y pointing UP, letterboxed into
//! whatever window the user has. Keeping modes in world units means they never
//! think about pixels or window resizes.

use macroquad::prelude::*;

/// World width in world units (wu).
pub const AW: f32 = 16.0;
/// World height in world units.
pub const AH: f32 = 9.0;

/// Maps world coordinates (y-up) to screen pixels, letterboxed and centered.
pub struct View {
    scale: f32,
    ox: f32,
    oy: f32,
}

impl View {
    pub fn fit() -> Self {
        let scale = (screen_width() / AW).min(screen_height() / AH);
        View {
            scale,
            ox: (screen_width() - AW * scale) * 0.5,
            oy: (screen_height() - AH * scale) * 0.5,
        }
    }

    /// World point -> screen point.
    pub fn xy(&self, x: f32, y: f32) -> (f32, f32) {
        (self.ox + x * self.scale, self.oy + (AH - y) * self.scale)
    }

    /// World length -> screen length.
    pub fn s(&self, v: f32) -> f32 {
        v * self.scale
    }

    /// Filled rect; (x, y_top) is the top-left corner in world space.
    pub fn rect(&self, x: f32, y_top: f32, w: f32, h: f32, color: Color) {
        let (sx, sy) = self.xy(x, y_top);
        draw_rectangle(sx, sy, self.s(w), self.s(h), color);
    }

    pub fn line(&self, x0: f32, y0: f32, x1: f32, y1: f32, thick_px: f32, color: Color) {
        let (sx0, sy0) = self.xy(x0, y0);
        let (sx1, sy1) = self.xy(x1, y1);
        draw_line(sx0, sy0, sx1, sy1, thick_px, color);
    }

    pub fn circle(&self, x: f32, y: f32, r: f32, color: Color) {
        let (sx, sy) = self.xy(x, y);
        draw_circle(sx, sy, self.s(r), color);
    }
}

// ---- palette (calm and solid; deliberately not neon) -----------------------

pub const BG: Color = Color::new(0.07, 0.08, 0.10, 1.0);
pub const INK: Color = Color::new(1.0, 1.0, 1.0, 0.65);
pub const INK_DIM: Color = Color::new(1.0, 1.0, 1.0, 0.35);
pub const WAVE: Color = Color::new(0.55, 0.72, 0.85, 0.95);

/// HSL (all 0..1) -> Color.
pub fn hsl(h: f32, s: f32, l: f32) -> Color {
    let h = ((h % 1.0) + 1.0) % 1.0;
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h * 6.0) as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    Color::new(r + m, g + m, b + m, 1.0)
}
