//! Galaxy — a spiral of stars wheeling around a glowing core. Mids drive the
//! spin, bass breathes the arms outward, treble twinkles the outer stars, and
//! beats pulse the whole disc. A tilted projection gives it depth.

use macroquad::prelude::*;

use crate::modes::{FrameCtx, Mode, Param};
use crate::style::{self, amber, grade, hash01, mix, teal_deep, with_alpha};
use crate::track::Track;
use crate::view::{View, AH, AW};

struct Star {
    r: f32,    // 0 (core) .. 1 (rim)
    ang: f32,  // base spiral angle
    flick: f32, // twinkle phase 0..1
}

pub struct Galaxy {
    stars: Vec<Star>,
    rot: f32,
    pulse: f32,
    spin: f32,
    n: i32,
    arms: i32,
}

impl Galaxy {
    pub fn new() -> Self {
        let mut g = Galaxy { stars: Vec::new(), rot: 0.0, pulse: 0.0, spin: 1.0, n: 700, arms: 3 };
        g.build();
        g
    }
    fn build(&mut self) {
        self.stars.clear();
        let winding = 6.0;
        for i in 0..self.n {
            let arm = (i % self.arms) as f32;
            let r = hash01(i * 5).powf(0.7);
            let ang = arm / self.arms as f32 * std::f32::consts::TAU + r * winding + (hash01(i * 7) - 0.5) * 0.4;
            self.stars.push(Star { r, ang, flick: hash01(i * 11) });
        }
    }
}

impl Mode for Galaxy {
    fn name(&self) -> &'static str {
        "Galaxy"
    }
    fn about(&self) -> &'static str {
        "A spiral of stars wheeling around a glowing core — mids spin it, treble twinkles."
    }
    fn trail(&self) -> f32 {
        0.16
    }

    fn params(&self) -> Vec<Param> {
        vec![
            Param::int("Stars", self.n, 200, 900),
            Param::float("Spin", self.spin, 0.0, 2.5),
            Param::int("Arms", self.arms, 2, 5),
        ]
    }
    fn set_param(&mut self, name: &str, v: f32) {
        match name {
            "Stars" => {
                self.n = (v.round() as i32).clamp(200, 900);
                self.build();
            }
            "Spin" => self.spin = v,
            "Arms" => {
                self.arms = (v.round() as i32).clamp(2, 5);
                self.build();
            }
            _ => {}
        }
    }

    fn reset(&mut self, _t: &Track) {
        self.rot = 0.0;
        self.pulse = 0.0;
    }

    fn update(&mut self, ctx: &FrameCtx) {
        self.rot += ctx.dt * (0.1 + ctx.feat.mid * 0.6) * self.spin;
        self.pulse = (self.pulse - ctx.dt * 3.0).max(0.0);
        if let Some(s) = ctx.feat.beat
            && s > 1.8 {
                self.pulse = (s * 0.3).min(0.6);
            }
    }

    fn draw(&self, ctx: &FrameCtx) {
        let v = View::fit_world(AW, AH);
        let feat = ctx.feat;
        let t = ctx.time;
        let (cx, cy) = (AW * 0.5, AH * 0.5);
        let tilt = 0.45;
        let breath = 1.0 + feat.bass * 0.3 + self.pulse * 0.2;

        for s in &self.stars {
            let th = s.ang + self.rot;
            let rr = s.r * 3.6 * breath;
            let x = cx + th.cos() * rr;
            let y = cy + th.sin() * rr * tilt;
            let flick = 0.5 + 0.5 * (t * 2.0 + s.flick * std::f32::consts::TAU).sin() * feat.treble;
            let bright = (1.0 - s.r) + flick * 0.3;
            let c = mix(teal_deep(), grade((0.5 + (1.0 - s.r) * 0.4 + flick * 0.3).min(1.0)), 0.7);
            v.circle(x, y, 0.02 + bright * 0.05, with_alpha(c, (0.3 + bright * 0.6).min(1.0)));
        }

        // Galactic bulge: warm bloom + a cream core.
        for &(rr, al) in &[(1.5f32, 0.12f32), (0.9, 0.2), (0.5, 0.32)] {
            v.circle(cx, cy, rr * breath, with_alpha(amber(), al * (0.6 + feat.rms * 0.6)));
        }
        style::glow_core(&v, cx, cy, 0.22 + feat.bass * 0.2, style::spec());
    }
}
