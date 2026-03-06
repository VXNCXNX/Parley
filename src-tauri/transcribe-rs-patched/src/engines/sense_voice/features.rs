use ndarray::{Array1, Array2};
use rustfft::{num_complex::Complex, FftPlanner};
use std::f32::consts::PI;

/// FBANK feature extraction parameters matching Kaldi/SenseVoice configuration.
pub struct FbankConfig {
    pub sample_rate: u32,
    pub num_mel_bins: usize,
    pub frame_length_ms: f32,
    pub frame_shift_ms: f32,
    pub preemphasis_coeff: f32,
    pub low_freq: f32,
    pub high_freq: f32,
    pub snip_edges: bool,
}

impl Default for FbankConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16000,
            num_mel_bins: 80,
            frame_length_ms: 25.0,
            frame_shift_ms: 10.0,
            preemphasis_coeff: 0.97,
            low_freq: 20.0,
            high_freq: 0.0, // 0 = Nyquist
            snip_edges: true,
        }
    }
}

/// Compute FBANK features from audio samples.
///
/// Samples are expected in [-1.0, 1.0] range. If `normalize_samples` is false (the
/// SenseVoice default), samples will be scaled to [-32768, 32767] before processing.
pub fn compute_fbank(
    samples: &[f32],
    config: &FbankConfig,
    normalize_samples: bool,
) -> Array2<f32> {
    let sr = config.sample_rate as f32;
    let frame_length = (config.frame_length_ms / 1000.0 * sr) as usize;
    let frame_shift = (config.frame_shift_ms / 1000.0 * sr) as usize;

    // Scale samples if model expects unnormalized ([-32768, 32767]) input
    let samples: Vec<f32> = if !normalize_samples {
        samples.iter().map(|&s| s * 32768.0).collect()
    } else {
        samples.to_vec()
    };

    // Number of frames (snip_edges = true: only full frames)
    let num_frames = if config.snip_edges {
        if samples.len() < frame_length {
            0
        } else {
            1 + (samples.len() - frame_length) / frame_shift
        }
    } else {
        (samples.len() + frame_shift - 1) / frame_shift
    };

    if num_frames == 0 {
        return Array2::zeros((0, config.num_mel_bins));
    }

    // FFT size: next power of 2 >= frame_length
    let fft_size = frame_length.next_power_of_two();
    let num_fft_bins = fft_size / 2 + 1;

    // Pre-compute Hamming window
    let window = hamming_window(frame_length);

    // Pre-compute mel filterbank
    let high_freq = if config.high_freq == 0.0 {
        sr / 2.0
    } else if config.high_freq < 0.0 {
        sr / 2.0 + config.high_freq
    } else {
        config.high_freq
    };
    let mel_banks = mel_filterbank(
        config.num_mel_bins,
        fft_size,
        sr,
        config.low_freq,
        high_freq,
    );

    // Set up FFT
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(fft_size);

    let mut features = Array2::zeros((num_frames, config.num_mel_bins));

    for i in 0..num_frames {
        let start = i * frame_shift;

        // Extract frame with zero-padding if needed
        let mut frame = vec![0.0f32; frame_length];
        let copy_len = frame_length.min(samples.len().saturating_sub(start));
        frame[..copy_len].copy_from_slice(&samples[start..start + copy_len]);

        // Pre-emphasis: y[n] = x[n] - coeff * x[n-1]
        for j in (1..frame_length).rev() {
            frame[j] -= config.preemphasis_coeff * frame[j - 1];
        }
        frame[0] *= 1.0 - config.preemphasis_coeff;

        // Apply Hamming window
        for j in 0..frame_length {
            frame[j] *= window[j];
        }

        // FFT (zero-pad to fft_size)
        let mut fft_input: Vec<Complex<f32>> =
            frame.iter().map(|&x| Complex::new(x, 0.0)).collect();
        fft_input.resize(fft_size, Complex::new(0.0, 0.0));
        fft.process(&mut fft_input);

        // Power spectrum: |X[k]|^2
        let power_spectrum: Vec<f32> = fft_input[..num_fft_bins]
            .iter()
            .map(|c| c.norm_sqr())
            .collect();

        // Apply mel filterbank and take log
        for m in 0..config.num_mel_bins {
            let mut energy: f32 = mel_banks
                .row(m)
                .iter()
                .zip(power_spectrum.iter())
                .map(|(&w, &p)| w * p)
                .sum();

            // Floor to avoid log(0)
            if energy < 1.0e-10 {
                energy = 1.0e-10;
            }
            features[[i, m]] = energy.ln();
        }
    }

    features
}

/// Compute a Hamming window of the given length.
fn hamming_window(length: usize) -> Vec<f32> {
    (0..length)
        .map(|i| 0.54 - 0.46 * (2.0 * PI * i as f32 / (length as f32 - 1.0)).cos())
        .collect()
}

/// Compute mel filterbank matrix of shape [num_mel_bins, num_fft_bins].
fn mel_filterbank(
    num_mel_bins: usize,
    fft_size: usize,
    sample_rate: f32,
    low_freq: f32,
    high_freq: f32,
) -> Array2<f32> {
    let num_fft_bins = fft_size / 2 + 1;

    let mel_low = hz_to_mel(low_freq);
    let mel_high = hz_to_mel(high_freq);

    // num_mel_bins + 2 points uniformly spaced in mel domain
    let num_points = num_mel_bins + 2;
    let mel_points: Vec<f32> = (0..num_points)
        .map(|i| mel_low + (mel_high - mel_low) * i as f32 / (num_points - 1) as f32)
        .collect();

    let hz_points: Vec<f32> = mel_points.iter().map(|&m| mel_to_hz(m)).collect();

    // Convert Hz to FFT bin indices
    let bin_points: Vec<f32> = hz_points
        .iter()
        .map(|&f| f * fft_size as f32 / sample_rate)
        .collect();

    let mut banks = Array2::zeros((num_mel_bins, num_fft_bins));

    for m in 0..num_mel_bins {
        let left = bin_points[m];
        let center = bin_points[m + 1];
        let right = bin_points[m + 2];

        for k in 0..num_fft_bins {
            let kf = k as f32;
            if kf > left && kf < center {
                banks[[m, k]] = (kf - left) / (center - left);
            } else if kf >= center && kf < right {
                banks[[m, k]] = (right - kf) / (right - center);
            }
        }
    }

    banks
}

/// Convert frequency in Hz to mel scale (HTK formula).
fn hz_to_mel(hz: f32) -> f32 {
    1127.0 * (1.0 + hz / 700.0).ln()
}

/// Convert mel scale back to Hz.
fn mel_to_hz(mel: f32) -> f32 {
    700.0 * ((mel / 1127.0).exp() - 1.0)
}

/// Apply Lower Frame Rate (LFR) stacking.
///
/// Concatenates `window_size` consecutive frames with a stride of `window_shift`,
/// reducing temporal resolution while increasing feature dimension.
///
/// Input shape: [num_frames, feat_dim]
/// Output shape: [(num_frames - window_size) / window_shift + 1, feat_dim * window_size]
pub fn apply_lfr(features: &Array2<f32>, window_size: usize, window_shift: usize) -> Array2<f32> {
    let in_frames = features.nrows();
    let in_dim = features.ncols();

    if in_frames < window_size {
        return Array2::zeros((0, in_dim * window_size));
    }

    let out_frames = (in_frames - window_size) / window_shift + 1;
    let out_dim = in_dim * window_size;

    let mut out = Array2::zeros((out_frames, out_dim));

    for i in 0..out_frames {
        let src_start = i * window_shift;
        for w in 0..window_size {
            let src_row = features.row(src_start + w);
            let dst_start = w * in_dim;
            for (j, &val) in src_row.iter().enumerate() {
                out[[i, dst_start + j]] = val;
            }
        }
    }

    out
}

/// Apply Cepstral Mean-Variance Normalization (CMVN).
///
/// Formula: x[i] = (x[i] + neg_mean[i]) * inv_stddev[i]
///
/// Modifies features in-place.
pub fn apply_cmvn(features: &mut Array2<f32>, neg_mean: &Array1<f32>, inv_stddev: &Array1<f32>) {
    let dim = features.ncols();
    debug_assert_eq!(neg_mean.len(), dim);
    debug_assert_eq!(inv_stddev.len(), dim);

    for mut row in features.rows_mut() {
        for j in 0..dim {
            row[j] = (row[j] + neg_mean[j]) * inv_stddev[j];
        }
    }
}
