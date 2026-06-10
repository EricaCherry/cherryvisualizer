//! Beat Runner — an endless runner played by the music.
//!
//! The jump/speed physics are ported from Chromium's T-Rex runner
//! (`components/neterror/resources/offline.js`, BSD-3-Clause), rescaled from
//! its 600x150 px canvas into world units. What makes it a visualizer:
//!
//!   - every beat in the track becomes an obstacle, placed (using the offline
//!     beat grid + loudness curve) so it arrives at the runner EXACTLY on the
//!     beat — strong beats spawn big cacti;
//!   - world speed follows the track's loudness;
//!   - nobody is holding the spacebar: the runner jumps itself, timed so the
//!     apex of every jump lands on the beat. The music plays the game.

use macroquad::prelude::*;

use crate::modes::{FrameCtx, Mode};
use crate::track::Track;
use crate::view::{View, AH, AW, BG, WAVE};

// ---- Chromium T-Rex constants (offline.js), px @ 60fps ---------------------
const TREX_SPEED: f32 = 6.0; //          Runner.config.SPEED
const TREX_GRAVITY: f32 = 0.6; //        Trex.config.GRAVITY      (px/frame^2)
const TREX_JUMP_V0: f32 = 10.0; //       Trex.config.INITIAL_JUMP_VELOCITY
const TREX_CANVAS_W: f32 = 600.0; //     Runner.config.DEFAULT_WIDTH

// ---- converted to world units (16x9, seconds) -------------------------------
const PX: f32 = AW / TREX_CANVAS_W;
const BASE_SPEED: f32 = TREX_SPEED * 60.0 * PX; //      ~9.6 wu/s
const GRAVITY: f32 = TREX_GRAVITY * 3600.0 * PX; //     ~57.6 wu/s^2
const JUMP_V0: f32 = TREX_JUMP_V0 * 60.0 * PX; //       ~16 wu/s
const AIR_TIME: f32 = 2.0 * JUMP_V0 / GRAVITY; //       ~0.55 s aloft

const GROUND_Y: f32 = 1.6;
const DINO_X: f32 = 3.0;

struct Obstacle {
    /// The beat this obstacle lands on.
    t: f32,
    /// World distance at which it sits (cumulative-distance space).
    d: f32,
    big: bool,
}

struct Dust {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: f32,
}

pub struct Runner {
    /// Cumulative world distance per profile hop: D[i] = distance at i*hop_dt.
    dist: Vec<f32>,
    hop_dt: f32,
    obstacles: Vec<Obstacle>,
    /// Jump state (height above ground, world units).
    y: f32,
    vy: f32,
    grounded: bool,
    dust: Vec<Dust>,
}

impl Runner {
    pub fn new() -> Self {
        Runner {
            dist: vec![0.0],
            hop_dt: 1.0 / 60.0,
            obstacles: Vec::new(),
            y: 0.0,
            vy: 0.0,
            grounded: true,
            dust: Vec::new(),
        }
    }

    /// Distance traveled by time `t` (interpolated from the precomputed table).
    fn dist_at(&self, t: f32) -> f32 {
        if self.dist.len() < 2 {
            return BASE_SPEED * t;
        }
        let f = (t / self.hop_dt).max(0.0);
        let i = (f as usize).min(self.dist.len() - 2);
        let frac = (f - i as f32).min(1.0);
        self.dist[i] * (1.0 - frac) + self.dist[i + 1] * frac
    }

}

impl Mode for Runner {
    fn name(&self) -> &'static str {
        "Beat Runner"
    }

    fn reset(&mut self, track: &Track) {
        // World speed follows the track's loudness curve; integrate it once so
        // both obstacles and the runner live in cumulative-distance space.
        // (This is what makes "the obstacle arrives ON the beat" exact, even
        // though the speed varies with the music.)
        let p = &track.profile;
        self.hop_dt = p.hop_dt;
        self.dist.clear();
        self.dist.push(0.0);
        let mut d = 0.0f32;
        for h in 0..p.rms.len() {
            let speed = BASE_SPEED * (0.55 + 0.9 * p.loudness_at(h as f32 * p.hop_dt));
            d += speed * p.hop_dt;
            self.dist.push(d);
        }

        self.obstacles = p
            .beats
            .iter()
            .map(|b| Obstacle { t: b.t, d: self.dist_at(b.t), big: b.strength > 1.7 })
            .collect();

        self.y = 0.0;
        self.vy = 0.0;
        self.grounded = true;
        self.dust.clear();
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let t = ctx.time;
        let dt = ctx.dt;

        // Auto-jump: take off half an air-time before the next beat so the
        // apex of the jump is exactly over the obstacle, exactly on the beat.
        let next = self.obstacles.partition_point(|o| o.t <= t);
        if self.grounded {
            if let Some(o) = self.obstacles.get(next) {
                if o.t - t <= AIR_TIME * 0.5 {
                    self.vy = JUMP_V0;
                    self.grounded = false;
                }
            }
        }

        // Jump physics (ported T-Rex constants).
        if !self.grounded {
            self.y += self.vy * dt;
            self.vy -= GRAVITY * dt;
            if self.y <= 0.0 {
                self.y = 0.0;
                self.vy = 0.0;
                self.grounded = true;
                // Landing dust.
                for i in 0..6 {
                    let a = i as f32 / 6.0 * std::f32::consts::PI;
                    self.dust.push(Dust {
                        x: DINO_X + 0.2,
                        y: GROUND_Y + 0.05,
                        vx: -a.cos() * 1.2,
                        vy: a.sin() * 0.7,
                        life: 0.4,
                    });
                }
            }
        }

        for p in &mut self.dust {
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.vy -= 3.0 * dt;
            p.life -= dt;
        }
        self.dust.retain(|p| p.life > 0.0);
    }

    fn draw(&self, ctx: &FrameCtx) {
        let v = View::fit();
        clear_background(BG);
        let t = ctx.time;
        let d_now = self.dist_at(t);

        // Distant skyline = the spectrum, desaturated and quiet. Squaring the
        // energy keeps the (often saturated) bass bands from pegging at max.
        let n = ctx.feat.bands.len();
        let bw = AW / n as f32;
        for (i, &e) in ctx.feat.bands.iter().enumerate() {
            let h = 0.25 + e * e * 1.9;
            let a = 0.09 + 0.09 * ctx.feat.treble;
            v.rect(i as f32 * bw, GROUND_Y + h, bw * 0.9, h, Color::new(0.45, 0.55, 0.70, a));
        }

        // The ground line IS the waveform; mids warm its color.
        let mid = ctx.feat.mid;
        let ground_line = Color::new(
            (WAVE.r + 0.25 * mid).min(1.0),
            (WAVE.g + 0.12 * mid).min(1.0),
            WAVE.b,
            0.95,
        );
        let pts = 96;
        let wave = ctx.wave;
        let mut prev = (0.0f32, GROUND_Y);
        for i in 0..pts {
            let f = i as f32 / (pts - 1) as f32;
            let si = ((f * (wave.len() - 1) as f32) as usize).min(wave.len() - 1);
            let p = (f * AW, GROUND_Y + wave[si] * 0.22);
            if i > 0 {
                v.line(prev.0, prev.1, p.0, p.1, 2.0, ground_line);
            }
            prev = p;
        }
        // Faint solid ground under the line.
        v.rect(0.0, GROUND_Y - 0.05, AW, GROUND_Y - 0.05, Color::new(0.10, 0.11, 0.14, 1.0));

        // Obstacles: cacti at their beat positions.
        let cactus = Color::new(0.36, 0.45, 0.36, 1.0);
        for o in &self.obstacles {
            let x = DINO_X + (o.d - d_now);
            if !(-1.0..AW + 1.0).contains(&x) {
                continue;
            }
            if o.big {
                v.rect(x - 0.30, GROUND_Y + 1.30, 0.22, 1.30, cactus);
                v.rect(x - 0.05, GROUND_Y + 1.05, 0.22, 1.05, cactus);
                v.rect(x + 0.20, GROUND_Y + 1.20, 0.22, 1.20, cactus);
            } else {
                v.rect(x - 0.10, GROUND_Y + 0.90, 0.24, 0.90, cactus);
            }
        }

        // The dino (blocky, procedural — light gray, no sprite assets).
        let body = Color::new(0.85, 0.84, 0.80, 1.0);
        let dark = Color::new(0.10, 0.10, 0.10, 1.0);
        let base = GROUND_Y + self.y;
        let run_phase = ((d_now * 4.0) as i32) % 2 == 0;
        // legs
        if self.grounded {
            let (l0, l1) = if run_phase { (0.32, 0.18) } else { (0.18, 0.32) };
            v.rect(DINO_X - 0.28, base + l0, 0.14, l0, body);
            v.rect(DINO_X + 0.06, base + l1, 0.14, l1, body);
        } else {
            v.rect(DINO_X - 0.28, base + 0.16, 0.14, 0.16, body);
            v.rect(DINO_X + 0.06, base + 0.16, 0.14, 0.16, body);
        }
        // tail, body, head
        v.rect(DINO_X - 0.62, base + 0.78, 0.26, 0.30, body);
        v.rect(DINO_X - 0.40, base + 0.95, 0.80, 0.68, body);
        v.rect(DINO_X + 0.18, base + 1.42, 0.50, 0.42, body);
        // eye
        v.rect(DINO_X + 0.48, base + 1.34, 0.08, 0.08, dark);

        // Dust.
        for p in &self.dust {
            v.circle(p.x, p.y, 0.04, Color::new(0.7, 0.7, 0.7, p.life * 1.6));
        }

        // Score: beats cleared so far.
        let passed = self.obstacles.partition_point(|o| o.t <= t);
        let text = format!("{passed}");
        let dim = measure_text(&text, None, 28, 1.0);
        let (sx, sy) = v.xy(AW - 0.35, AH - 0.35);
        draw_text(&text, sx - dim.width, sy, 28.0, Color::new(1.0, 1.0, 1.0, 0.5));
    }
}
