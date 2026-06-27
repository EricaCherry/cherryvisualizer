//! Spectrum — frequency bars, art-directed.
//!
//! 32 log-spaced bands rise from an off-center baseline with peak-hold caps
//! that fall under gravity. Color is mapped to ENERGY, not band index: the
//! whole bank sits in one cool teal family separated by brightness, and only
//! the single loudest band tips warm (amber cap + cream tip) as the one hero.
//! Bar widths are log-weighted and jittered so it reads as designed, not as a
//! mechanical comb.

use macroquad::prelude::*;

use crate::analysis::N_BANDS;
use crate::modes::{FrameCtx, Mode, Param};
use crate::style::{grade, hash01, teal_deep, with_alpha};
use crate::track::Track;
use crate::view::{View, AH, AW};

pub struct Spectrum {
    heights: [f32; N_BANDS],
    caps: [f32; N_BANDS],
    cap_vel: [f32; N_BANDS],
    flash: f32,
    gap: f32,    // cosmetic bar spacing (audioMotion barSpace)
    height: f32, // cosmetic display scale on the bar height
}

impl Spectrum {
    pub fn new() -> Self {
        Spectrum {
            heights: [0.0; N_BANDS],
            caps: [0.0; N_BANDS],
            cap_vel: [0.0; N_BANDS],
            flash: 0.0,
            gap: 0.22,
            height: 1.0,
        }
    }
}

impl Mode for Spectrum {
    fn name(&self) -> &'static str {
        "Spectrum"
    }

    fn about(&self) -> &'static str {
        "Frequency bars graded by energy — a cool field with one warm hero band."
    }

    fn trail(&self) -> f32 {
        0.10
    }

    fn params(&self) -> Vec<Param> {
        vec![
            Param::float("Bar height", self.height, 0.5, 1.3),
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

    fn reset(&mut self, _track: &Track) {
        self.heights = [0.0; N_BANDS];
        self.caps = [0.0; N_BANDS];
        self.cap_vel = [0.0; N_BANDS];
        self.flash = 0.0;
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let dt = ctx.dt;
        self.flash = (self.flash - dt * 3.0).max(0.0);
        if let Some(s) = ctx.feat.beat {
            if s > 1.8 {
                self.flash = self.flash.max((s * 0.22).min(0.6));
            }
        }
        // Bars track the analysis bands DIRECTLY — the single smoothing already
        // happened once in analysis.rs (the AnalyserNode EMA). No second EMA.
        for i in 0..N_BANDS {
            self.heights[i] = ctx.feat.bands[i];
            // Peak cap: snaps up to the bar, then falls under gravity.
            if self.heights[i] >= self.caps[i] {
                self.caps[i] = self.heights[i];
                self.cap_vel[i] = 0.0;
            } else {
                self.cap_vel[i] -= 2.4 * dt;
                self.caps[i] = (self.caps[i] + self.cap_vel[i] * dt).max(self.heights[i]);
            }
        }
    }

    fn draw(&self, _ctx: &FrameCtx) {
        let v = View::fit_world(AW, AH);
        if self.flash > 0.001 {
            v.rect(0.0, AH, AW, AH, with_alpha(teal_deep(), self.flash * 0.05));
        }

        let base = AH * 0.42; // off-center baseline -> asymmetric negative space
        let max_h = AH * 0.50 * self.height;
        let margin = 0.35;
        let usable = AW - margin * 2.0;
        // Log-ish bar widths (wider lows, narrower highs) instead of 32 clones.
        let weight = |i: usize| 1.0 - 0.5 * (i as f32 / N_BANDS as f32);
        let wsum: f32 = (0..N_BANDS).map(weight).sum();
        let gap = self.gap.clamp(0.0, 0.6);

        let mut x = margin;
        for i in 0..N_BANDS {
            let e = self.heights[i];
            let slot = weight(i) / wsum * usable;
            let jw = 1.0 + (hash01(i as i32 * 7 + 1) - 0.5) * 0.30; // width ±15%
            let bw = (slot * (1.0 - gap) * jw).max(0.04);
            let bx = x + (slot - bw) * 0.5;
            let by = base + (hash01(i as i32 * 13 + 3) - 0.5) * 0.05 * AH; // baseline jitter
            let h = (e * max_h).clamp(0.012, AH - base - 0.3); // stay on-screen

            // Every bar is coloured by its OWN level (energy = colour): quiet teal,
            // loud warms to amber — a consistent gradient, no special "hero" bar.
            let c = grade(0.12 + e * 0.82);
            let a = 0.4 + 0.6 * e;
            // Flat bar — no underglow, no white tip.
            v.rect(bx, by + h, bw, h, with_alpha(c, a));

            // Peak-hold cap, same colour family (no amber dot).
            if e > 0.02 {
                let cap = self.caps[i] * max_h;
                v.rect(bx, by + cap + 0.08, bw, 0.05, with_alpha(grade(0.12 + self.caps[i] * 0.82), 0.7));
            }

            x += slot;
        }
    }
}
