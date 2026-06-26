//! Polar Spectrum — the 32-band spectrum wrapped into a mirrored ring, slowly
//! rotating with the mids. Quiet bands hug the rim in teal; loud bands shoot
//! outward and warm; the single loudest band is the hero.

use macroquad::prelude::*;

use crate::analysis::N_BANDS;
use crate::modes::{FrameCtx, Mode, Param, focus_band};
use crate::style::{self, amber, grade, teal_deep, with_alpha};
use crate::track::Track;
use crate::view::{View, AH, AW};

pub struct Radial {
    heights: [f32; N_BANDS],
    caps: [f32; N_BANDS],
    cap_vel: [f32; N_BANDS],
    rot: f32,
    flash: f32,
    mid_s: f32,
    last_hero: usize,
    gain: f32,
    smooth: f32,
    inner: f32,
    focus: f32,
}

impl Radial {
    pub fn new() -> Self {
        Radial {
            heights: [0.0; N_BANDS],
            caps: [0.0; N_BANDS],
            cap_vel: [0.0; N_BANDS],
            rot: 0.0,
            flash: 0.0,
            mid_s: 0.0,
            last_hero: 0,
            gain: 1.0,
            smooth: 0.5,
            inner: 1.7,
            focus: 0.0,
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
        vec![
            Param::float("Gain", self.gain, 0.4, 2.5),
            Param::float("Smoothing", self.smooth, 0.0, 0.9),
            Param::float("Inner radius", self.inner, 0.8, 2.0),
            Param::float("Focus", self.focus, 0.0, 1.0),
        ]
    }
    fn set_param(&mut self, name: &str, v: f32) {
        match name {
            "Gain" => self.gain = v,
            "Smoothing" => self.smooth = v,
            "Inner radius" => self.inner = v,
            "Focus" => self.focus = v,
            _ => {}
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
        // Smooth the rotation driver so the spin doesn't twitch on band jitter.
        self.mid_s += (ctx.feat.mid - self.mid_s) * (1.0 - (-dt / 0.15).exp());
        self.rot += dt * (0.05 + self.mid_s * 0.4);
        // ONE symmetric EMA — the Web-Audio smoothingTimeConstant (Smoothing slider).
        let tc = self.smooth.clamp(0.0, 0.95);
        let k = 1.0 - tc.powf(dt * 60.0);
        for i in 0..N_BANDS {
            let target = (focus_band(&ctx.feat.bands, i, self.focus) * self.gain).min(1.0);
            self.heights[i] += (target - self.heights[i]) * k;
            if self.heights[i] >= self.caps[i] {
                self.caps[i] = self.heights[i];
                self.cap_vel[i] = 0.0;
            } else {
                self.cap_vel[i] -= 2.4 * dt;
                self.caps[i] = (self.caps[i] + self.cap_vel[i] * dt).max(self.heights[i]);
            }
        }
        // Hero = loudest inner band, with hysteresis (no strobing between equals).
        let cand = (2..N_BANDS - 2).max_by(|&a, &b| self.heights[a].total_cmp(&self.heights[b])).unwrap_or(2);
        if self.heights[cand] > self.heights[self.last_hero] * 1.15 {
            self.last_hero = cand;
        }
    }

    fn draw(&self, _ctx: &FrameCtx) {
        let v = View::fit_world(AW, AH);
        let (cx, cy) = (AW * 0.5, AH * 0.5);
        let r0 = self.inner * (1.0 + self.flash * 0.12);
        let maxlen = 2.0;
        let hero = self.last_hero;

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
            let c = grade(h);
            let is_hero = i == hero && h > 0.05;
            for mir in [1.0f32, -1.0] {
                let ang = self.rot + mir * (i as f32 / N_BANDS as f32) * std::f32::consts::PI;
                let (ca, sa) = (ang.cos(), ang.sin());
                let b = (r0 + h * maxlen).min(AH * 0.5 - 0.25);
                let thick = if is_hero { 5.0 } else { 3.0 };
                v.line(cx + ca * r0, cy + sa * r0, cx + ca * b, cy + sa * b, thick, c);
                if is_hero {
                    style::glow_core(&v, cx + ca * b, cy + sa * b, 0.12, amber());
                } else {
                    let cap = (r0 + self.caps[i] * maxlen).min(AH * 0.5 - 0.25);
                    v.circle(cx + ca * cap, cy + sa * cap, 0.04, with_alpha(c, 0.7));
                }
            }
        }
    }
}
