//! Ring of Fire — the spectrum wrapped into a ring of glowing spikes over a
//! bass-driven corona. Treble spikes flare outward, the loudest band is the
//! cream-hot hero, and beats bloom the whole ring. Warmer and more dramatic than
//! the cool Polar Spectrum.

use macroquad::prelude::*;

use crate::analysis::N_BANDS;
use crate::modes::{FrameCtx, Mode, Param};
use crate::style::{self, amber, grade, ink, mix, with_alpha};
use crate::track::Track;
use crate::view::{View, AH, AW};

pub struct RingFire {
    heights: [f32; N_BANDS],
    inner: f32,
    bass_glow: f32,
    flash: f32,
}

impl RingFire {
    pub fn new() -> Self {
        RingFire { heights: [0.0; N_BANDS], inner: 1.2, bass_glow: 1.0, flash: 0.0 }
    }
}

impl Mode for RingFire {
    fn name(&self) -> &'static str {
        "Ring of Fire"
    }
    fn about(&self) -> &'static str {
        "The spectrum as a ring of glowing spikes over a pulsing bass corona."
    }
    fn trail(&self) -> f32 {
        0.15
    }

    fn params(&self) -> Vec<Param> {
        vec![
            Param::float("Inner radius", self.inner, 0.6, 1.8),
            Param::float("Bass glow", self.bass_glow, 0.0, 1.0),
        ]
    }
    fn set_param(&mut self, name: &str, v: f32) {
        match name {
            "Inner radius" => self.inner = v,
            "Bass glow" => self.bass_glow = v,
            _ => {}
        }
    }

    fn reset(&mut self, _t: &Track) {
        self.heights = [0.0; N_BANDS];
        self.flash = 0.0;
    }

    fn update(&mut self, ctx: &FrameCtx) {
        // Spikes track the analysis bands directly (single smoothing is upstream).
        for i in 0..N_BANDS {
            self.heights[i] = ctx.feat.bands[i];
        }
        self.flash = (self.flash - ctx.dt * 4.0).max(0.0);
        if let Some(s) = ctx.feat.beat {
            if s > 1.8 {
                self.flash = (s * 0.3).min(0.7);
            }
        }
    }

    fn draw(&self, ctx: &FrameCtx) {
        let v = View::fit_world(AW, AH);
        let feat = ctx.feat;
        let (cx, cy) = (AW * 0.5, AH * 0.5);
        let r0 = (self.inner * (1.0 + feat.bass * 0.4 + self.flash * 0.2)).min(2.4);

        // Bass corona behind the ring.
        for &(rr, al) in &[(2.0f32, 0.10f32), (1.6, 0.16), (1.2, 0.26)] {
            v.circle(cx, cy, r0 * rr, with_alpha(mix(ink(), amber(), 0.6), al * feat.bass * self.bass_glow));
        }
        // Inner ring outline.
        let segs = 48;
        let mut prev: Option<(f32, f32)> = None;
        for kk in 0..=segs {
            let a = kk as f32 / segs as f32 * std::f32::consts::TAU;
            let p = (cx + a.cos() * r0, cy + a.sin() * r0);
            if let Some((px, py)) = prev {
                v.line(px, py, p.0, p.1, v.s(0.03), with_alpha(grade(0.4 + feat.bass * 0.4), 0.5 + self.flash * 0.3));
            }
            prev = Some(p);
        }
        // Spikes — 32 bands mirrored into 64 angles so the ring is symmetric.
        let hero = (0..N_BANDS).max_by(|&a, &b| self.heights[a].total_cmp(&self.heights[b])).unwrap_or(0);
        for kk in 0..64 {
            let i = if kk < 32 { kk } else { 63 - kk };
            let ang = kk as f32 / 64.0 * std::f32::consts::TAU;
            let e = self.heights[i];
            let len = 0.3 + e * 1.6;
            let (c0, s0) = (ang.cos(), ang.sin());
            let c = grade((e + feat.rms * 0.4).min(1.0));
            v.line(cx + c0 * r0, cy + s0 * r0, cx + c0 * (r0 + len), cy + s0 * (r0 + len), v.s(0.05 + e * 0.12), c);
            if i == hero && e > 0.12 {
                style::glow_core(&v, cx + c0 * (r0 + len), cy + s0 * (r0 + len), 0.1, amber());
            }
        }
        // Hot core.
        style::glow_core(&v, cx, cy, 0.2 + feat.bass * 0.25, amber());
    }
}
