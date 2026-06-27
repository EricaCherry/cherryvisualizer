//! Mirrored Bars — the spectrum reflected above and below a centre line so the
//! bars open into a symmetric band. Same Web-Audio dB band levels as Spectrum;
//! every bar is coloured by its own level (energy = colour).

use macroquad::prelude::*;

use crate::analysis::N_BANDS;
use crate::modes::{FrameCtx, Mode, Param};
use crate::style::{grade, hash01, teal_deep, with_alpha};
use crate::track::Track;
use crate::view::{View, AH, AW};

pub struct Mirror {
    heights: [f32; N_BANDS],
    height: f32,
    gap: f32,
}

impl Mirror {
    pub fn new() -> Self {
        Mirror { heights: [0.0; N_BANDS], height: 1.0, gap: 0.26 }
    }
}

impl Mode for Mirror {
    fn name(&self) -> &'static str {
        "Mirrored Bars"
    }
    fn about(&self) -> &'static str {
        "The spectrum mirrored above and below a centre line — a symmetric bar field."
    }
    fn trail(&self) -> f32 {
        0.12
    }

    fn params(&self) -> Vec<Param> {
        vec![
            Param::float("Bar height", self.height, 0.5, 1.2),
            Param::float("Bar gap", self.gap, 0.0, 0.6),
        ]
    }
    fn set_param(&mut self, name: &str, v: f32) {
        match name {
            "Bar height" => self.height = v,
            "Bar gap" => self.gap = v,
            _ => {}
        }
    }

    fn reset(&mut self, _t: &Track) {
        self.heights = [0.0; N_BANDS];
    }

    fn update(&mut self, ctx: &FrameCtx) {
        // Bars track the analysis bands directly (single smoothing is upstream).
        for i in 0..N_BANDS {
            self.heights[i] = ctx.feat.bands[i];
        }
    }

    fn draw(&self, _ctx: &FrameCtx) {
        let v = View::fit_world(AW, AH);
        let cy = AH * 0.5;
        let max_h = AH * 0.46 * self.height;
        let margin = 0.3;
        let slot = (AW - margin * 2.0) / N_BANDS as f32;
        let fill = (1.0 - self.gap).clamp(0.2, 1.0);

        for i in 0..N_BANDS {
            let e = self.heights[i];
            let jw = 1.0 + (hash01(i as i32 * 7 + 1) - 0.5) * 0.25;
            let bw = (slot * fill * jw).max(0.04);
            let bx = margin + i as f32 * slot + (slot - bw) * 0.5;
            let h = (e * max_h).clamp(0.01, AH * 0.5 - 0.2); // stay on-screen

            // Every bar coloured by its own level (energy = colour); no "hero".
            let c = grade(0.12 + e * 0.82);
            let a = 0.4 + 0.6 * e;
            // Flat bar mirrored above and below the centre line — no white tips.
            v.rect(bx, cy + h, bw, h * 2.0, with_alpha(c, a));
        }
        // A faint centre line keeps the symmetry reading.
        v.rect(0.0, cy + 0.012, AW, 0.024, with_alpha(teal_deep(), 0.4));
    }
}
