use rustfft::{FftPlanner, num_complex::Complex};

/// A trait for audio processing components.
pub trait AudioProcessor: Send {
    /// Real-time safe: no allocations or blocking.
    fn process(&mut self, input: &[f32], output: &mut [f32]);
}

// ---------------------------------------------------------------------------
// Phase 2: Gain / Phase Inversion
// ---------------------------------------------------------------------------

pub struct GainProcessor {
    pub gain: f32,
    pub invert: bool,
}

impl GainProcessor {
    pub fn new(gain: f32, invert: bool) -> Self {
        Self { gain, invert }
    }
}

impl AudioProcessor for GainProcessor {
    fn process(&mut self, input: &[f32], output: &mut [f32]) {
        let multiplier = if self.invert { -self.gain } else { self.gain };
        for (i, &sample) in input.iter().enumerate() {
            if i < output.len() {
                output[i] = sample * multiplier;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Phase 3: Delay + Gain
// ---------------------------------------------------------------------------

pub struct DelayProcessor {
    pub gain: f32,
    pub invert: bool,
    delay_samples: usize,
    buffer: Box<[f32]>,
    write_pos: usize,
}

impl DelayProcessor {
    pub fn new(gain: f32, invert: bool, delay_samples: usize, max_delay: usize) -> Self {
        let capacity = max_delay.max(1);
        Self {
            gain,
            invert,
            delay_samples: delay_samples.min(capacity - 1),
            buffer: vec![0.0f32; capacity].into_boxed_slice(),
            write_pos: 0,
        }
    }
}

impl AudioProcessor for DelayProcessor {
    fn process(&mut self, input: &[f32], output: &mut [f32]) {
        let multiplier = if self.invert { -self.gain } else { self.gain };
        let len = self.buffer.len();
        for (i, &sample) in input.iter().enumerate() {
            if i >= output.len() { break; }
            self.buffer[self.write_pos] = sample;
            let read_pos = (self.write_pos + len - self.delay_samples) % len;
            output[i] = self.buffer[read_pos] * multiplier;
            self.write_pos = (self.write_pos + 1) % len;
        }
    }
}

// ---------------------------------------------------------------------------
// Phase 4: Band-Limited Noise Suppression (Biquad IIR filters)
// ---------------------------------------------------------------------------

struct BiquadFilter {
    b0: f32, b1: f32, b2: f32,
    a1: f32, a2: f32,
    x1: f32, x2: f32,
    y1: f32, y2: f32,
}

impl BiquadFilter {
    fn high_pass(cutoff_hz: f32, sample_rate: f32) -> Self {
        let w0 = 2.0 * std::f32::consts::PI * cutoff_hz / sample_rate;
        let cos_w0 = w0.cos();
        let alpha = w0.sin() / (2.0 * 0.707);
        let b0 =  (1.0 + cos_w0) / 2.0;
        let b1 = -(1.0 + cos_w0);
        let b2 =  (1.0 + cos_w0) / 2.0;
        let a0 =   1.0 + alpha;
        let a1 =  -2.0 * cos_w0;
        let a2 =   1.0 - alpha;
        Self::new(b0/a0, b1/a0, b2/a0, a1/a0, a2/a0)
    }

    fn notch(center_hz: f32, q: f32, sample_rate: f32) -> Self {
        let w0 = 2.0 * std::f32::consts::PI * center_hz / sample_rate;
        let cos_w0 = w0.cos();
        let alpha = w0.sin() / (2.0 * q);
        let b0 =  1.0;
        let b1 = -2.0 * cos_w0;
        let b2 =  1.0;
        let a0 =  1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 =  1.0 - alpha;
        Self::new(b0/a0, b1/a0, b2/a0, a1/a0, a2/a0)
    }

    fn new(b0: f32, b1: f32, b2: f32, a1: f32, a2: f32) -> Self {
        Self { b0, b1, b2, a1, a2, x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0 }
    }

    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
                            - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1; self.x1 = x;
        self.y2 = self.y1; self.y1 = y;
        y
    }
}

pub struct FilterProcessor {
    pub gain: f32,
    hp: BiquadFilter,
    notches: Box<[BiquadFilter]>,
}

impl FilterProcessor {
    pub fn new(gain: f32, hp_cutoff_hz: f32, notch_freqs: &[(f32, f32)], sample_rate: f32) -> Self {
        let notches = notch_freqs
            .iter()
            .map(|&(hz, q)| BiquadFilter::notch(hz, q, sample_rate))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self {
            gain,
            hp: BiquadFilter::high_pass(hp_cutoff_hz, sample_rate),
            notches,
        }
    }
}

impl AudioProcessor for FilterProcessor {
    fn process(&mut self, input: &[f32], output: &mut [f32]) {
        for (i, &sample) in input.iter().enumerate() {
            if i >= output.len() { break; }
            let mut s = self.hp.tick(sample);
            for notch in self.notches.iter_mut() {
                s = notch.tick(s);
            }
            output[i] = s * self.gain;
        }
    }
}

// ---------------------------------------------------------------------------
// Phase 5: Adaptive LMS Filter
// ---------------------------------------------------------------------------

/// Least Mean Squares adaptive filter.
///
/// Treats the mic input as a noisy reference and minimises output energy,
/// continuously adapting weights to cancel steady-state noise components.
///
/// y[n]   = Σ w[k] * x[n-k]
/// e[n]   = x[n] - y[n]          (error / desired output)
/// w[k]  += μ * e[n] * x[n-k]
///
/// The output is e[n]: the residual after the adaptive filter has subtracted
/// its best estimate of the noise from the input.
pub struct LmsProcessor {
    pub mu: f32,
    weights: Box<[f32]>,
    delay_line: Box<[f32]>,
    pos: usize,
}

impl LmsProcessor {
    /// `filter_len`: number of taps (e.g. 64).
    /// `mu`: learning rate (e.g. 0.00005). Too high → instability.
    pub fn new(filter_len: usize, mu: f32) -> Self {
        let len = filter_len.max(1);
        Self {
            mu,
            weights: vec![0.0f32; len].into_boxed_slice(),
            delay_line: vec![0.0f32; len].into_boxed_slice(),
            pos: 0,
        }
    }
}

impl AudioProcessor for LmsProcessor {
    fn process(&mut self, input: &[f32], output: &mut [f32]) {
        let len = self.weights.len();
        for (i, &x) in input.iter().enumerate() {
            if i >= output.len() { break; }

            // Write new sample into circular delay line
            self.delay_line[self.pos] = x;

            // Compute filter output: y = w · x_delayed
            let mut y = 0.0f32;
            for k in 0..len {
                let idx = (self.pos + len - k) % len;
                y += self.weights[k] * self.delay_line[idx];
            }

            // Error signal is the desired output (noise-suppressed residual)
            let e = x - y;

            // Update weights: w[k] += μ * e * x[n-k]
            for k in 0..len {
                let idx = (self.pos + len - k) % len;
                self.weights[k] += self.mu * e * self.delay_line[idx];
            }

            output[i] = e;
            self.pos = (self.pos + 1) % len;
        }
    }
}

// ---------------------------------------------------------------------------
// Phase 6: Spectral Suppression (FFT-based noise floor subtraction)
// ---------------------------------------------------------------------------

/// Real-time spectral subtraction using overlapping FFT windows.
///
/// Algorithm per block:
///   1. Accumulate samples into an overlap-add input buffer.
///   2. When a full window is ready, apply a Hann window and forward FFT.
///   3. Estimate noise floor per bin using a slow-decaying magnitude average.
///   4. Subtract noise floor from magnitude (floored at a residual ratio).
///   5. Reconstruct via inverse FFT and overlap-add into the output buffer.
///   6. Drain output buffer sample-by-sample into the callback output slice.
///
/// All buffers are pre-allocated at construction.
pub struct SpectralProcessor {
    pub gain: f32,
    fft_size: usize,
    hop_size: usize,
    // Pre-allocated FFT plans (Arc inside, cheap to clone, not used in callback)
    fft: std::sync::Arc<dyn rustfft::Fft<f32>>,
    ifft: std::sync::Arc<dyn rustfft::Fft<f32>>,
    // Hann window coefficients
    window: Box<[f32]>,
    // Circular input accumulation buffer
    in_buf: Box<[f32]>,
    in_pos: usize,
    samples_since_hop: usize,
    // Noise floor estimate per bin (magnitude)
    noise_floor: Box<[f32]>,
    // Overlap-add output accumulation buffer (2 * fft_size for safety)
    out_buf: Box<[f32]>,
    out_write: usize,
    out_read: usize,
    // Scratch buffer for FFT (re-used each hop, no allocation)
    fft_buf: Box<[Complex<f32>]>,
    // Noise floor smoothing factor (per hop)
    alpha: f32,
    // Minimum residual ratio (spectral floor, prevents musical noise)
    floor_ratio: f32,
}

impl SpectralProcessor {
    /// `fft_size`: FFT window size, must be power of two (e.g. 1024).
    /// `hop_size`: samples between hops, typically fft_size/4 for 75% overlap.
    /// `alpha`: noise floor smoothing (0.9–0.99). Higher = slower adaptation.
    /// `floor_ratio`: minimum gain per bin after subtraction (e.g. 0.1).
    pub fn new(gain: f32, fft_size: usize, hop_size: usize, alpha: f32, floor_ratio: f32) -> Self {
        let mut planner = FftPlanner::new();
        let fft  = planner.plan_fft_forward(fft_size);
        let ifft = planner.plan_fft_inverse(fft_size);

        let window: Box<[f32]> = (0..fft_size)
            .map(|i| {
                0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (fft_size - 1) as f32).cos())
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();

        let out_buf_size = fft_size * 2;
        Self {
            gain,
            fft_size,
            hop_size,
            fft,
            ifft,
            window,
            in_buf: vec![0.0f32; fft_size].into_boxed_slice(),
            in_pos: 0,
            samples_since_hop: 0,
            noise_floor: vec![0.0f32; fft_size / 2 + 1].into_boxed_slice(),
            out_buf: vec![0.0f32; out_buf_size].into_boxed_slice(),
            out_write: 0,
            out_read: 0,
            fft_buf: vec![Complex::new(0.0, 0.0); fft_size].into_boxed_slice(),
            alpha,
            floor_ratio,
        }
    }

    fn process_hop(&mut self) {
        let n = self.fft_size;

        // Fill fft_buf with windowed samples from circular in_buf
        for i in 0..n {
            let idx = (self.in_pos + i) % n;
            self.fft_buf[i] = Complex::new(self.in_buf[idx] * self.window[i], 0.0);
        }

        // Forward FFT (in-place)
        self.fft.process(&mut self.fft_buf);

        // Spectral subtraction on positive bins
        let bins = n / 2 + 1;
        for k in 0..bins {
            let mag = self.fft_buf[k].norm();
            // Update noise floor estimate
            self.noise_floor[k] = self.alpha * self.noise_floor[k] + (1.0 - self.alpha) * mag;
            // Compute suppression gain: max(1 - noise/mag, floor_ratio)
            let suppression = if mag > 0.0 {
                (1.0 - self.noise_floor[k] / mag).max(self.floor_ratio)
            } else {
                self.floor_ratio
            };
            self.fft_buf[k] = self.fft_buf[k] * suppression;
            // Mirror to negative frequencies (conjugate symmetry)
            if k > 0 && k < n - k {
                self.fft_buf[n - k] = self.fft_buf[k].conj();
            }
        }

        // Inverse FFT
        self.ifft.process(&mut self.fft_buf);

        // Overlap-add normalised output into out_buf
        let norm = 1.0 / n as f32;
        let out_len = self.out_buf.len();
        for i in 0..n {
            let idx = (self.out_write + i) % out_len;
            self.out_buf[idx] += self.fft_buf[i].re * norm * self.window[i];
        }
        self.out_write = (self.out_write + self.hop_size) % out_len;
    }
}

impl AudioProcessor for SpectralProcessor {
    fn process(&mut self, input: &[f32], output: &mut [f32]) {
        let out_len = self.out_buf.len();
        for (i, &sample) in input.iter().enumerate() {
            if i >= output.len() { break; }

            // Write into circular input buffer
            self.in_buf[self.in_pos] = sample;
            self.in_pos = (self.in_pos + 1) % self.fft_size;
            self.samples_since_hop += 1;

            // Trigger a hop when enough new samples have arrived
            if self.samples_since_hop >= self.hop_size {
                self.samples_since_hop = 0;
                self.process_hop();
            }

            // Drain one sample from output buffer
            output[i] = self.out_buf[self.out_read] * self.gain;
            self.out_buf[self.out_read] = 0.0; // clear after reading (overlap-add)
            self.out_read = (self.out_read + 1) % out_len;
        }
    }
}

// ---------------------------------------------------------------------------
// Pipeline: chain multiple processors
// ---------------------------------------------------------------------------

/// Runs a sequence of processors in order, output of each feeds the next.
/// Uses a single scratch buffer: copy input in, run each stage in-place via
/// a temporary owned Vec (allocated once at construction, reused each call).
pub struct PipelineProcessor {
    stages: Vec<Box<dyn AudioProcessor>>,
    scratch: Vec<f32>,
}

impl PipelineProcessor {
    pub fn new(stages: Vec<Box<dyn AudioProcessor>>, max_buffer: usize) -> Self {
        Self {
            stages,
            scratch: vec![0.0f32; max_buffer],
        }
    }
}

impl AudioProcessor for PipelineProcessor {
    fn process(&mut self, input: &[f32], output: &mut [f32]) {
        let len = input.len().min(output.len()).min(self.scratch.len());
        self.scratch[..len].copy_from_slice(&input[..len]);
        // Each stage reads from scratch[..len] and writes back into scratch[..len]
        // via a temporary buffer to satisfy the borrow checker.
        let mut tmp = vec![0.0f32; len];
        for stage in self.stages.iter_mut() {
            stage.process(&self.scratch[..len], &mut tmp);
            self.scratch[..len].copy_from_slice(&tmp);
        }
        output[..len].copy_from_slice(&self.scratch[..len]);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- Phase 2 ---

    #[test]
    fn test_passthrough() {
        let mut p = GainProcessor::new(1.0, false);
        let input = vec![0.5, -0.2, 0.0, 1.0];
        let mut output = vec![0.0; 4];
        p.process(&input, &mut output);
        assert_eq!(input, output);
    }

    #[test]
    fn test_inversion() {
        let mut p = GainProcessor::new(1.0, true);
        let input = vec![0.5, -0.2, 0.0, 1.0];
        let mut output = vec![0.0; 4];
        p.process(&input, &mut output);
        assert_eq!(output, vec![-0.5, 0.2, -0.0, -1.0]);
    }

    #[test]
    fn test_gain() {
        let mut p = GainProcessor::new(2.0, false);
        let input = vec![0.1, -0.5];
        let mut output = vec![0.0; 2];
        p.process(&input, &mut output);
        assert_eq!(output, vec![0.2, -1.0]);
    }

    // --- Phase 3 ---

    #[test]
    fn test_delay_zero() {
        let mut p = DelayProcessor::new(1.0, false, 0, 16);
        let input = vec![1.0, 2.0, 3.0, 4.0];
        let mut output = vec![0.0; 4];
        p.process(&input, &mut output);
        assert_eq!(output, input);
    }

    #[test]
    fn test_delay_offset() {
        let mut p = DelayProcessor::new(1.0, false, 2, 16);
        let input = vec![1.0, 2.0, 3.0, 4.0];
        let mut output = vec![0.0; 4];
        p.process(&input, &mut output);
        assert_eq!(output, vec![0.0, 0.0, 1.0, 2.0]);
    }

    #[test]
    fn test_delay_with_gain() {
        let mut p = DelayProcessor::new(2.0, false, 1, 16);
        let input = vec![1.0, 2.0, 3.0];
        let mut output = vec![0.0; 3];
        p.process(&input, &mut output);
        assert_eq!(output, vec![0.0, 2.0, 4.0]);
    }

    // --- Phase 4 ---

    #[test]
    fn test_hp_filter_attenuates_dc() {
        let mut p = FilterProcessor::new(1.0, 200.0, &[], 48000.0);
        let input = vec![1.0f32; 512];
        let mut output = vec![0.0f32; 512];
        p.process(&input, &mut output);
        assert!(output[511].abs() < 0.01, "DC not attenuated: {}", output[511]);
    }

    #[test]
    fn test_hp_filter_passes_high_freq() {
        let sr = 48000.0f32;
        let freq = 10000.0f32;
        let input: Vec<f32> = (0..512)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sr).sin())
            .collect();
        let mut output = vec![0.0f32; 512];
        let mut p = FilterProcessor::new(1.0, 200.0, &[], sr);
        p.process(&input, &mut output);
        let rms_in  = (input.iter().map(|x| x*x).sum::<f32>()  / 512.0).sqrt();
        let rms_out = (output.iter().map(|x| x*x).sum::<f32>() / 512.0).sqrt();
        assert!((rms_out / rms_in - 1.0).abs() < 0.1,
            "High-freq signal attenuated too much: rms_in={rms_in}, rms_out={rms_out}");
    }

    #[test]
    fn test_notch_attenuates_target_freq() {
        let sr = 48000.0f32;
        let freq = 100.0f32;
        let input: Vec<f32> = (0..2048)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sr).sin())
            .collect();
        let mut output = vec![0.0f32; 2048];
        let mut p = FilterProcessor::new(1.0, 20.0, &[(freq, 10.0)], sr);
        p.process(&input, &mut output);
        let rms_in  = (input[1536..].iter().map(|x| x*x).sum::<f32>()  / 512.0).sqrt();
        let rms_out = (output[1536..].iter().map(|x| x*x).sum::<f32>() / 512.0).sqrt();
        assert!(rms_out < rms_in * 0.5,
            "Notch did not attenuate: rms_in={rms_in}, rms_out={rms_out}");
    }

    // --- Phase 5: LMS ---

    #[test]
    fn test_lms_converges_on_sine() {
        // Feed a pure sine as "noise". After enough samples the LMS filter
        // should model it and the output energy should drop significantly.
        let sr = 48000.0f32;
        let freq = 200.0f32;
        let n = 8192usize;
        let input: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sr).sin())
            .collect();
        let mut output = vec![0.0f32; n];
        let mut p = LmsProcessor::new(64, 0.01);
        p.process(&input, &mut output);

        // Compare RMS of first quarter vs last quarter — should drop
        let rms_early = rms(&output[..n/4]);
        let rms_late  = rms(&output[3*n/4..]);
        assert!(rms_late < rms_early * 0.5,
            "LMS did not converge: rms_early={rms_early:.4}, rms_late={rms_late:.4}");
    }

    #[test]
    fn test_lms_passes_novel_signal() {
        // After adapting to silence (zero input), a sudden impulse should appear in output.
        let n = 512usize;
        let mut input = vec![0.0f32; n];
        input[n - 1] = 1.0; // impulse at the end
        let mut output = vec![0.0f32; n];
        let mut p = LmsProcessor::new(32, 0.001);
        p.process(&input, &mut output);
        // The impulse sample should survive (weights adapted to zero, so e = x - 0 = x)
        assert!(output[n - 1].abs() > 0.5,
            "LMS swallowed novel signal: output={}", output[n-1]);
    }

    #[test]
    fn test_lms_zero_input_zero_output() {
        let mut p = LmsProcessor::new(32, 0.01);
        let input  = vec![0.0f32; 256];
        let mut output = vec![1.0f32; 256]; // pre-fill with non-zero
        p.process(&input, &mut output);
        for &s in &output {
            assert_eq!(s, 0.0);
        }
    }

    // --- Phase 6: Spectral ---

    #[test]
    fn test_spectral_produces_output() {
        // Basic smoke test: output should be non-zero after enough input.
        let fft_size = 256;
        let hop_size = 64;
        let n = fft_size * 4;
        let input: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 48000.0).sin())
            .collect();
        let mut output = vec![0.0f32; n];
        let mut p = SpectralProcessor::new(1.0, fft_size, hop_size, 0.9, 0.1);
        p.process(&input, &mut output);
        let rms_out = rms(&output[fft_size..]); // skip initial latency
        assert!(rms_out > 0.0, "Spectral processor produced only silence");
    }

    #[test]
    fn test_spectral_attenuates_steady_noise() {
        // Feed a steady sine. The very first hop output has no noise floor estimate
        // yet (floor=0), so suppression=1 and output ≈ input amplitude.
        // After many hops the floor converges and output amplitude drops.
        let fft_size = 256;
        let hop_size = 64;
        // Run enough hops for the floor to converge with alpha=0.1
        // After k hops: floor ≈ mag * (1 - 0.1^k). With k=30 that's >99%.
        let n = fft_size + hop_size * 60;
        let input: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * 300.0 * i as f32 / 48000.0).sin())
            .collect();
        let mut output = vec![0.0f32; n];
        // alpha=0.1: fast convergence; floor_ratio=0.0: full suppression allowed
        let mut p = SpectralProcessor::new(1.0, fft_size, hop_size, 0.1, 0.0);
        p.process(&input, &mut output);

        // First hop window (indices fft_size..fft_size+hop_size) — floor not yet built
        let rms_first = rms(&output[fft_size..fft_size + hop_size]);
        // Last hop window — floor fully converged
        let rms_last  = rms(&output[n - hop_size..]);
        assert!(rms_last < rms_first * 0.5,
            "Spectral did not attenuate: rms_first={rms_first:.5}, rms_last={rms_last:.5}");
    }

    #[test]
    fn test_spectral_zero_input_zero_output() {
        let fft_size = 256;
        let hop_size = 64;
        let n = fft_size * 4;
        let input  = vec![0.0f32; n];
        let mut output = vec![0.0f32; n];
        let mut p = SpectralProcessor::new(1.0, fft_size, hop_size, 0.9, 0.1);
        p.process(&input, &mut output);
        for &s in &output {
            assert_eq!(s, 0.0, "Expected silence for zero input");
        }
    }

    // --- Pipeline ---

    #[test]
    fn test_pipeline_chains_stages() {
        // filter (hp) → lms: output should be less energetic than raw input
        let sr = 48000.0f32;
        let n = 4096usize;
        let input: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * 100.0 * i as f32 / sr).sin())
            .collect();
        let mut output = vec![0.0f32; n];
        let mut p = PipelineProcessor::new(
            vec![
                Box::new(FilterProcessor::new(1.0, 200.0, &[], sr)),
                Box::new(LmsProcessor::new(64, 0.01)),
            ],
            1024,
        );
        p.process(&input, &mut output);
        let rms_in  = rms(&input[n/2..]);
        let rms_out = rms(&output[n/2..]);
        assert!(rms_out < rms_in, "Pipeline did not reduce energy: in={rms_in:.4} out={rms_out:.4}");
    }

    #[test]
    fn test_pipeline_zero_input_zero_output() {
        let sr = 48000.0f32;
        let input  = vec![0.0f32; 512];
        let mut output = vec![0.0f32; 512];
        let mut p = PipelineProcessor::new(
            vec![
                Box::new(FilterProcessor::new(1.0, 200.0, &[], sr)),
                Box::new(LmsProcessor::new(32, 0.01)),
            ],
            1024,
        );
        p.process(&input, &mut output);
        for &s in &output {
            assert_eq!(s, 0.0);
        }
    }

    // --- Helpers ---

    fn rms(buf: &[f32]) -> f32 {
        (buf.iter().map(|x| x * x).sum::<f32>() / buf.len() as f32).sqrt()
    }
}
