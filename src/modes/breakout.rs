//! Waveform Breakout — breakout played by the audio.
//!
//! No player and no paddle sprite: the live waveform IS the paddle. It forms a
//! full-width deforming surface along the bottom (a rapier polyline reshaped
//! every frame from the PCM window). When the ball lands on it, the waveform
//! bats it back up with power proportional to the music's loudness. The ball
//! breaks the bricks; the waveform never touches them. Strong beats kick the
//! ball; broken bricks grow back a few seconds later so the rally never ends.

use macroquad::prelude::*;
use rapier2d::prelude::*;
use std::collections::VecDeque;
use std::sync::mpsc::{channel, Receiver};

use crate::analysis::N_BANDS;
use crate::modes::{FrameCtx, Mode};
use crate::track::Track;
use crate::view::{hsl, View, AH, AW, BG, WAVE};

const BALL_R: f32 = 0.28;
const PADDLE_BASE_Y: f32 = 1.3;
const PADDLE_AMP: f32 = 1.5;
const WAVE_PTS: usize = 64;
const BRICK_RESPAWN_S: f32 = 3.2;
const TRAIL_LEN: usize = 14;

struct Brick {
    handle: ColliderHandle,
    x: f32,
    y: f32,
    hw: f32,
    hh: f32,
    band: usize,
    color: Color,
    alive: bool,
    /// Draw scale, eased toward 1 (alive) or 0 (broken) for pop animations.
    anim: f32,
    died_at: f32,
}

/// A flying fragment of a broken brick.
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
    gravity: Vector,

    ball: RigidBodyHandle,
    ball_collider: ColliderHandle,
    paddle: ColliderHandle,
    bricks: Vec<Brick>,

    col_recv: Receiver<CollisionEvent>,
    _force_recv: Receiver<ContactForceEvent>,
    events: ChannelEventCollector,

    paddle_world: Vec<(f32, f32)>,
    trail: VecDeque<(f32, f32)>,
    shards: Vec<Shard>,
    paddle_flash: f32,
    score: u32,
}

/// Tiny deterministic hash -> 0..1, for the star backdrop.
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

        // Bricks: a calm steel-blue -> amber wall, one spectrum band per column.
        let cols = 14usize;
        let rows = 4usize;
        let margin = 1.5f32;
        let area_w = AW - margin * 2.0;
        let slot = area_w / cols as f32;
        let bw = slot * 0.88 / 2.0;
        let (top, bottom) = (8.2f32, 5.2f32);
        let rgap = (top - bottom) / rows as f32;
        let bh = rgap * 0.6 / 2.0;
        let mut bricks = Vec::new();
        for r in 0..rows {
            for c in 0..cols {
                let x = margin + (c as f32 + 0.5) * slot;
                let y = bottom + (r as f32 + 0.5) * rgap;
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
                    band: (c * N_BANDS / cols).min(N_BANDS - 1),
                    color: hsl(0.58 - c as f32 / cols as f32 * 0.6, 0.5, 0.55),
                    alive: true,
                    anim: 1.0,
                    died_at: 0.0,
                });
            }
        }

        // The ball.
        let ball_rb = RigidBodyBuilder::dynamic()
            .translation(Vector::new(AW / 2.0, AH * 0.5))
            .linvel(Vector::new(2.5, -3.0))
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
            gravity: Vector::new(0.0, -4.0),
            ball,
            ball_collider,
            paddle,
            bricks,
            col_recv,
            _force_recv,
            events,
            paddle_world: vec![(0.0, PADDLE_BASE_Y); WAVE_PTS],
            trail: VecDeque::new(),
            shards: Vec::new(),
            paddle_flash: 0.0,
            score: 0,
        }
    }

    /// Burst a broken brick into flying fragments.
    fn spawn_shards(&mut self, idx: usize) {
        let b = &self.bricks[idx];
        for k in 0..7 {
            let a = (k as f32 / 7.0 - 0.5) * std::f32::consts::PI + std::f32::consts::FRAC_PI_2;
            self.shards.push(Shard {
                x: b.x,
                y: b.y,
                vx: a.cos() * (1.5 + hash01(k * 31) * 2.0),
                vy: a.sin() * (1.0 + hash01(k * 17) * 2.5),
                w: 0.10 + hash01(k * 7) * 0.12,
                life: 0.7,
                color: b.color,
            });
        }
    }

    fn launch_ball(&mut self) {
        let angle = (macroquad::rand::gen_range(0.2, 0.8)) * std::f32::consts::PI
            * if macroquad::rand::gen_range(0.0, 1.0) < 0.5 { 1.0 } else { -1.0 };
        if let Some(rb) = self.bodies.get_mut(self.ball) {
            rb.set_translation(Vector::new(AW / 2.0, AH * 0.5), true);
            rb.set_linvel(Vector::new(angle.cos() * 3.0, -angle.sin().abs() * 3.0), true);
        }
    }

    fn set_brick_alive(&mut self, idx: usize, alive: bool, now: f32) {
        let b = &mut self.bricks[idx];
        b.alive = alive;
        if !alive {
            b.died_at = now;
        }
        if let Some(c) = self.colliders.get_mut(b.handle) {
            c.set_sensor(!alive);
        }
    }
}

impl Mode for Breakout {
    fn name(&self) -> &'static str {
        "Breakout"
    }

    fn reset(&mut self, _track: &Track) {
        for i in 0..self.bricks.len() {
            self.set_brick_alive(i, true, 0.0);
            self.bricks[i].anim = 1.0;
        }
        self.score = 0;
        self.trail.clear();
        self.launch_ball();
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let feat = ctx.feat;
        let dt = ctx.dt;
        self.paddle_flash = (self.paddle_flash - dt * 4.0).max(0.0);

        // 1) Reshape the waveform paddle from the live window.
        let wave = ctx.wave;
        let mut verts: Vec<Vector> = Vec::with_capacity(WAVE_PTS);
        for i in 0..WAVE_PTS {
            let f = i as f32 / (WAVE_PTS - 1) as f32;
            let x = f * AW;
            let si = ((f * (wave.len().saturating_sub(1)) as f32) as usize)
                .min(wave.len().saturating_sub(1));
            let y = PADDLE_BASE_Y + wave.get(si).copied().unwrap_or(0.0) * PADDLE_AMP;
            verts.push(Vector::new(x, y));
            self.paddle_world[i] = (x, y);
        }
        if let Some(c) = self.colliders.get_mut(self.paddle) {
            c.set_shape(SharedShape::polyline(verts, None));
        }

        // 2) Step physics at a fixed timestep.
        self.pipeline.step(
            self.gravity,
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

        // 3) Collisions: the waveform launches the ball; the ball breaks bricks.
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
                // Launch power comes straight from the music's loudness.
                let launch = 3.5 + feat.rms * 9.0 + feat.bass * 7.0;
                if let Some(rb) = self.bodies.get_mut(self.ball) {
                    let v = rb.linvel();
                    rb.set_linvel(Vector::new(v.x, v.y.max(0.0)), true);
                    rb.apply_impulse(Vector::new(0.0, launch), true);
                }
                self.paddle_flash = 1.0;
            } else if let Some(i) = self
                .bricks
                .iter()
                .position(|b| b.handle == other && b.alive)
            {
                self.set_brick_alive(i, false, ctx.time);
                self.spawn_shards(i);
                self.score += 1;
            }
        }

        // 4) Strong beats kick the ball upward into the wall.
        if let Some(strength) = feat.beat {
            if strength > 1.6 {
                if let Some(rb) = self.bodies.get_mut(self.ball) {
                    rb.apply_impulse(Vector::new(0.0, 2.0 + strength), true);
                }
                self.paddle_flash = 1.0;
            }
        }

        // 5) Brick regrowth + pop animations.
        let respawn_due: Vec<usize> = self
            .bricks
            .iter()
            .enumerate()
            .filter(|(_, b)| !b.alive && ctx.time - b.died_at > BRICK_RESPAWN_S)
            .map(|(i, _)| i)
            .collect();
        for i in respawn_due {
            self.set_brick_alive(i, true, ctx.time);
        }
        for b in &mut self.bricks {
            let target = if b.alive { 1.0 } else { 0.0 };
            b.anim += (target - b.anim).clamp(-dt * 8.0, dt * 8.0);
        }

        // 6) Keep the ball lively but bounded; recover if it ever escapes.
        if let Some(rb) = self.bodies.get_mut(self.ball) {
            let v = rb.linvel();
            let sp = (v.x * v.x + v.y * v.y).sqrt();
            let max = 16.0;
            if sp > max {
                rb.set_linvel(Vector::new(v.x, v.y) * (max / sp), true);
            }
        }
        let pos = self.bodies[self.ball].translation();
        if pos.y < -1.5 || pos.y > AH + 3.0 || pos.x < -2.0 || pos.x > AW + 2.0 {
            self.launch_ball();
        }

        // 7) Trail + shard physics.
        let pos = self.bodies[self.ball].translation();
        self.trail.push_back((pos.x, pos.y));
        if self.trail.len() > TRAIL_LEN {
            self.trail.pop_front();
        }
        for s in &mut self.shards {
            s.x += s.vx * dt;
            s.y += s.vy * dt;
            s.vy -= 9.0 * dt;
            s.life -= dt;
        }
        self.shards.retain(|s| s.life > 0.0);
    }

    fn draw(&self, ctx: &FrameCtx) {
        let v = View::fit();
        clear_background(BG);
        let feat = ctx.feat;

        // --- backdrop layers -------------------------------------------------
        // Stars twinkle with the treble; a dim moon swells with the bass.
        for i in 0..60 {
            let x = hash01(i * 3 + 1) * AW;
            let y = 2.2 + hash01(i * 3 + 2) * (AH - 2.4);
            let tw = 0.5 + 0.5 * (ctx.time * (0.8 + hash01(i) * 2.5) + i as f32).sin();
            let a = (0.05 + 0.22 * feat.treble) * tw;
            v.rect(x, y, 0.035, 0.035, Color::new(0.9, 0.92, 1.0, a));
        }
        v.circle(
            AW * 0.5,
            AH * 0.55,
            1.1 + 0.25 * feat.bass,
            Color::new(0.75, 0.70, 0.80, 0.020 + 0.030 * feat.bass),
        );

        // Side + top rails so the arena reads as a space.
        let rail = Color::new(0.16, 0.19, 0.25, 1.0);
        v.rect(-0.08, AH, 0.08, AH, rail);
        v.rect(AW, AH, 0.08, AH, rail);
        v.rect(-0.08, AH + 0.08, AW + 0.16, 0.08, rail);

        // --- bricks: beveled, lit by their column's band ----------------------
        for b in &self.bricks {
            if b.anim < 0.02 {
                continue;
            }
            let e = feat.bands[b.band];
            let k = (0.60 + 0.55 * e).min(1.15);
            let c = Color::new(
                (b.color.r * k).min(1.0),
                (b.color.g * k).min(1.0),
                (b.color.b * k).min(1.0),
                1.0,
            );
            let (hw, hh) = (b.hw * b.anim, b.hh * b.anim * (1.0 + 0.12 * e));
            let (x, y_top, w, h) = (b.x - hw, b.y + hh, hw * 2.0, hh * 2.0);
            v.rect(x, y_top, w, h, c);
            // bevel: lit top edge, shaded bottom edge
            v.rect(x, y_top, w, h * 0.18, Color::new(
                (c.r + 0.22).min(1.0),
                (c.g + 0.22).min(1.0),
                (c.b + 0.22).min(1.0),
                1.0,
            ));
            v.rect(x, b.y - hh + h * 0.16, w, h * 0.16, Color::new(c.r * 0.55, c.g * 0.55, c.b * 0.55, 1.0));
        }

        // --- shards from broken bricks ---------------------------------------
        for s in &self.shards {
            let a = (s.life / 0.7).clamp(0.0, 1.0);
            v.rect(
                s.x,
                s.y,
                s.w * a,
                s.w * a,
                Color::new(s.color.r, s.color.g, s.color.b, a),
            );
        }

        // --- ball trail (subtle gray, fading) ---------------------------------
        for (i, &(x, y)) in self.trail.iter().enumerate() {
            let a = i as f32 / self.trail.len().max(1) as f32 * 0.22;
            v.circle(x, y, BALL_R * (0.4 + 0.4 * a), Color::new(0.9, 0.92, 1.0, a));
        }

        // --- the waveform paddle: filled body + line, warmed by the mids ------
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
            // Trapezoid fill under the sloped line (two triangles — a flat-
            // topped rect would stair-step against the smooth waveform).
            let a = v.xy(x0, y0.max(0.02));
            let b = v.xy(x1, y1.max(0.02));
            let c = v.xy(x1, 0.0);
            let d = v.xy(x0, 0.0);
            draw_triangle(a.into(), b.into(), c.into(), fill_c);
            draw_triangle(a.into(), c.into(), d.into(), fill_c);
            v.line(x0, y0, x1, y1, 3.0 + flash * 2.0, wave_c);
        }

        // --- ball with a simple highlight -------------------------------------
        let pos = self.bodies[self.ball].translation();
        v.circle(pos.x, pos.y, BALL_R, Color::new(0.82, 0.84, 0.90, 1.0));
        v.circle(pos.x - BALL_R * 0.25, pos.y + BALL_R * 0.25, BALL_R * 0.55, WHITE);

        // Score, top-right inside the arena.
        let text = format!("{}", self.score);
        let dim = measure_text(&text, None, 28, 1.0);
        let (sx, sy) = v.xy(AW - 0.35, AH - 0.35);
        draw_text(&text, sx - dim.width, sy, 28.0, Color::new(1.0, 1.0, 1.0, 0.5));
    }
}
