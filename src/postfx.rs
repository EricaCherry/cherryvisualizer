//! The "alive" post pipeline: feedback echo-trails.
//!
//! A persistent buffer accumulates the frame. At the start of each frame it is
//! decayed toward the backdrop (old content fades), then the mode's fresh
//! content is drawn on top — so motion leaves echoes. The buffer is composited
//! to the screen (live) or the export target, with the vignette finish applied
//! LAST so it never accumulates.
//!
//! Shader-free: the decay is a backdrop blit at low alpha and the trails are
//! ordinary alpha-blended draws, so it runs the same in play and export.

use macroquad::prelude::*;

use crate::modes::{FrameCtx, Mode};
use crate::style;
use crate::view;

pub struct PostFx {
    fb: RenderTarget,
    w: u32,
    h: u32,
    fresh: bool,
}

impl PostFx {
    pub fn new(w: u32, h: u32) -> Self {
        let (w, h) = (w.max(1), h.max(1));
        let fb = render_target(w, h);
        fb.texture.set_filter(FilterMode::Linear);
        PostFx { fb, w, h, fresh: true }
    }

    pub fn size(&self) -> (u32, u32) {
        (self.w, self.h)
    }

    /// Clear the trail buffer on the next frame (mode/theme switch, seek, loop).
    pub fn reset(&mut self) {
        self.fresh = true;
    }

    /// Draw `mode` (already `update`d) for this frame and composite it to `dest`
    /// (None = the screen). Applies feedback when `mode.trail() > 0`.
    pub fn render(&mut self, mode: &dyn Mode, ctx: &FrameCtx, dest: Option<&RenderTarget>) {
        let (wf, hf) = (self.w as f32, self.h as f32);
        let fade = mode.trail().clamp(0.0, 0.5);

        view::set_render_size(Some((wf, hf)));

        // ---- accumulate this frame into the feedback buffer ----------------
        view::set_export_target(Some(self.fb.clone()));
        view::apply_screen_camera();
        if self.fresh || fade <= 0.0 {
            // Full floor (first frame, or modes without trails) — the user's
            // background image if set, else the graded backdrop.
            if !style::draw_background(1.0) {
                style::backdrop();
            }
            self.fresh = false;
        } else if !style::draw_background(fade) {
            // Decay old content toward the floor (or settle it over the image).
            style::backdrop_blend(fade);
        }
        mode.draw(ctx); // content only — its backdrop/finish are the pipeline's job

        // ---- composite the buffer to the destination + vignette ------------
        match dest {
            Some(rt) => view::set_export_target(Some(rt.clone())),
            None => view::set_export_target(None),
        }
        view::apply_screen_camera();
        // Clear the destination first so the (partly-transparent) feedback blit
        // settles over a solid floor rather than accumulating across frames.
        clear_background(style::ink());
        // Render-target textures sample bottom-up, so flip back to upright here.
        draw_texture_ex(
            &self.fb.texture,
            0.0,
            0.0,
            WHITE,
            DrawTextureParams { dest_size: Some(vec2(wf, hf)), flip_y: true, ..Default::default() },
        );
        style::finish();

        set_default_camera();
        view::set_export_target(None);
        view::set_render_size(None);
    }
}
