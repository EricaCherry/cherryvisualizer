//! Mirrored Bars — the spectrum reflected above and below a centre line so the
//! bars open into a symmetric band (a diamond when the mids dominate). Same
//! Web-Audio dB band levels + one EMA as Spectrum; energy is colour and the
//! loudest band is the single warm hero.

use macroquad::prelude::*;

use crate::analysis::N_BANDS;
use crate::modes::{FrameCtx, Mode};
use crate::style::{amber, hash01, mix, smoothstep, spec, teal, teal_deep, with_alpha};
use crate::track::Track;
use crate::view::{View, AH, AW};

pub struct Mirror {
    heights: [f32; N_BANDS],
    last_hero: usize,
}

impl Mirror {
    pub fn new() -> Self {
        Mirror { heights: [0.0; N_BANDS], last_hero: 0 }
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

    fn reset(&mut self, _t: &Track) {
        self.heights = [0.0; N_BANDS];
    }

    fn update(&mut self, ctx: &FrameCtx) {
        // Bars track the analysis bands directly (single smoothing is upstream).
        for i in 0..N_BANDS {
            self.heights[i] = ctx.feat.bands[i];
        }
        let cand = (2..N_BANDS - 2).max_by(|&a, &b| self.heights[a].total_cmp(&self.heights[b])).unwrap_or(2);
        if self.heights[cand] > self.heights[self.last_hero] * 1.15 {
            self.last_hero = cand;
        }
    }

    fn draw(&self, _ctx: &FrameCtx) {
        let v = View::fit_world(AW, AH);
        let cy = AH * 0.5;
        let max_h = AH * 0.46;
        let margin = 0.3;
        let slot = (AW - margin * 2.0) / N_BANDS as f32;
        let hero = self.last_hero;

        for i in 0..N_BANDS {
            let e = self.heights[i];
            let jw = 1.0 + (hash01(i as i32 * 7 + 1) - 0.5) * 0.25;
            let bw = (slot * 0.74 * jw).max(0.04);
            let bx = margin + i as f32 * slot + (slot - bw) * 0.5;
            let h = (e * max_h).max(0.01);
            let hero_bar = i == hero && e > 0.05;

            let mut c = mix(teal_deep(), teal(), smoothstep(0.04, 0.78, e));
            if hero_bar {
                c = mix(c, amber(), smoothstep(0.30, 0.95, e));
            }
            let a = if hero_bar { 1.0 } else { 0.4 + 0.6 * e };
            // The bar mirrored above and below the centre line.
            v.rect(bx, cy + h, bw, h * 2.0, with_alpha(c, a));
            // Cream tips at both ends within the bar's own family.
            let tip = with_alpha(mix(c, spec(), 0.3), a);
            v.rect(bx, cy + h, bw, (h * 0.12).min(0.12), tip);
            v.rect(bx, cy - h + (h * 0.12).min(0.12), bw, (h * 0.12).min(0.12), tip);
        }
        // A faint centre line keeps the symmetry reading.
        v.rect(0.0, cy + 0.012, AW, 0.024, with_alpha(teal_deep(), 0.4));
    }
}
