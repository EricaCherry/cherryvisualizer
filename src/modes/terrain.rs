//! Terrain — an audio landscape. Each moment the 32-band spectrum becomes a new
//! ridgeline at the FRONT of a heightmap that scrolls away to the horizon, so the
//! music extrudes a flowing mountain range (bass piles up the central ridge,
//! treble ripples the flanks). It's a real lit mesh — per-vertex world normals
//! through the PBR material, graded by energy, fogged into a dusk sky.

use macroquad::prelude::*;

use crate::material3d;
use crate::modes::heightfield::HeightField;
use crate::modes::{Category, FrameCtx, Mode, Param};
use crate::style::{self, amber, hash01, mix, spec, teal_deep};
use crate::track::Track;
use crate::view;

const TERRAIN_W: f32 = 24.0;
const NEAR: f32 = 5.0;
const DEPTH: f32 = 68.0;
const SKY_HORIZON: Color = Color::new(0.10, 0.095, 0.12, 1.0);

pub struct Terrain {
    hf: HeightField,
    height: f32,
    speed: f32,
}

impl Terrain {
    pub fn new() -> Self {
        Terrain { hf: HeightField::new(), height: 5.2, speed: 22.0 }
    }
}

impl Mode for Terrain {
    fn name(&self) -> &'static str {
        "Terrain"
    }
    fn about(&self) -> &'static str {
        "An audio landscape — the spectrum extrudes flowing mountains that scroll to the horizon."
    }
    fn category(&self) -> Category {
        Category::Visualizer
    }
    fn own_background(&self) -> bool {
        true
    }

    fn params(&self) -> Vec<Param> {
        vec![
            Param::float("Height", self.height, 1.0, 9.0),
            Param::float("Scroll speed", self.speed, 8.0, 40.0),
        ]
    }
    fn set_param(&mut self, name: &str, v: f32) {
        match name {
            "Height" => self.height = v,
            "Scroll speed" => self.speed = v,
            _ => {}
        }
    }

    fn reset(&mut self, _t: &Track) {
        self.hf.reset();
    }

    fn update(&mut self, ctx: &FrameCtx) {
        self.hf.update(&ctx.feat.bands, ctx.dt, self.speed);
    }

    fn draw(&self, ctx: &FrameCtx) {
        let feat = ctx.feat;
        let t = ctx.time;
        let sky_top = style::ink();

        // ---- 2D backdrop (dusk sky, stars, a low sun glow) ------------------
        view::apply_screen_camera();
        clear_background(sky_top);
        let (sw, sh) = (view::screen_w(), view::screen_h());
        let horizon_y = sh * 0.42;
        let strips = 12;
        for i in 0..strips {
            let f = i as f32 / strips as f32;
            let c = Color::new(
                sky_top.r + (SKY_HORIZON.r - sky_top.r) * f,
                sky_top.g + (SKY_HORIZON.g - sky_top.g) * f,
                sky_top.b + (SKY_HORIZON.b - sky_top.b) * f,
                1.0,
            );
            draw_rectangle(0.0, horizon_y * f, sw, horizon_y / strips as f32 + 1.0, c);
        }
        let star_px = (sh / 760.0 * 2.0).max(1.5);
        for i in 0..44 {
            let x = hash01(i * 3 + 1) * sw;
            let y = hash01(i * 3 + 2) * horizon_y * 0.8;
            let tw = 0.5 + 0.5 * ((t * (1.0 + hash01(i) * 2.0) + i as f32).sin());
            let a = (0.06 + 0.25 * feat.treble) * tw;
            draw_rectangle(x, y, star_px, star_px, Color::new(spec().r, spec().g, spec().b, a));
        }
        draw_rectangle(0.0, horizon_y, sw, sh - horizon_y, SKY_HORIZON);

        // ---- 3D pass --------------------------------------------------------
        let cam_pos = vec3(0.0, 5.2 + feat.bass * 0.6, 9.0);
        set_camera(&Camera3D {
            position: cam_pos,
            target: vec3(0.0, 1.6, -30.0),
            up: vec3(0.0, 1.0, 0.0),
            fovy: (58.0 + feat.rms * 6.0).to_radians(),
            aspect: Some(view::screen_w() / view::screen_h()),
            render_target: view::export_target(),
            ..Default::default()
        });

        // The heightmap mesh from the shared field (the Rail Shooter flies over
        // this same landscape).
        let (verts, idx) = self.hf.build_mesh(TERRAIN_W, NEAR, DEPTH, self.height);

        material3d::bind(
            material3d::Surface::Ribbon,
            &material3d::LitParams {
                cam: cam_pos,
                light_dir: vec3(-0.3, -0.85, -0.35),
                light_color: { let a = mix(amber(), spec(), 0.4); vec3(a.r, a.g, a.b) },
                ambient: { let s = teal_deep(); vec3(s.r, s.g, s.b) * 1.0 },
                horizon: SKY_HORIZON,
                metal: 0.3,
                rough: 0.95,
                tile: vec2(1.0, 1.0),
                pulse: feat.bass * 0.5 + feat.beat.unwrap_or(0.0) * 0.4,
            },
        );
        draw_mesh(&Mesh { vertices: verts, indices: idx, texture: None });
        material3d::unbind();

        view::apply_screen_camera();
        style::finish();
        set_default_camera();
    }
}
