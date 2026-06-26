//! Polar Spectrum — the 32-band spectrum wrapped into a mirrored ring, slowly
//! rotating with the mids. Quiet bands hug the rim in teal; loud bands shoot
//! outward and warm; the single loudest band is the hero.

use macroquad::prelude::*;

use crate::analysis::N_BANDS;
use crate::modes::{FrameCtx, Mode, Param};
use crate::style::{grade, teal_deep, with_alpha};
use crate::track::Track;
use crate::view::{View, AH, AW};

pub struct Radial {
    heights: [f32; N_BANDS],
    caps: [f32; N_BANDS],
    cap_vel: [f32; N_BANDS],
    rot: f32,
    flash: f32,
    inner: f32,
}

impl Radial {
    pub fn new() -> Self {
        Radial {
            heights: [0.0; N_BANDS],
            caps: [0.0; N_BANDS],
            cap_vel: [0.0; N_BANDS],
            rot: 0.0,
            flash: 0.0,
            inner: 1.7,
        }
    }
}

impl Mode for Radial {
    fn name(&self) -> &'static str {
        "Polar Spectrum"
    }
    fn about(&self) -> &'static str {
        "The spectrum wrapped into a mirrored ring; the loudest band shoots out as the hero."
    }
    fn trail(&self) -> f32 {
        0.10
    }

    fn params(&self) -> Vec<Param> {
        vec![Param::float("Inner radius", self.inner, 0.8, 2.0)]
    }
    fn set_param(&mut self, name: &str, v: f32) {
        if name == "Inner radius" {
            self.inner = v;
        }
    }

    fn reset(&mut self, _t: &Track) {
        self.heights = [0.0; N_BANDS];
        self.caps = [0.0; N_BANDS];
        self.cap_vel = [0.0; N_BANDS];
        self.rot = 0.0;
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
        self.rot += dt * (0.05 + ctx.feat.mid * 0.4);
        // Spokes track the analysis bands directly (single smoothing is upstream).
        for i in 0..N_BANDS {
            self.heights[i] = ctx.feat.bands[i];
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
        let (cx, cy) = (AW * 0.5, AH * 0.5);
        let r0 = self.inner * (1.0 + self.flash * 0.12);
        let maxlen = 2.0;

        // Faint inner ring.
        let segs = 48;
        let mut prev: Option<(f32, f32)> = None;
        for k in 0..=segs {
            let a = k as f32 / segs as f32 * std::f32::consts::TAU;
            let p = (cx + a.cos() * r0, cy + a.sin() * r0);
            if let Some((px, py)) = prev {
                v.line(px, py, p.0, p.1, 1.5, with_alpha(teal_deep(), 0.4 + self.flash * 0.3));
            }
            prev = Some(p);
        }

        for i in 0..N_BANDS {
            let h = self.heights[i];
            let c = grade(h); // every spoke coloured by its own level
            for mir in [1.0f32, -1.0] {
                let ang = self.rot + mir * (i as f32 / N_BANDS as f32) * std::f32::consts::PI;
                let (ca, sa) = (ang.cos(), ang.sin());
                let b = (r0 + h * maxlen).min(AH * 0.5 - 0.25);
                let thick = 2.5 + h * 3.0; // louder spokes a touch bolder
                v.line(cx + ca * r0, cy + sa * r0, cx + ca * b, cy + sa * b, thick, c);
                let cap = (r0 + self.caps[i] * maxlen).min(AH * 0.5 - 0.25);
                v.circle(cx + ca * cap, cy + sa * cap, 0.04, with_alpha(c, 0.7));
            }
        }
    }
}
