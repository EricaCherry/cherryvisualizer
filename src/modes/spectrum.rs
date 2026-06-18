//! Spectrum — the classic Winamp-style frequency-bar visualizer.
//!
//! 32 log-spaced bands rise from a center baseline with peak-hold caps that
//! fall under gravity, a translucent reflection below, and a beat-driven
//! background flush. It reads only `Features` (no physics, no profile), so it
//! is the simplest "classic" mode and a clean reference for new ones.

use macroquad::prelude::*;

use crate::analysis::N_BANDS;
use crate::modes::{FrameCtx, Mode, Param};
use crate::track::Track;
use crate::view::{hsl, View, AH, AW, BG};

pub struct Spectrum {
    heights: [f32; N_BANDS],
    caps: [f32; N_BANDS],
    cap_vel: [f32; N_BANDS],
    flash: f32,
    // live-tunable
    gain: f32,
    smooth: f32,
    gap: f32,
}

impl Spectrum {
    pub fn new() -> Self {
        Spectrum {
            heights: [0.0; N_BANDS],
            caps: [0.0; N_BANDS],
            cap_vel: [0.0; N_BANDS],
            flash: 0.0,
            gain: 1.1,
            smooth: 0.45,
            gap: 0.22,
        }
    }
}

impl Mode for Spectrum {
    fn name(&self) -> &'static str {
        "Spectrum"
    }

    fn about(&self) -> &'static str {
        "Classic frequency bars with peak-hold caps and a mirrored reflection."
    }

    fn params(&self) -> Vec<Param> {
        vec![
            Param::float("Gain", self.gain, 0.4, 2.5),
            Param::float("Smoothing", self.smooth, 0.0, 0.9),
            Param::float("Bar gap", self.gap, 0.0, 0.6),
        ]
    }

    fn set_param(&mut self, name: &str, v: f32) {
        match name {
            "Gain" => self.gain = v,
            "Smoothing" => self.smooth = v,
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
        // Bars attack fast, release at a rate the "Smoothing" knob slows.
        let release = (1.0 - self.smooth.clamp(0.0, 0.95)) * 0.5;
        for i in 0..N_BANDS {
            let target = (ctx.feat.bands[i] * self.gain).min(1.0);
            if target > self.heights[i] {
                self.heights[i] += (target - self.heights[i]) * 0.6;
            } else {
                self.heights[i] += (target - self.heights[i]) * release;
            }
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
        let bg = Color::new(BG.r + self.flash * 0.10, BG.g + self.flash * 0.05, BG.b + self.flash * 0.12, 1.0);
        clear_background(bg);

        let base = AH * 0.50;
        let max_h = AH * 0.46;
        let slot = AW / N_BANDS as f32;
        let bw = slot * (1.0 - self.gap.clamp(0.0, 0.9));
        let pad = (slot - bw) * 0.5;

        for i in 0..N_BANDS {
            let h = (self.heights[i] * max_h).max(0.02);
            let x = i as f32 * slot + pad;
            let hue = 0.58 - i as f32 / N_BANDS as f32 * 0.66;
            let c = hsl(hue, 0.55, 0.50 + 0.12 * self.heights[i]);

            // Reflection below the baseline (translucent, half height).
            v.rect(x, base, bw, h * 0.55, Color::new(c.r, c.g, c.b, 0.18));
            // The bar.
            v.rect(x, base + h, bw, h, c);
            // Bright tip.
            let tip = (h * 0.12).min(0.14);
            v.rect(x, base + h, bw, tip, Color::new((c.r + 0.3).min(1.0), (c.g + 0.3).min(1.0), (c.b + 0.3).min(1.0), 1.0));
            // Peak cap.
            let cap = self.caps[i] * max_h;
            v.rect(x, base + cap + 0.07, bw, 0.05, Color::new(0.92, 0.95, 1.0, 0.9));
        }

        v.line(0.0, base, AW, base, 2.0, Color::new(0.5, 0.55, 0.65, 0.45));
    }
}
