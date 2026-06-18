//! Per-frame spectral analysis: one window of PCM in, one `Features` out.
//!
//! Beats are NOT detected here — they come from the offline beat grid computed
//! once per track in [`crate::track::Profile`], which is far more reliable than
//! a realtime guess and lets modes place things at *future* beats.

use realfft::{RealFftPlanner, RealToComplex};
use rustfft::num_complex::Complex;
use std::sync::Arc;

use crate::track::Track;

pub const N_BANDS: usize = 32;
/// FFT window length, shared by every render path (live, export, bench) so they
/// never diverge in spectral content.
pub const FFT_LEN: usize = 2048;

/// Build the per-frame [`Features`] at time `t`: copy the PCM window, run the
/// FFT, and fill in the beat from the offline grid (first beat in `(prev_t, t]`).
/// One recipe for every render path.
pub fn features_at(
    analyser: &mut Analyser,
    track: &Track,
    window: &mut [f32],
    t: f32,
    prev_t: f32,
    dt: f32,
) -> Features {
    track.window_at(t, window);
    let mut feat = analyser.analyze(window, track.sr, dt);
    feat.beat = track.profile.beat_in(prev_t, t);
    feat
}

/// What every mode reads, every frame.
#[derive(Clone, Default)]
pub struct Features {
    /// Log-spaced band energies, 0..1, attack/release smoothed for visuals.
    pub bands: [f32; N_BANDS],
    /// Overall loudness of the current window, 0..1.
    pub rms: f32,
    /// Band energies, 0..1 (instantaneous, not smoothed).
    pub bass: f32,
    pub mid: f32,
    pub treble: f32,
    /// Beat strength (~1..3) if a beat landed since the previous frame.
    pub beat: Option<f32>,
}

/// Owns the FFT plan and reusable buffers; call [`Analyser::analyze`] per frame.
pub struct Analyser {
    fft: Arc<dyn RealToComplex<f32>>,
    hann: Vec<f32>,
    input: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    fft_len: usize,
    smoothed: [f32; N_BANDS],
}

impl Analyser {
    pub fn new(fft_len: usize) -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(fft_len);
        let input = fft.make_input_vec();
        let spectrum = fft.make_output_vec();
        let scratch = fft.make_scratch_vec();
        let hann = (0..fft_len)
            .map(|i| {
                0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (fft_len - 1) as f32).cos())
            })
            .collect();
        Analyser { fft, hann, input, spectrum, scratch, fft_len, smoothed: [0.0; N_BANDS] }
    }

    /// Analyze one window. `dt` drives the visual release-smoothing of bands.
    pub fn analyze(&mut self, samples: &[f32], sample_rate: u32, dt: f32) -> Features {
        let n = self.fft_len.min(samples.len());

        let mut sum_sq = 0.0f32;
        for &s in &samples[..n] {
            sum_sq += s * s;
        }
        let rms = (sum_sq / n.max(1) as f32).sqrt().min(1.0);

        for i in 0..self.fft_len {
            let s = if i < n { samples[i] } else { 0.0 };
            self.input[i] = s * self.hann[i];
        }
        self.fft
            .process_with_scratch(&mut self.input, &mut self.spectrum, &mut self.scratch)
            .ok();

        let n_bins = self.spectrum.len();
        let sr = sample_rate as f32;
        let bin_hz = sr / self.fft_len as f32;
        let to_bin = |hz: f32| ((hz / bin_hz).round() as usize).clamp(1, n_bins - 1);
        let gain = 6.0f32;

        // 32 log-spaced bands (peak magnitude per band), attack-fast / release-slow.
        let f_min = 30.0f32;
        let f_max = (sr / 2.0).min(16_000.0);
        let (lmin, lmax) = (f_min.log2(), f_max.log2());
        let release = (-dt * 4.5).exp();
        let mut bands = [0.0f32; N_BANDS];
        for b in 0..N_BANDS {
            let lo = 2f32.powf(lmin + (b as f32 / N_BANDS as f32) * (lmax - lmin));
            let hi = 2f32.powf(lmin + ((b + 1) as f32 / N_BANDS as f32) * (lmax - lmin));
            let (lob, hib) = (to_bin(lo), to_bin(hi).max(to_bin(lo)));
            let mut peak = 0.0f32;
            for k in lob..=hib.min(n_bins - 1) {
                peak = peak.max(self.spectrum[k].norm());
            }
            let raw = (peak * 2.0 / self.fft_len as f32 * gain).min(1.0);
            self.smoothed[b] = raw.max(self.smoothed[b] * release);
            bands[b] = self.smoothed[b];
        }

        // Broad bands (instantaneous RMS-style energy).
        let energy = |lo: f32, hi: f32| -> f32 {
            let (lob, hib) = (to_bin(lo), to_bin(hi).max(to_bin(lo)));
            let mut s = 0.0f32;
            let mut c = 0.0f32;
            for k in lob..=hib.min(n_bins - 1) {
                let v = self.spectrum[k].norm() * 2.0 / self.fft_len as f32;
                s += v * v;
                c += 1.0;
            }
            ((s / c.max(1.0)).sqrt() * gain).min(1.0)
        };

        Features {
            bands,
            rms,
            bass: energy(30.0, 250.0),
            mid: energy(250.0, 2000.0),
            treble: energy(2000.0, f_max),
            beat: None, // filled in by the caller from the track's beat grid
        }
    }
}
