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

// A wide vista: the range must read as a landscape under the sky, not one
// mountain filling the frame.
const TERRAIN_W: f32 = 44.0;
// The front row starts right under the camera (z = 9) so the frustum never
// looks past the mesh's leading edge into the void below the frame.
const NEAR: f32 = 9.5;
const DEPTH: f32 = 74.0;
const SKY_HORIZON: Color = Color::new(0.12, 0.11, 0.14, 1.0);

pub struct Terrain {
    hf: HeightField,
    height: f32,
    speed: f32,
}

impl Terrain {
    pub fn new() -> Self {
        Terrain { hf: HeightField::new(), height: 4.0, speed: 22.0 }
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
        // The camera floats a little above the tallest possible ridge, off the
        // centre line (the mirrored spectrum is symmetric; a 3/4 view keeps it
        // from reading synthetic), looking down the valley so peaks silhouette
        // against the sky instead of towering past the eye line into a trench.
        let cam_pos = vec3(3.5, 7.4 + feat.bass * 0.5, 9.0);
        set_camera(&Camera3D {
            position: cam_pos,
            target: vec3(0.0, 0.2, -40.0),
            up: vec3(0.0, 1.0, 0.0),
            fovy: (58.0 + feat.rms * 5.0).to_radians(),
            aspect: Some(view::screen_w() / view::screen_h()),
            render_target: view::export_target(),
            ..Default::default()
        });

        // The heightmap mesh from the shared field (the Rail Shooter flies over
        // this same landscape).
        let (verts, idx) = self.hf.build_mesh(TERRAIN_W, NEAR, DEPTH, self.height);

        material3d::bind(
            material3d::Surface::Rock,
            &material3d::LitParams {
                cam: cam_pos,
                // A warm low key light raking across the ridges from the left,
                // lifted enough that the slopes model instead of going murky.
                light_dir: vec3(-0.55, -0.75, -0.3),
                light_color: { let a = mix(amber(), spec(), 0.45); vec3(a.r, a.g, a.b) * 1.25 },
                ambient: { let s = teal_deep(); vec3(s.r, s.g, s.b) * 1.35 },
                horizon: SKY_HORIZON,
                metal: 0.3,
                rough: 1.0,
                tile: vec2(0.65, 0.65),
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
