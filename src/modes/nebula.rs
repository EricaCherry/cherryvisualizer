//! Nebula — a particle bloom field. Each frame the treble sprays sparks out
//! from a luminous core; every spark's direction comes from its frequency band
//! and its color from that band's energy, the mids swirl the whole cloud, and a
//! beat bursts it brighter. The feedback trails turn the sparks into smoky gas.

use macroquad::prelude::*;

use crate::analysis::N_BANDS;
use crate::modes::{FrameCtx, Mode, Param};
use crate::style::{self, amber, grade, ink, mix, with_alpha};
use crate::track::Track;
use crate::view::{View, AH, AW};

const CAP: usize = 700;

struct P {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: f32,
    max: f32,
    e: f32,
}

pub struct Nebula {
    parts: Vec<P>,
    seed: u32,
    spawn_accum: f32,
    bloom: f32,
    density: f32,
    drift: f32,
}

impl Nebula {
    pub fn new() -> Self {
        Nebula { parts: Vec::new(), seed: 0x2468abcd, spawn_accum: 0.0, bloom: 0.0, density: 440.0, drift: 1.0 }
    }
    fn rand(&mut self) -> f32 {
        self.seed = self.seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.seed >> 8) as f32 / 16_777_216.0
    }
}

impl Mode for Nebula {
    fn name(&self) -> &'static str {
        "Nebula"
    }
    fn about(&self) -> &'static str {
        "A particle bloom field — each band sprays sparks in its own direction; beats burst the cloud."
    }
    fn trail(&self) -> f32 {
        0.2
    }

    fn params(&self) -> Vec<Param> {
        vec![Param::float("Density", self.density, 80.0, 600.0), Param::float("Drift", self.drift, 0.2, 2.0)]
    }
    fn set_param(&mut self, name: &str, v: f32) {
        match name {
            "Density" => self.density = v,
            "Drift" => self.drift = v,
            _ => {}
        }
    }

    fn reset(&mut self, _t: &Track) {
        self.parts.clear();
        self.seed = 0x2468abcd;
        self.spawn_accum = 0.0;
        self.bloom = 0.0;
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let dt = ctx.dt;
        self.bloom = (self.bloom - dt * 1.6).max(0.0);
        if let Some(s) = ctx.feat.beat {
            self.bloom = self.bloom.max((s * 0.5).min(1.5));
        }

        // Spawn rate rides the treble + bloom; each spark's angle = its band.
        self.spawn_accum += ctx.feat.treble * self.density * (1.0 + self.bloom) * dt;
        let (cx, cy) = (AW * 0.5, AH * 0.5);
        while self.spawn_accum >= 1.0 && self.parts.len() < CAP {
            self.spawn_accum -= 1.0;
            let band = (self.rand() * N_BANDS as f32) as usize % N_BANDS;
            let e = ctx.feat.bands[band];
            let ang = band as f32 / N_BANDS as f32 * std::f32::consts::TAU + (self.rand() - 0.5) * 0.5;
            let spd = (0.6 + e * 3.2) * self.drift * (1.0 + self.bloom);
            let jx = (self.rand() - 0.5) * 0.3;
            let jy = (self.rand() - 0.5) * 0.3;
            let max = 1.1 + self.rand() * 1.1;
            self.parts.push(P { x: cx + jx, y: cy + jy, vx: ang.cos() * spd, vy: ang.sin() * spd, life: 0.0, max, e });
        }

        // Drag + a gentle mid-driven curl swirl the cloud.
        let curl = ctx.feat.mid * dt * 0.9;
        let (cc, sc) = (curl.cos(), curl.sin());
        for p in &mut self.parts {
            let (nvx, nvy) = (p.vx * cc - p.vy * sc, p.vx * sc + p.vy * cc);
            p.vx = nvx * 0.985;
            p.vy = nvy * 0.985;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.life += dt;
        }
        self.parts.retain(|p| p.life < p.max && p.x > -2.0 && p.x < AW + 2.0 && p.y > -2.0 && p.y < AH + 2.0);
    }

    fn draw(&self, _ctx: &FrameCtx) {
        let v = View::fit_world(AW, AH);
        // The luminous heart the cloud streams from.
        style::glow_core(&v, AW * 0.5, AH * 0.5, 0.12 + self.bloom * 0.22, amber());

        // Brightest live particle is the hero.
        let hero = self
            .parts
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.e.total_cmp(&b.e))
            .map(|(i, _)| i);

        for (i, p) in self.parts.iter().enumerate() {
            let k = (1.0 - p.life / p.max).clamp(0.0, 1.0);
            let c = grade(0.2 + p.e * 0.7 + self.bloom * 0.3);
            let r = 0.04 + p.e * 0.13;
            v.circle(p.x, p.y, r * 2.3, with_alpha(mix(ink(), c, 0.5), k * 0.22));
            v.circle(p.x, p.y, r, with_alpha(c, k * 0.9));
            if Some(i) == hero {
                style::glow_core(&v, p.x, p.y, r.max(0.08), amber());
            }
        }
    }
}
