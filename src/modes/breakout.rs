//! Waveform Breakout — real breakout, played by the audio.
//!
//! Classic breakout has NO gravity: the ball travels straight lines at constant
//! speed and reflects off whatever it hits. There is no player and no paddle
//! sprite — the live waveform IS the paddle, a full-width deforming surface on
//! the floor whose slope steers each bounce back up into a tall brick wall.
//! The wall is large and does NOT regenerate, so a song demolishes it.
//!
//! The arena is 16:9 (fills the export frame). A small ball and small bricks
//! packed at the top leave a big open court below. The paddle waveform is
//! smoothed in space and over time, and pulses with loudness, so it flows and
//! breathes instead of jittering.

use macroquad::prelude::*;
use rapier2d::prelude::*;
use std::sync::mpsc::{channel, Receiver};

use crate::analysis::N_BANDS;
use crate::modes::{Category, FrameCtx, Mode, Param};
use crate::style::{amber, hash01, mix, slate, spec, teal, teal_deep, with_alpha};
use crate::track::Track;
use crate::view::{View, AH, AW};

// 16:9 world (AW x AH) — fills a 16:9 frame exactly, which is what the video
// exporter wants. Small ball + small bricks confined to the top leave a large
// open court below.
const BALL_R: f32 = 0.16;
const PADDLE_BASE_Y: f32 = 0.5;
const PADDLE_FLOOR: f32 = 0.1;
const WAVE_PTS: usize = 110;
const BRICK_TOP: f32 = 8.85;

// Defaults for the live-tunable settings exposed via params().
const DEF_BALL_SPEED: f32 = 3.5;
const DEF_PADDLE_AMP: f32 = 1.5;
const DEF_BRICK_FILL: f32 = 0.55; // brick size as a fraction of its grid cell
const DEF_COURT: f32 = 7.7; // brick_bottom; higher = bricks higher = bigger court
const DEF_COLS: usize = 34;
const DEF_ROWS: usize = 6;

struct Brick {
    handle: ColliderHandle,
    x: f32,
    y: f32,
    hw: f32,
    hh: f32,
    band: usize,
    /// Mix factor teal_deep→teal (0..1). The COLOR is resolved live from the
    /// theme each frame so a theme switch re-skins the wall immediately, instead
    /// of caching a Color that freezes until the track reloads.
    shade: f32,
    alive: bool,
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

#[derive(Clone, Copy)]
struct Ball {
    body: RigidBodyHandle,
    collider: ColliderHandle,
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

    static_body: RigidBodyHandle,
    balls: Vec<Ball>,
    n_balls: usize,
    paddle: ColliderHandle,
    bricks: Vec<Brick>,
    total_bricks: u32,

    // live-tunable settings (see params()/set_param())
    ball_speed: f32,
    ball_size: f32,
    paddle_amp: f32,
    brick_fill: f32,
    court: f32,
    cols: usize,
    rows: usize,

    col_recv: Receiver<CollisionEvent>,
    _force_recv: Receiver<ContactForceEvent>,
    events: ChannelEventCollector,

    /// A longer audio window (~140ms) the paddle reads, so its smoothed curve
    /// shows real bass-shaped amplitude instead of a flat 46ms snapshot.
    paddle_buf: Vec<f32>,
    /// Per-point paddle height, temporally smoothed (what is drawn + collided).
    paddle_y: Vec<f32>,
    paddle_world: Vec<(f32, f32)>,
    shards: Vec<Shard>,
    paddle_flash: f32,
    boost: f32,
    score: u32,
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

        let verts: Vec<Vector> = (0..WAVE_PTS)
            .map(|i| Vector::new(i as f32 / (WAVE_PTS - 1) as f32 * AW, PADDLE_BASE_Y))
            .collect();
        let paddle_col = ColliderBuilder::polyline(verts, None)
            .restitution(1.0)
            .friction(0.0)
            .active_events(ActiveEvents::COLLISION_EVENTS)
            .build();
        let paddle = colliders.insert_with_parent(paddle_col, static_body, &mut bodies);

        // The ball — no gravity; speed held constant each frame.
        let ball_rb = RigidBodyBuilder::dynamic()
            .translation(Vector::new(AW / 2.0, AH * 0.35))
            .linvel(Vector::new(2.4, DEF_BALL_SPEED))
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

        let mut me = Breakout {
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
            static_body,
            balls: vec![Ball { body: ball, collider: ball_collider }],
            n_balls: 1,
            paddle,
            bricks: Vec::new(),
            total_bricks: 0,
            ball_speed: DEF_BALL_SPEED,
            ball_size: 1.0,
            paddle_amp: DEF_PADDLE_AMP,
            brick_fill: DEF_BRICK_FILL,
            court: DEF_COURT,
            cols: DEF_COLS,
            rows: DEF_ROWS,
            col_recv,
            _force_recv,
            events,
            paddle_buf: vec![0.0; 6144],
            paddle_y: vec![PADDLE_BASE_Y; WAVE_PTS],
            paddle_world: vec![(0.0, PADDLE_BASE_Y); WAVE_PTS],
            shards: Vec::new(),
            paddle_flash: 0.0,
            boost: 1.0,
            score: 0,
        };
        me.build_wall();
        me
    }

    /// (Re)build the brick wall from the current cols/rows/brick_fill/court.
    /// Removes any existing brick colliders first, so it is safe to call live.
    fn build_wall(&mut self) {
        let old: Vec<ColliderHandle> = self.bricks.iter().map(|b| b.handle).collect();
        self.bricks.clear();
        for h in old {
            self.colliders.remove(h, &mut self.islands, &mut self.bodies, false);
        }

        let margin = 0.4f32;
        let area_w = AW - margin * 2.0;
        let slot = area_w / self.cols as f32;
        let rgap = (BRICK_TOP - self.court) / self.rows as f32;
        let bh = rgap * self.brick_fill / 2.0;
        let denom = (self.rows.max(2) - 1) as f32;
        for r in 0..self.rows {
            // Bond the courses like real brickwork — shift alternate rows.
            let bond = if r % 2 == 1 { slot * 0.5 } else { 0.0 };
            // Front (low) rows read brighter/closer; back rows sink to deep teal.
            // Color is row DEPTH (a cool family), never column index — no rainbow.
            let tone = 0.30 + 0.45 * (1.0 - r as f32 / denom);
            for c in 0..self.cols {
                let seed = (r as i32) * 131 + (c as i32) * 17;
                let jw = 1.0 + (hash01(seed) - 0.5) * 0.12; // width ±6%
                let bw = slot * self.brick_fill / 2.0 * jw;
                let x = margin + (c as f32 + 0.5) * slot + bond;
                let y = self.court + (r as f32 + 0.5) * rgap;
                let col = ColliderBuilder::cuboid(bw, bh)
                    .translation(Vector::new(x, y))
                    .restitution(1.0)
                    .friction(0.0)
                    .active_events(ActiveEvents::COLLISION_EVENTS)
                    .build();
                let handle = self.colliders.insert_with_parent(col, self.static_body, &mut self.bodies);
                let lift = 1.0 + (hash01(seed + 7) - 0.5) * 0.16; // brightness ±8%
                self.bricks.push(Brick {
                    handle,
                    x,
                    y,
                    hw: bw,
                    hh: bh,
                    band: (c * N_BANDS / self.cols).min(N_BANDS - 1),
                    shade: (tone * lift).clamp(0.0, 1.0),
                    alive: true,
                    anim: 1.0,
                });
            }
        }
        self.total_bricks = self.bricks.len() as u32;
    }

    /// Spawn one ball body + collider (no gravity; speed held each frame).
    fn add_ball(&mut self) {
        let rb = RigidBodyBuilder::dynamic().ccd_enabled(true).build();
        let body = self.bodies.insert(rb);
        let col = ColliderBuilder::ball(BALL_R * self.ball_size)
            .restitution(1.0)
            .restitution_combine_rule(CoefficientCombineRule::Max)
            .friction(0.0)
            .active_events(ActiveEvents::COLLISION_EVENTS)
            .build();
        let collider = self.colliders.insert_with_parent(col, body, &mut self.bodies);
        self.balls.push(Ball { body, collider });
    }

    /// Grow/shrink the live ball set to `n` (1..6), then re-serve.
    fn set_ball_count(&mut self, n: usize) {
        let n = n.clamp(1, 6);
        while self.balls.len() < n {
            self.add_ball();
        }
        while self.balls.len() > n {
            if let Some(b) = self.balls.pop() {
                self.bodies.remove(
                    b.body,
                    &mut self.islands,
                    &mut self.colliders,
                    &mut self.impulse_joints,
                    &mut self.multibody_joints,
                    true,
                );
            }
        }
        self.n_balls = n;
        self.serve();
    }

    /// Serve ball `i` from an off-center start with a random angle, spread across
    /// the court so multiple balls don't stack.
    fn serve_ball(&mut self, i: usize) {
        let n = self.balls.len().max(1);
        let body = self.balls[i].body;
        let dir = if macroquad::rand::gen_range(0.0, 1.0) < 0.5 { -1.0 } else { 1.0 };
        let vx = macroquad::rand::gen_range(1.6, 2.8) * dir;
        let speed = self.ball_speed;
        let x = AW * (0.28 + 0.44 * (i as f32 + 0.5) / n as f32);
        if let Some(rb) = self.bodies.get_mut(body) {
            rb.set_translation(Vector::new(x, AH * 0.35), true);
            rb.set_linvel(Vector::new(vx, speed), true);
        }
    }

    fn serve(&mut self) {
        for i in 0..self.balls.len() {
            self.serve_ball(i);
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
                color: mix(teal_deep(), teal(), b.shade),
            });
        }
    }

    /// Rebuild the paddle from the waveform. A LIGHT spatial average keeps the
    /// real oscilloscope shape (heavy averaging flattens it to nothing), and a
    /// per-point temporal ease removes the frame-to-frame jitter so it flows.
    /// Amplitude pulses with loudness.
    /// Returns the largest point movement this frame, so the caller can skip
    /// rebuilding the collider when the surface is effectively still.
    fn reshape_paddle(&mut self, _rms: f32, dt: f32) -> f32 {
        let n = self.paddle_buf.len();
        // 1) ZERO-CROSSING TRIGGER — phase-lock the waveform so it HOLDS STILL.
        //    Without this the window scrolls every frame and scaling the height
        //    amplifies that scroll into jitter (the exact bug). Find a rising edge
        //    through zero in the head of the buffer; sample the span after it.
        // A SHORT span (~35ms) so the waveform reads as a wave in 110 points
        // instead of being averaged into a flat line.
        let span = (n / 4).max(1);
        let mut start = 0usize;
        for i in 1..(n - span).max(1) {
            if self.paddle_buf[i - 1] < 0.0 && self.paddle_buf[i] >= 0.0 {
                start = i;
                break;
            }
        }
        // 2) Sample the triggered span to control points with only LIGHT averaging
        //    (anti-alias) — not a heavy low-pass that flattens the amplitude.
        let mut pts = [0.0f32; WAVE_PTS];
        let avg = 3usize;
        for i in 0..WAVE_PTS {
            let c = start + (i as f32 / (WAVE_PTS - 1) as f32 * (span - 1) as f32) as usize;
            let lo = c.saturating_sub(avg);
            let hi = (c + avg).min(n - 1);
            let mut s = 0.0;
            for k in lo..=hi {
                s += self.paddle_buf[k];
            }
            pts[i] = s / (hi - lo + 1) as f32;
        }
        // A couple of [1,2,1] passes (edge-replicated endpoints).
        for _ in 0..2 {
            let src = pts;
            for i in 0..WAVE_PTS {
                let l = src[i.saturating_sub(1)];
                let r = src[(i + 1).min(WAVE_PTS - 1)];
                pts[i] = l * 0.25 + src[i] * 0.5 + r * 0.25;
            }
        }
        // 3) Scale by a FIXED gain × wave-height. NO per-frame RMS normalisation
        //    (that fluctuated every frame and amplified jitter); the PCM
        //    amplitude IS the loudness (per-track calibrated in window_at),
        //    so quiet stays small and loud fills.
        const GAIN: f32 = 3.6;
        let height = self.paddle_amp * GAIN;
        // Centre the wave high enough that it has symmetric room ABOVE and BELOW.
        let base = PADDLE_BASE_Y + 1.8;
        let half = base - PADDLE_FLOOR - 0.1; // max symmetric excursion
        let st = (dt * 7.0).min(1.0);
        let mut moved = 0.0f32;
        const TAPER: f32 = 0.08;
        for i in 0..WAVE_PTS {
            let f = i as f32 / (WAVE_PTS - 1) as f32;
            // Ease the amplitude to zero at both ends so the wave mounts cleanly
            // to the court walls instead of cutting off (no edge spike).
            let edge = (f / TAPER).min((1.0 - f) / TAPER).clamp(0.0, 1.0);
            let edge = edge * edge * (3.0 - 2.0 * edge); // smoothstep
            // Clamp the displacement SYMMETRICALLY so a loud wave stays a full
            // waveform (crests AND troughs visible), never a one-sided mountain.
            let disp = (pts[i] * height * edge).clamp(-half, half);
            let target = base + disp;
            let delta = (target - self.paddle_y[i]) * st;
            self.paddle_y[i] += delta;
            moved = moved.max(delta.abs());
            self.paddle_world[i] = (f * AW, self.paddle_y[i]);
        }
        moved
    }
}

impl Mode for Breakout {
    fn name(&self) -> &'static str {
        "Breakout"
    }

    fn about(&self) -> &'static str {
        "Breakout with no player — the waveform is the paddle. The spectrum builds the wall."
    }

    fn category(&self) -> Category {
        Category::Game
    }

    fn trail(&self) -> f32 {
        0.16
    }

    fn params(&self) -> Vec<Param> {
        vec![
            Param::int("Balls", self.n_balls as i32, 1, 6),
            Param::float("Ball speed", self.ball_speed, 1.5, 9.0),
            Param::float("Ball size", self.ball_size, 0.5, 2.5),
            Param::float("Wave height", self.paddle_amp, 0.3, 3.5),
            Param::float("Block size", self.brick_fill, 0.3, 0.95),
            Param::float("Court height", self.court, 5.5, 8.4),
            Param::int("Columns", self.cols as i32, 12, 64),
            Param::int("Rows", self.rows as i32, 2, 14),
        ]
    }

    fn set_param(&mut self, name: &str, v: f32) {
        match name {
            "Balls" => self.set_ball_count(v.round() as usize),
            "Ball speed" => self.ball_speed = v,
            "Ball size" => {
                self.ball_size = v;
                let cs: Vec<_> = self.balls.iter().map(|b| b.collider).collect();
                for c in cs {
                    if let Some(col) = self.colliders.get_mut(c) {
                        col.set_shape(SharedShape::ball(BALL_R * v));
                    }
                }
            }
            "Wave height" => self.paddle_amp = v,
            "Block size" => {
                self.brick_fill = v;
                self.build_wall();
            }
            "Court height" => {
                self.court = v;
                self.build_wall();
            }
            "Columns" => {
                let n = (v.round() as usize).max(1);
                if n != self.cols {
                    self.cols = n;
                    self.build_wall();
                }
            }
            "Rows" => {
                let n = (v.round() as usize).max(1);
                if n != self.rows {
                    self.rows = n;
                    self.build_wall();
                }
            }
            _ => {}
        }
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
        self.shards.clear();
        for y in &mut self.paddle_y {
            *y = PADDLE_BASE_Y;
        }
        self.serve();
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let feat = ctx.feat;
        let dt = ctx.dt;
        // Step physics at the real frame delta so playback and exports at any
        // frame rate move the ball at the same real-world speed.
        self.params.dt = dt.clamp(1.0 / 240.0, 1.0 / 24.0);
        self.paddle_flash = (self.paddle_flash - dt * 4.0).max(0.0);
        self.boost += (1.0 - self.boost) * (dt * 3.0).min(1.0);

        // 1) Smoothed, pulsing waveform paddle. Rebuilding the polyline collider
        //    rebuilds its whole BVH (the bulk of update()), so only do it when
        //    the surface actually moved enough to change a bounce.
        ctx.track.window_at(ctx.time, &mut self.paddle_buf);
        let moved = self.reshape_paddle(feat.rms, dt);
        if moved > 0.005 {
            let verts: Vec<Vector> = self.paddle_world.iter().map(|&(x, y)| Vector::new(x, y)).collect();
            if let Some(c) = self.colliders.get_mut(self.paddle) {
                c.set_shape(SharedShape::polyline(verts, None));
            }
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

        // 3) Collisions: the waveform reflects the ball; bricks break.
        let mut events = Vec::new();
        while let Ok(e) = self.col_recv.try_recv() {
            events.push(e);
        }
        for e in events {
            let CollisionEvent::Started(h1, h2, _) = e else { continue };
            // Either collider may be one of our balls.
            let b1 = self.balls.iter().any(|b| b.collider == h1);
            let b2 = self.balls.iter().any(|b| b.collider == h2);
            let other = if b1 { h2 } else if b2 { h1 } else { continue };
            if other == self.paddle {
                self.paddle_flash = 1.0;
            } else if let Some(i) = self.bricks.iter().position(|b| b.handle == other && b.alive) {
                self.bricks[i].alive = false;
                if let Some(c) = self.colliders.get_mut(other) {
                    c.set_sensor(true);
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

        // 5) Hold EACH ball at a constant (loudness-scaled) speed; keep a real
        //    vertical component so it always travels to the wall and back.
        let target = (self.ball_speed + feat.rms * 1.0) * self.boost;
        let bodies: Vec<_> = self.balls.iter().map(|b| b.body).collect();
        for body in &bodies {
            if let Some(rb) = self.bodies.get_mut(*body) {
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
        }

        // 6) Recover any ball that slips past the paddle or a wall.
        for i in 0..self.balls.len() {
            let pos = self.bodies[self.balls[i].body].translation();
            if pos.y < -0.5 || pos.y > AH + 1.0 || pos.x < -1.0 || pos.x > AW + 1.0 {
                self.serve_ball(i);
            }
        }

        // 7) Shards + brick pop animation.
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
        let v = View::fit_world(AW, AH);
        let feat = ctx.feat;

        // Dust: two loose vertical drifts (not a band, not a grid), lit by treble.
        for i in 0..20 {
            let near = hash01(i * 7) < 0.7; // ~3:1 density between the drifts
            let (cx, cy) = if near { (AW * 0.28, AH * 0.55) } else { (AW * 0.70, AH * 0.35) };
            let x = cx + (hash01(i * 3 + 1) - 0.5) * 3.2;
            let y = cy + (hash01(i * 3 + 2) - 0.5) * 4.6; // taller spread than wide
            let tw = 0.5 + 0.5 * (ctx.time * (0.6 + hash01(i) * 2.0) + i as f32).sin();
            let a = (0.03 + 0.10 * feat.treble) * tw;
            v.circle(x, y, 0.025, with_alpha(mix(teal(), spec(), 0.4), a));
        }

        // Frame rails.
        let rail = mix(slate(), teal_deep(), 0.4);
        v.rect(-0.08, AH, 0.08, AH, rail);
        v.rect(AW, AH, 0.08, AH, rail);
        v.rect(-0.08, AH + 0.08, AW + 0.16, 0.08, rail);

        // Bricks: a lit cool wall. Loud bands brighten toward cream, never amber.
        for b in &self.bricks {
            if b.anim < 0.02 {
                continue;
            }
            let e = feat.bands[b.band];
            // Resolve the brick color from the LIVE theme so a switch re-skins it.
            let c = mix(mix(teal_deep(), teal(), b.shade), spec(), (e * 0.4).min(0.45));
            let (hw, hh) = (b.hw * b.anim, b.hh * b.anim);
            let (x, y_top, w, h) = (b.x - hw, b.y + hh, hw * 2.0, hh * 2.0);
            v.rect(x, y_top, w, h, c);
            v.rect(x, y_top, w, h * 0.22, mix(c, spec(), 0.18)); // slim top bevel
        }

        for s in &self.shards {
            let a = (s.life / 0.6).clamp(0.0, 1.0);
            v.rect(s.x, s.y, s.w * a, s.w * a, with_alpha(s.color, a));
        }

        // (The ball's trail is now the feedback buffer's job — see postfx.)

        // Waveform paddle: one crisp teal crest over a thin shadow and a short
        // under-band. Flash/beat tints it warm. No stacked glow.
        let flash = self.paddle_flash;
        let crest = mix(teal(), amber(), (flash * 0.7).min(0.7));
        let band = with_alpha(teal(), 0.13 + flash * 0.05);
        for i in 1..self.paddle_world.len() {
            let (x0, y0) = self.paddle_world[i - 1];
            let (x1, y1) = self.paddle_world[i];
            let a = v.xy(x0, y0);
            let b = v.xy(x1, y1);
            // Fill the whole body from the crest down to the floor — a bold,
            // solid waveform rather than a thin line.
            let cc = v.xy(x1, 0.0);
            let dd = v.xy(x0, 0.0);
            draw_triangle(a.into(), b.into(), cc.into(), band);
            draw_triangle(a.into(), cc.into(), dd.into(), band);
            v.line(x0, y0 - 0.04, x1, y1 - 0.04, 2.5, with_alpha(slate(), 0.85));
            v.line(x0, y0, x1, y1, 4.0 + flash * 1.5, crest);
        }

        // Balls: FLAT solid amber discs — no glow halo, no white highlight.
        let r = BALL_R * self.ball_size;
        for b in &self.balls {
            let pos = self.bodies[b.body].translation();
            v.circle(pos.x, pos.y, r, amber());
        }
    }
}
