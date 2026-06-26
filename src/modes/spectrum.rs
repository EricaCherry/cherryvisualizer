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
use crate::style::{amber, hash01, mix, smoothstep, spec, teal, teal_deep, with_alpha};
use crate::track::Track;
use crate::view::{View, AH, AW};

pub struct Spectrum {
    heights: [f32; N_BANDS],
    caps: [f32; N_BANDS],
    cap_vel: [f32; N_BANDS],
    flash: f32,
    last_hero: usize,
    gap: f32, // cosmetic bar spacing (audioMotion barSpace)
}

impl Spectrum {
    pub fn new() -> Self {
        Spectrum {
            heights: [0.0; N_BANDS],
            caps: [0.0; N_BANDS],
            cap_vel: [0.0; N_BANDS],
            flash: 0.0,
            last_hero: 0,
            gap: 0.22,
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
        vec![Param::float("Bar gap", self.gap, 0.0, 0.6)]
    }

    fn set_param(&mut self, name: &str, v: f32) {
        if name == "Bar gap" {
            self.gap = v;
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
        // Hero = loudest INNER band, with hysteresis so the amber flash doesn't
        // strobe between two near-equal bands.
        let cand = (2..N_BANDS - 2).max_by(|&a, &b| self.heights[a].total_cmp(&self.heights[b])).unwrap_or(2);
        if self.heights[cand] > self.heights[self.last_hero] * 1.15 {
            self.last_hero = cand;
        }
    }

    fn draw(&self, _ctx: &FrameCtx) {
        let v = View::fit_world(AW, AH);
        if self.flash > 0.001 {
            // Beats breathe the cool body faintly; amber stays reserved for the
            // hero, so the negative space never warms.
            v.rect(0.0, AH, AW, AH, with_alpha(teal_deep(), self.flash * 0.05));
        }

        // The hero is the single loudest band (chosen with hysteresis in update).
        let hero = self.last_hero;

        let base = AH * 0.42; // off-center baseline -> asymmetric negative space
        let max_h = AH * 0.50; // keep full-scale bars (+ caps) inside the 16x9 world
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
            let h = (e * max_h).max(0.012);
            let hero_bar = i == hero && e > 0.05;

            // Body is one cool family separated by brightness; only the hero
            // bar tips toward amber as it gets loud.
            let mut c = mix(teal_deep(), teal(), smoothstep(0.04, 0.78, e));
            if hero_bar {
                c = mix(c, amber(), smoothstep(0.30, 0.95, e));
            }

            // Quiet bars recede toward the backdrop so the loud (and hero) bars
            // own the value; the hero stays fully solid.
            let bar_a = if hero_bar { 1.0 } else { 0.35 + 0.65 * e };
            // Short graded underglow (replaces the stacked-alpha mirror).
            v.rect(bx, by, bw, (h * 0.16).min(0.45), with_alpha(c, 0.10 * bar_a));
            // The bar.
            v.rect(bx, by + h, bw, h, with_alpha(c, bar_a));
            // Tip lifted within the bar's own family (no white).
            v.rect(bx, by + h, bw, (h * 0.10).min(0.10), with_alpha(mix(c, spec(), 0.30), bar_a));

            // Caps: dim teal ticks, except the hero = amber cap + cream tip.
            let cap = self.caps[i] * max_h;
            if hero_bar {
                v.rect(bx, by + cap + 0.10, bw, 0.07, amber());
                v.rect(bx + bw * 0.28, by + cap + 0.18, bw * 0.44, 0.05, spec());
            } else if e > 0.02 {
                v.rect(bx, by + cap + 0.08, bw, 0.045, with_alpha(teal(), 0.5));
            }

            x += slot;
        }
    }
}
