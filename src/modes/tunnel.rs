//! Tunnel — the demoscene staple: concentric rings rushing the camera, twisting
//! on the treble and punching into warp on the beat. Pure 2D projection.

use macroquad::prelude::*;

use crate::modes::{FrameCtx, Mode, Param};
use crate::style::{self, amber, grade, with_alpha};
use crate::track::Track;
use crate::view::{View, AH, AW};

const N: usize = 64;
const SEG: usize = 32;
const NEAR: f32 = 0.45;
const FAR: f32 = 16.0;

pub struct Tunnel {
    rings: Vec<f32>, // z-depths, evenly spaced, recycled
    spin: f32,
    warp: f32,
    speed: f32,
    twist: f32,
}

impl Tunnel {
    pub fn new() -> Self {
        let mut t = Tunnel { rings: Vec::new(), spin: 0.0, warp: 0.0, speed: 5.0, twist: 1.0 };
        t.respawn();
        t
    }

    fn respawn(&mut self) {
        self.rings.clear();
        for i in 0..N {
            self.rings.push(NEAR + (i as f32 / N as f32) * (FAR - NEAR));
        }
    }
}

impl Mode for Tunnel {
    fn name(&self) -> &'static str {
        "Tunnel"
    }
    fn about(&self) -> &'static str {
        "Demoscene rings rushing the camera; treble twists them, beats punch to warp."
    }
    fn trail(&self) -> f32 {
        0.18
    }

    fn params(&self) -> Vec<Param> {
        vec![Param::float("Speed", self.speed, 1.5, 10.0), Param::float("Twist", self.twist, 0.0, 3.0)]
    }
    fn set_param(&mut self, name: &str, v: f32) {
        match name {
            "Speed" => self.speed = v,
            "Twist" => self.twist = v,
            _ => {}
        }
    }

    fn reset(&mut self, _t: &Track) {
        self.respawn();
        self.spin = 0.0;
        self.warp = 0.0;
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let dt = ctx.dt;
        self.warp = (self.warp - dt * 2.2).max(0.0);
        if let Some(s) = ctx.feat.beat {
            self.warp = self.warp.max((s * 0.5).min(1.4));
        }
        self.spin += dt * (0.3 + ctx.feat.treble * 1.6) * self.twist;
        let v = self.speed * (0.4 + ctx.feat.rms * 1.8) * (1.0 + self.warp);
        for z in &mut self.rings {
            *z -= v * dt;
            if *z <= NEAR {
                *z += FAR - NEAR;
            }
        }
    }

    fn draw(&self, ctx: &FrameCtx) {
        // Content only — the shared pipeline owns the backdrop + finish.
        let v = View::fit_world(AW, AH);
        let feat = ctx.feat;
        let (cx, cy) = (AW * 0.47, AH * 0.46); // off-center vanishing point
        let focal = AW * 0.55;

        for &z in &self.rings {
            let depth = 1.0 - z / FAR; // 0 far .. 1 near
            let r = (focal / z).min(AW * 2.0);
            let a = (depth * depth * 0.95).clamp(0.0, 1.0);
            let c = with_alpha(grade(depth * 0.5 + feat.rms * 0.45), a);
            let thick = (1.0 + depth * 4.0).clamp(1.0, 6.0);
            let twist = self.spin * (0.4 + depth);
            let mut prev: Option<(f32, f32)> = None;
            for k in 0..=SEG {
                let ang = k as f32 / SEG as f32 * std::f32::consts::TAU + twist;
                let p = (cx + ang.cos() * r, cy + ang.sin() * r);
                if let Some((px, py)) = prev {
                    v.line(px, py, p.0, p.1, thick, c);
                }
                prev = Some(p);
            }
        }
        // The eye of the tunnel is the hero.
        style::glow_core(&v, cx, cy, 0.18 + self.warp * 0.25, amber());
    }
}
