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
    /// Adaptive references (running peaks) for auto-gain, so the visuals respond
    /// to ANY track's level, not just the synthetic demo they were tuned to.
    band_ref: f32,
    loud_ref: f32,
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
        Analyser { fft, hann, input, spectrum, scratch, fft_len, smoothed: [0.0; N_BANDS], band_ref: 0.01, loud_ref: 0.05 }
    }

    /// Analyze one window. `dt` drives the visual release-smoothing of bands.
    pub fn analyze(&mut self, samples: &[f32], sample_rate: u32, dt: f32) -> Features {
        let n = self.fft_len.min(samples.len());

        let mut sum_sq = 0.0f32;
        for &s in &samples[..n] {
            sum_sq += s * s;
        }
        let raw_rms = (sum_sq / n.max(1) as f32).sqrt();

        // Adaptive loudness: auto-level to the recent peak (a running peak with a
        // ~2s release + floor) so quiet and loud tracks both use the full range.
        let rel = (-dt / 2.5).exp();
        self.loud_ref = raw_rms.max(self.loud_ref * rel).max(0.008);
        // Headroom (×1.25) so typical content lands ~0.6–0.8 and only true peaks
        // reach 1.0. Without it `rms` pinned to a constant on any sustained
        // passage (loud_ref == raw_rms), so loud and quiet looked identical.
        let rms = (raw_rms / (self.loud_ref * 1.25)).min(1.0);

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

        // 32 log-spaced bands (raw peak magnitude per band).
        let f_min = 30.0f32;
        let f_max = (sr / 2.0).min(16_000.0);
        let (lmin, lmax) = (f_min.log2(), f_max.log2());
        let mut raw = [0.0f32; N_BANDS];
        let mut frame_max = 0.0f32;
        for b in 0..N_BANDS {
            let lo = 2f32.powf(lmin + (b as f32 / N_BANDS as f32) * (lmax - lmin));
            let hi = 2f32.powf(lmin + ((b + 1) as f32 / N_BANDS as f32) * (lmax - lmin));
            let (lob, hib) = (to_bin(lo), to_bin(hi).max(to_bin(lo)));
            let mut peak = 0.0f32;
            for k in lob..=hib.min(n_bins - 1) {
                peak = peak.max(self.spectrum[k].norm());
            }
            raw[b] = peak * 2.0 / self.fft_len as f32;
            frame_max = frame_max.max(raw[b]);
        }

        // Adaptive band gain: drive strong-but-not-peak bands to full so real
        // (broadband) music fills the range, with a near-linear curve that keeps
        // loud and quiet bands visibly different. The release is near-transparent
        // (~0.1s) so each mode owns its own fall instead of stacking on a slow one.
        self.band_ref = frame_max.max(self.band_ref * rel).max(0.0012);
        let band_gain = 1.2 / self.band_ref;
        let release = (-dt * 9.0).exp();
        let mut bands = [0.0f32; N_BANDS];
        for b in 0..N_BANDS {
            let target = (raw[b] * band_gain).min(1.0).powf(0.85);
            self.smoothed[b] = target.max(self.smoothed[b] * release);
            bands[b] = self.smoothed[b];
        }

        // Broad bands derived from the normalized spectrum (already adaptive,
        // balance preserved). Boundaries from the log map (~250 Hz, ~2 kHz).
        let band_at = |hz: f32| -> usize {
            (((hz.log2() - lmin) / (lmax - lmin)) * N_BANDS as f32).clamp(0.0, N_BANDS as f32) as usize
        };
        let b_lo = band_at(250.0).clamp(1, N_BANDS - 1);
        let b_hi = band_at(2000.0).clamp(b_lo + 1, N_BANDS);
        let (mut bass_mean, mut bass_peak) = (0.0f32, 0.0f32);
        for &v in &bands[0..b_lo] {
            bass_mean += v;
            bass_peak = bass_peak.max(v);
        }
        let bass = (bass_mean / b_lo as f32 * 0.4 + bass_peak * 0.6).min(1.0);
        let mid = bands[b_lo..b_hi].iter().sum::<f32>() / (b_hi - b_lo) as f32;
        let treble = bands[b_hi..N_BANDS].iter().sum::<f32>() / (N_BANDS - b_hi) as f32;

        Features {
            bands,
            rms,
            bass,
            mid,
            treble,
            beat: None, // filled in by the caller from the track's beat grid
        }
    }
}
