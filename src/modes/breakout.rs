//! Waveform Breakout — real breakout, played by the audio.
//!
//! Classic breakout has NO gravity: the ball travels in straight lines at a
//! constant speed and reflects off whatever it hits. Here there is no player
//! and no paddle sprite — the live waveform IS the paddle. It forms a
//! full-width deforming surface along the bottom; when the ball comes down and
//! hits it, the waveform's slope at that point steers the bounce back up into
//! the wall. The ball breaks bricks (each column lit by its frequency band);
//! the wall is large and does NOT regenerate, so a song slowly demolishes it.
//!
//! The music plays the game two ways: its waveform shapes the paddle every
//! frame (where and how the ball is sent back), and its loudness sets the ball
//! speed — so the rally surges in loud passages and eases in quiet ones.

use macroquad::prelude::*;
use rapier2d::prelude::*;
use std::collections::VecDeque;
use std::sync::mpsc::{channel, Receiver};

use crate::analysis::N_BANDS;
use crate::modes::{FrameCtx, Mode};
use crate::track::Track;
use crate::view::{hsl, View, AH, AW, BG, WAVE};

const BALL_R: f32 = 0.26;
const PADDLE_BASE_Y: f32 = 1.0;
const PADDLE_AMP: f32 = 0.9;
const PADDLE_FLOOR: f32 = 0.18;
const WAVE_PTS: usize = 80;

const BALL_SPEED: f32 = 5.6; // base world units/sec; loudness adds to it
const COLS: usize = 22;
const ROWS: usize = 10;
const BRICK_TOP: f32 = 8.6;
const BRICK_BOTTOM: f32 = 3.9;
const TRAIL_LEN: usize = 16;

struct Brick {
    handle: ColliderHandle,
    x: f32,
    y: f32,
    hw: f32,
    hh: f32,
    band: usize,
    color: Color,
    alive: bool,
    /// Draw scale, eased to 0 when broken (a quick shrink-pop).
    anim: f32,
}

struct Shard {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    w: f32,
    life: f32,
    color: Color,
}

pub struct Breakout {
    bodies: RigidBodySet,
    colliders: ColliderSet,
    islands: IslandManager,
    broad: BroadPhaseBvh,
    narrow: NarrowPhase,
    impulse_joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
    ccd: CCDSolver,
    pipeline: PhysicsPipeline,
    params: IntegrationParameters,

    ball: RigidBodyHandle,
    ball_collider: ColliderHandle,
    paddle: ColliderHandle,
    bricks: Vec<Brick>,
    total_bricks: u32,

    col_recv: Receiver<CollisionEvent>,
    _force_recv: Receiver<ContactForceEvent>,
    events: ChannelEventCollector,

    paddle_world: Vec<(f32, f32)>,
    trail: VecDeque<(f32, f32)>,
    shards: Vec<Shard>,
    paddle_flash: f32,
    boost: f32,
    score: u32,
}

fn hash01(n: i32) -> f32 {
    let mut x = n.wrapping_mul(374761393).wrapping_add(668265263) as u32;
    x = (x ^ (x >> 13)).wrapping_mul(1274126177);
    ((x ^ (x >> 16)) & 0xffff) as f32 / 65535.0
}

impl Breakout {
    pub fn new() -> Self {
        let mut bodies = RigidBodySet::new();
        let mut colliders = ColliderSet::new();
        let static_body = bodies.insert(RigidBodyBuilder::fixed().build());

        // Walls: left, right, top. The bottom is the waveform paddle.
        let t = 0.5;
        for (cx, cy, hw, hh) in [
            (-t, AH / 2.0, t, AH),
            (AW + t, AH / 2.0, t, AH),
            (AW / 2.0, AH + t, AW, t),
        ] {
            let c = ColliderBuilder::cuboid(hw, hh)
                .translation(Vector::new(cx, cy))
                .restitution(1.0)
                .friction(0.0)
                .build();
            colliders.insert_with_parent(c, static_body, &mut bodies);
        }

        // The waveform paddle: a polyline reshaped every frame via set_shape().
        let verts: Vec<Vector> = (0..WAVE_PTS)
            .map(|i| Vector::new(i as f32 / (WAVE_PTS - 1) as f32 * AW, PADDLE_BASE_Y))
            .collect();
        let paddle_col = ColliderBuilder::polyline(verts, None)
            .restitution(1.0)
            .friction(0.0)
            .active_events(ActiveEvents::COLLISION_EVENTS)
            .build();
        let paddle = colliders.insert_with_parent(paddle_col, static_body, &mut bodies);

        // A large brick wall, one spectrum band per column.
        let margin = 0.6f32;
        let area_w = AW - margin * 2.0;
        let slot = area_w / COLS as f32;
        let bw = slot * 0.9 / 2.0;
        let rgap = (BRICK_TOP - BRICK_BOTTOM) / ROWS as f32;
        let bh = rgap * 0.62 / 2.0;
        let mut bricks = Vec::new();
        for r in 0..ROWS {
            for c in 0..COLS {
                let x = margin + (c as f32 + 0.5) * slot;
                let y = BRICK_BOTTOM + (r as f32 + 0.5) * rgap;
                let col = ColliderBuilder::cuboid(bw, bh)
                    .translation(Vector::new(x, y))
                    .restitution(1.0)
                    .friction(0.0)
                    .active_events(ActiveEvents::COLLISION_EVENTS)
                    .build();
                let handle = colliders.insert_with_parent(col, static_body, &mut bodies);
                bricks.push(Brick {
                    handle,
                    x,
                    y,
                    hw: bw,
                    hh: bh,
                    band: (c * N_BANDS / COLS).min(N_BANDS - 1),
                    color: hsl(0.58 - c as f32 / COLS as f32 * 0.62, 0.5, 0.55),
                    alive: true,
                    anim: 1.0,
                });
            }
        }
        let total_bricks = bricks.len() as u32;

        // The ball — no gravity; speed is held constant each frame.
        let ball_rb = RigidBodyBuilder::dynamic()
            .translation(Vector::new(AW / 2.0, 2.8))
            .linvel(Vector::new(2.2, BALL_SPEED))
            .ccd_enabled(true)
            .build();
        let ball = bodies.insert(ball_rb);
        let ball_col = ColliderBuilder::ball(BALL_R)
            .restitution(1.0)
            .restitution_combine_rule(CoefficientCombineRule::Max)
            .friction(0.0)
            .active_events(ActiveEvents::COLLISION_EVENTS)
            .build();
        let ball_collider = colliders.insert_with_parent(ball_col, ball, &mut bodies);

        let (col_send, col_recv) = channel();
        let (force_send, _force_recv) = channel();
        let events = ChannelEventCollector::new(col_send, force_send);

        let mut params = IntegrationParameters::default();
        params.dt = 1.0 / 60.0;

        Breakout {
            bodies,
            colliders,
            islands: IslandManager::new(),
            broad: BroadPhaseBvh::new(),
            narrow: NarrowPhase::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            ccd: CCDSolver::new(),
            pipeline: PhysicsPipeline::new(),
            params,
            ball,
            ball_collider,
            paddle,
            bricks,
            total_bricks,
            col_recv,
            _force_recv,
            events,
            paddle_world: vec![(0.0, PADDLE_BASE_Y); WAVE_PTS],
            trail: VecDeque::new(),
            shards: Vec::new(),
            paddle_flash: 0.0,
            boost: 1.0,
            score: 0,
        }
    }

    fn serve(&mut self) {
        let dir = if macroquad::rand::gen_range(0.0, 1.0) < 0.5 { -1.0 } else { 1.0 };
        let vx = macroquad::rand::gen_range(1.6, 2.8) * dir;
        if let Some(rb) = self.bodies.get_mut(self.ball) {
            rb.set_translation(Vector::new(AW / 2.0, 2.8), true);
            rb.set_linvel(Vector::new(vx, BALL_SPEED), true);
        }
    }

    fn spawn_shards(&mut self, idx: usize) {
        let b = &self.bricks[idx];
        for k in 0..7 {
            let a = (k as f32 / 7.0 - 0.5) * std::f32::consts::PI + std::f32::consts::FRAC_PI_2;
            self.shards.push(Shard {
                x: b.x,
                y: b.y,
                vx: a.cos() * (1.5 + hash01(k * 31) * 2.0),
                vy: a.sin() * (1.0 + hash01(k * 17) * 2.5),
                w: 0.10 + hash01(k * 7) * 0.10,
                life: 0.6,
                color: b.color,
            });
        }
    }
}

impl Mode for Breakout {
    fn name(&self) -> &'static str {
        "Breakout"
    }

    fn reset(&mut self, _track: &Track) {
        for b in &mut self.bricks {
            b.alive = true;
            b.anim = 1.0;
            if let Some(c) = self.colliders.get_mut(b.handle) {
                c.set_sensor(false);
            }
        }
        self.score = 0;
        self.boost = 1.0;
        self.trail.clear();
        self.shards.clear();
        self.serve();
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let feat = ctx.feat;
        let dt = ctx.dt;
        self.paddle_flash = (self.paddle_flash - dt * 4.0).max(0.0);
        self.boost += (1.0 - self.boost) * (dt * 3.0).min(1.0);

        // 1) Reshape the waveform paddle from the live window.
        let wave = ctx.wave;
        let mut verts: Vec<Vector> = Vec::with_capacity(WAVE_PTS);
        for i in 0..WAVE_PTS {
            let f = i as f32 / (WAVE_PTS - 1) as f32;
            let x = f * AW;
            let si = ((f * (wave.len().saturating_sub(1)) as f32) as usize)
                .min(wave.len().saturating_sub(1));
            let y = (PADDLE_BASE_Y + wave.get(si).copied().unwrap_or(0.0) * PADDLE_AMP)
                .max(PADDLE_FLOOR);
            verts.push(Vector::new(x, y));
            self.paddle_world[i] = (x, y);
        }
        if let Some(c) = self.colliders.get_mut(self.paddle) {
            c.set_shape(SharedShape::polyline(verts, None));
        }

        // 2) Step physics (no gravity — straight-line reflection).
        self.pipeline.step(
            Vector::new(0.0, 0.0),
            &self.params,
            &mut self.islands,
            &mut self.broad,
            &mut self.narrow,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.ccd,
            &(),
            &self.events,
        );

        // 3) Collisions: the waveform just reflects the ball; bricks break.
        let mut events = Vec::new();
        while let Ok(e) = self.col_recv.try_recv() {
            events.push(e);
        }
        for e in events {
            let CollisionEvent::Started(h1, h2, _) = e else { continue };
            let other = if h1 == self.ball_collider {
                h2
            } else if h2 == self.ball_collider {
                h1
            } else {
                continue;
            };
            if other == self.paddle {
                self.paddle_flash = 1.0;
            } else if let Some(i) = self.bricks.iter().position(|b| b.handle == other && b.alive) {
                self.bricks[i].alive = false;
                if let Some(c) = self.colliders.get_mut(other) {
                    c.set_sensor(true); // stays broken — the wall never regenerates
                }
                self.spawn_shards(i);
                self.score += 1;
            }
        }

        // 4) Beat = a brief speed surge + paddle flash.
        if let Some(strength) = feat.beat {
            if strength > 1.8 {
                self.boost = 1.0 + (strength * 0.10).min(0.35);
                self.paddle_flash = self.paddle_flash.max(0.8);
            }
        }

        // 5) Hold the ball at a constant (loudness-scaled) speed and keep a
        //    real vertical component so it always travels to the wall and back.
        let target = (BALL_SPEED + feat.rms * 2.4) * self.boost;
        if let Some(rb) = self.bodies.get_mut(self.ball) {
            let mut v = rb.linvel();
            let sp = (v.x * v.x + v.y * v.y).sqrt();
            if sp > 1e-3 {
                v *= target / sp;
            } else {
                v = Vector::new(target * 0.4, target);
            }
            let min_vy = target * 0.34;
            if v.y.abs() < min_vy {
                v.y = if v.y < 0.0 { -min_vy } else { min_vy };
                let vx2 = (target * target - v.y * v.y).max(0.0).sqrt();
                v.x = if v.x < 0.0 { -vx2 } else { vx2 };
            }
            rb.set_linvel(v, true);
        }

        // 6) Recover if the ball ever slips past the paddle or a wall.
        let pos = self.bodies[self.ball].translation();
        if pos.y < -0.5 || pos.y > AH + 1.0 || pos.x < -1.0 || pos.x > AW + 1.0 {
            self.serve();
        }

        // 7) Trail + shards + brick pop animation.
        let pos = self.bodies[self.ball].translation();
        self.trail.push_back((pos.x, pos.y));
        if self.trail.len() > TRAIL_LEN {
            self.trail.pop_front();
        }
        for s in &mut self.shards {
            s.x += s.vx * dt;
            s.y += s.vy * dt;
            s.vy -= 6.0 * dt;
            s.life -= dt;
        }
        self.shards.retain(|s| s.life > 0.0);
        for b in &mut self.bricks {
            if !b.alive && b.anim > 0.0 {
                b.anim = (b.anim - dt * 7.0).max(0.0);
            }
        }
    }

    fn draw(&self, ctx: &FrameCtx) {
        let v = View::fit();
        clear_background(BG);
        let feat = ctx.feat;

        // Backdrop: stars twinkle with treble, a dim moon swells with bass.
        for i in 0..60 {
            let x = hash01(i * 3 + 1) * AW;
            let y = 1.0 + hash01(i * 3 + 2) * (AH - 1.2);
            let tw = 0.5 + 0.5 * (ctx.time * (0.8 + hash01(i) * 2.5) + i as f32).sin();
            let a = (0.05 + 0.22 * feat.treble) * tw;
            v.rect(x, y, 0.035, 0.035, Color::new(0.9, 0.92, 1.0, a));
        }
        v.circle(AW * 0.5, AH * 0.6, 1.1 + 0.25 * feat.bass, Color::new(0.75, 0.70, 0.80, 0.020 + 0.03 * feat.bass));

        let rail = Color::new(0.16, 0.19, 0.25, 1.0);
        v.rect(-0.08, AH, 0.08, AH, rail);
        v.rect(AW, AH, 0.08, AH, rail);
        v.rect(-0.08, AH + 0.08, AW + 0.16, 0.08, rail);

        // Bricks: beveled, lit by their column's band.
        for b in &self.bricks {
            if b.anim < 0.02 {
                continue;
            }
            let e = feat.bands[b.band];
            let k = (0.60 + 0.55 * e).min(1.15);
            let c = Color::new((b.color.r * k).min(1.0), (b.color.g * k).min(1.0), (b.color.b * k).min(1.0), 1.0);
            let (hw, hh) = (b.hw * b.anim, b.hh * b.anim);
            let (x, y_top, w, h) = (b.x - hw, b.y + hh, hw * 2.0, hh * 2.0);
            v.rect(x, y_top, w, h, c);
            v.rect(x, y_top, w, h * 0.20, Color::new((c.r + 0.22).min(1.0), (c.g + 0.22).min(1.0), (c.b + 0.22).min(1.0), 1.0));
        }

        for s in &self.shards {
            let a = (s.life / 0.6).clamp(0.0, 1.0);
            v.rect(s.x, s.y, s.w * a, s.w * a, Color::new(s.color.r, s.color.g, s.color.b, a));
        }

        for (i, &(x, y)) in self.trail.iter().enumerate() {
            let a = i as f32 / self.trail.len().max(1) as f32 * 0.22;
            v.circle(x, y, BALL_R * (0.4 + 0.4 * a), Color::new(0.9, 0.92, 1.0, a));
        }

        // The waveform paddle: filled body + line, warmed by the mids.
        let flash = self.paddle_flash;
        let warm = feat.mid;
        let wave_c = Color::new(
            (WAVE.r + 0.22 * warm + (1.0 - WAVE.r) * flash * 0.7).min(1.0),
            (WAVE.g + 0.10 * warm + (1.0 - WAVE.g) * flash * 0.7).min(1.0),
            WAVE.b,
            0.95,
        );
        let fill_c = Color::new(WAVE.r, WAVE.g, WAVE.b, 0.10 + flash * 0.06);
        for i in 1..self.paddle_world.len() {
            let (x0, y0) = self.paddle_world[i - 1];
            let (x1, y1) = self.paddle_world[i];
            let a = v.xy(x0, y0);
            let b = v.xy(x1, y1);
            let c = v.xy(x1, 0.0);
            let d = v.xy(x0, 0.0);
            draw_triangle(a.into(), b.into(), c.into(), fill_c);
            draw_triangle(a.into(), c.into(), d.into(), fill_c);
            v.line(x0, y0, x1, y1, 3.0 + flash * 2.0, wave_c);
        }

        let pos = self.bodies[self.ball].translation();
        v.circle(pos.x, pos.y, BALL_R, Color::new(0.82, 0.84, 0.90, 1.0));
        v.circle(pos.x - BALL_R * 0.25, pos.y + BALL_R * 0.25, BALL_R * 0.55, WHITE);

        // Progress: bricks cleared of the total.
        let text = format!("{} / {}", self.score, self.total_bricks);
        let dim = measure_text(&text, None, 26, 1.0);
        let (sx, sy) = v.xy(AW - 0.3, AH - 0.3);
        draw_text(&text, sx - dim.width, sy, 26.0, Color::new(1.0, 1.0, 1.0, 0.5));
    }
}
