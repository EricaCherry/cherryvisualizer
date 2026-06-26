//! Lava Lamp — slow gooey blobs drifting up and down, swelling with the bass and
//! popping on the beat. Built from stacked translucent circles whose halos merge
//! into soft metaballs over the feedback trail. Warm, on-palette, hypnotic.

use macroquad::prelude::*;

use crate::modes::{FrameCtx, Mode, Param};
use crate::style::{self, amber, grade, hash01, with_alpha};
use crate::track::Track;
use crate::view::{View, AH, AW};

struct Blob {
    x: f32,
    y: f32,
    vy: f32,
    base_r: f32,
    phase: f32,
}

pub struct Lava {
    blobs: Vec<Blob>,
    beat_pulse: f32,
    heat: f32,
    wobble: f32,
    n: i32,
}

impl Lava {
    pub fn new() -> Self {
        let mut l = Lava { blobs: Vec::new(), beat_pulse: 0.0, heat: 1.0, wobble: 1.0, n: 8 };
        l.build();
        l
    }
    fn build(&mut self) {
        self.blobs.clear();
        for i in 0..self.n {
            self.blobs.push(Blob {
                x: 3.5 + hash01(i * 3) * 9.0,
                y: 3.0 + hash01(i * 5) * 3.0,
                vy: (hash01(i * 7) - 0.5) * 1.3,
                base_r: 0.45 + hash01(i * 9) * 0.45,
                phase: hash01(i * 11) * 6.28,
            });
        }
    }
}

impl Mode for Lava {
    fn name(&self) -> &'static str {
        "Lava Lamp"
    }
    fn about(&self) -> &'static str {
        "Gooey blobs drifting up and down, swelling with the bass and popping on the beat."
    }
    fn trail(&self) -> f32 {
        0.14
    }

    fn params(&self) -> Vec<Param> {
        vec![
            Param::int("Blobs", self.n, 4, 12),
            Param::float("Heat", self.heat, 0.0, 2.0),
            Param::float("Wobble", self.wobble, 0.0, 2.0),
        ]
    }
    fn set_param(&mut self, name: &str, v: f32) {
        match name {
            "Blobs" => {
                self.n = (v.round() as i32).clamp(4, 12);
                self.build();
            }
            "Heat" => self.heat = v,
            "Wobble" => self.wobble = v,
            _ => {}
        }
    }

    fn reset(&mut self, _t: &Track) {
        self.build();
        self.beat_pulse = 0.0;
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let dt = ctx.dt;
        let feat = ctx.feat;
        self.beat_pulse = (self.beat_pulse - dt * 3.0).max(0.0);
        if let Some(s) = feat.beat {
            if s > 1.8 {
                self.beat_pulse = 0.4;
            }
        }
        for b in &mut self.blobs {
            b.y += b.vy * dt * (0.5 + feat.bass * 1.5);
            if b.y < 3.0 {
                b.y = 3.0;
                b.vy = b.vy.abs();
            }
            if b.y > 6.0 {
                b.y = 6.0;
                b.vy = -b.vy.abs();
            }
            b.x += (ctx.time * 0.5 + b.phase).sin() * feat.treble * self.wobble * dt;
            b.x = b.x.clamp(3.2, AW - 3.2);
        }
    }

    fn draw(&self, ctx: &FrameCtx) {
        let v = View::fit_world(AW, AH);
        let feat = ctx.feat;
        let swell = 1.0 + feat.bass * 0.4 + self.beat_pulse;
        let mut hero = 0usize;
        let mut hr = 0.0f32;
        for (i, b) in self.blobs.iter().enumerate() {
            let r = b.base_r * swell;
            if r > hr {
                hr = r;
                hero = i;
            }
        }
        for (i, b) in self.blobs.iter().enumerate() {
            let r = b.base_r * swell;
            let c = grade((0.45 + feat.rms * 0.4 * self.heat).min(1.0));
            for &(rm, al) in &[(1.8f32, 0.12f32), (1.5, 0.20), (1.2, 0.40), (1.0, 0.85)] {
                v.circle(b.x, b.y, r * rm, with_alpha(c, al));
            }
            if i == hero {
                style::glow_core(&v, b.x, b.y, r * 0.5, amber());
            }
        }
    }
}
