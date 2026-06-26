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
/// FFT window length (audioMotion-analyzer default) — 5.4 Hz/bin at 44.1 kHz, so
/// the low octave bands actually resolve. Shared by every render path.
pub const FFT_LEN: usize = 8192;
/// The shorter slice of the window handed to modes as the time-domain `wave` (a
/// crisp ~46 ms oscilloscope trace, independent of the longer FFT window).
pub const WAVE_LEN: usize = 2048;

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
    /// Per-BIN EMA of the linear magnitude — the AnalyserNode smoothingTimeConstant,
    /// the ONLY smoothing in the pipeline (audioMotion order: smooth bins, then band).
    smoothed: Vec<f32>,
    /// One EMA on the loudness so the rms-driven modes (scope/vinyl/tunnel) read a
    /// smooth value directly instead of each re-smoothing it themselves.
    rms_s: f32,
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
        let n_bins = spectrum.len();
        Analyser { fft, hann, input, spectrum, scratch, fft_len, smoothed: vec![0.0; n_bins], rms_s: 0.0 }
    }

    /// Analyze one window. `dt` drives the visual release-smoothing of bands.
    pub fn analyze(&mut self, samples: &[f32], sample_rate: u32, _dt: f32) -> Features {
        let n = self.fft_len.min(samples.len());

        // ---- FFT (Hann window) ---------------------------------------------
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

        // ---- the AnalyserNode pipeline, ported verbatim from audioMotion ----
        // STAGE 1: ONE EMA on the LINEAR magnitude, per BIN, before banding — the
        // smoothingTimeConstant, and the ONLY smoothing in the whole pipeline.
        const SMOOTHING: f32 = 0.5; // audioMotion default
        for k in 0..n_bins {
            let mag = self.spectrum[k].norm() * 2.0 / self.fft_len as f32;
            self.smoothed[k] = SMOOTHING * self.smoothed[k] + (1.0 - SMOOTHING) * mag;
        }

        // STAGE 2: log-spaced bands (20 Hz..20 kHz). Each band = the MAX smoothed
        // bin in its range -> dB -> normalized over a FIXED [-85,-25] dB window.
        // No auto-gain, no gamma, no gate — the dB floor IS the normalization.
        const MIN_DB: f32 = -85.0;
        const MAX_DB: f32 = -25.0;
        let f_min = 20.0f32;
        let f_max = (sr / 2.0).min(20_000.0);
        let (lmin, lmax) = (f_min.log2(), f_max.log2());
        let mut bands = [0.0f32; N_BANDS];
        for b in 0..N_BANDS {
            let lo = 2f32.powf(lmin + (b as f32 / N_BANDS as f32) * (lmax - lmin));
            let hi = 2f32.powf(lmin + ((b + 1) as f32 / N_BANDS as f32) * (lmax - lmin));
            let lob = ((lo / bin_hz).floor() as usize).clamp(1, n_bins - 1);
            let hib = ((hi / bin_hz).ceil() as usize).clamp(lob, n_bins - 1);
            let mut peak = 0.0f32;
            for k in lob..=hib {
                peak = peak.max(self.smoothed[k]);
            }
            let db = 20.0 * (peak + 1e-9).log10();
            bands[b] = ((db - MIN_DB) / (MAX_DB - MIN_DB)).clamp(0.0, 1.0);
        }

        // rms from a fixed dB window too (no AGC): -60 dBFS floor -> 0, 0 dB -> 1.
        let mut sum_sq = 0.0f32;
        for &s in &samples[..n] {
            sum_sq += s * s;
        }
        let rms_lin = (sum_sq / n.max(1) as f32).sqrt();
        let rms_raw = ((20.0 * (rms_lin + 1e-9).log10() + 60.0) / 60.0).clamp(0.0, 1.0);
        self.rms_s += (rms_raw - self.rms_s) * 0.4; // one EMA (the only loudness smoothing)
        let rms = self.rms_s;

        // Broad bands derived from the normalized spectrum (already adaptive,
        // balance preserved). Boundaries from the log map (~250 Hz, ~2 kHz).
        let band_at = |hz: f32| -> usize {
            (((hz.log2() - lmin) / (lmax - lmin)) * N_BANDS as f32).clamp(0.0, N_BANDS as f32) as usize
        };
        let b_lo = band_at(250.0).clamp(1, N_BANDS - 1);
        let b_hi = band_at(4000.0).clamp(b_lo + 1, N_BANDS); // MilkDrop bass/mid/treble splits
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
