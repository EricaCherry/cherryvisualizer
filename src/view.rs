//! World-space helpers and the shared palette.
//!
//! Every mode works in a fixed 16x9 world with y pointing UP, letterboxed into
//! whatever window the user has. Keeping modes in world units means they never
//! think about pixels or window resizes.

use std::cell::{Cell, RefCell};

use macroquad::prelude::*;

// ---- offscreen-render plumbing (used by the video exporter) ----------------
//
// During export, modes must lay out for the export resolution (not the window)
// and draw into an offscreen target rather than the screen. These thread-locals
// carry that context so modes need no extra parameters: `View` and any mode
// that reads the screen size go through [`screen_w`]/[`screen_h`], and 2D draws
// go through [`apply_screen_camera`].

thread_local! {
    static RENDER_SIZE: Cell<Option<(f32, f32)>> = const { Cell::new(None) };
    static EXPORT_TARGET: RefCell<Option<RenderTarget>> = const { RefCell::new(None) };
}

/// Override the logical screen size (Some during export), else the window size.
pub fn set_render_size(size: Option<(f32, f32)>) {
    RENDER_SIZE.with(|c| c.set(size));
}

/// Install the offscreen target modes should render into (Some during export).
pub fn set_export_target(rt: Option<RenderTarget>) {
    EXPORT_TARGET.with(|c| *c.borrow_mut() = rt);
}

/// The active offscreen target, if any (a 3D mode attaches this to its camera).
pub fn export_target() -> Option<RenderTarget> {
    EXPORT_TARGET.with(|c| c.borrow().clone())
}

/// Logical screen width — the export width during export, else the window's.
pub fn screen_w() -> f32 {
    RENDER_SIZE.with(|c| c.get()).map_or_else(screen_width, |s| s.0)
}

/// Logical screen height — the export height during export, else the window's.
pub fn screen_h() -> f32 {
    RENDER_SIZE.with(|c| c.get()).map_or_else(screen_height, |s| s.1)
}

/// Activate the 2D screen-space camera modes draw through. In normal play this
/// is just the default camera; during export it renders into the export target
/// at the export resolution.
pub fn apply_screen_camera() {
    match export_target() {
        Some(rt) => {
            let mut cam = Camera2D::from_display_rect(Rect::new(0.0, 0.0, screen_w(), screen_h()));
            cam.render_target = Some(rt);
            set_camera(&cam);
        }
        None => set_default_camera(),
    }
}

/// World width in world units (wu).
pub const AW: f32 = 16.0;
/// Default world height; modes may pick their own via [`View::fit_world`].
#[allow(dead_code)]
pub const AH: f32 = 9.0;

/// Maps world coordinates (y-up) to screen pixels, letterboxed and centered.
pub struct View {
    scale: f32,
    ox: f32,
    oy: f32,
    h: f32,
}

impl View {
    /// The default 16x9 world.
    #[allow(dead_code)]
    pub fn fit() -> Self {
        Self::fit_world(AW, AH)
    }

    /// Fit an arbitrary world size into the window (a mode may want a taller
    /// arena — it letterboxes/pillarboxes as needed).
    pub fn fit_world(world_w: f32, world_h: f32) -> Self {
        let (sw, sh) = (screen_w(), screen_h());
        let scale = (sw / world_w).min(sh / world_h);
        View {
            scale,
            ox: (sw - world_w * scale) * 0.5,
            oy: (sh - world_h * scale) * 0.5,
            h: world_h,
        }
    }

    /// World point -> screen point.
    pub fn xy(&self, x: f32, y: f32) -> (f32, f32) {
        (self.ox + x * self.scale, self.oy + (self.h - y) * self.scale)
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

// The shared palette + energy grade live in `style` (theme-driven); call sites
// that want the background color use `style::ink()`.
