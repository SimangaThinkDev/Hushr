/// A trait for audio processing components.
/// This ensures we can swap out different algorithms (Phase 2, 4, 5, etc.)
pub trait AudioProcessor {
    /// Processes a block of audio samples.
    /// 
    /// Takes an `input` slice of 32-bit floats and writes the result into an `output` slice.
    /// This method is designed to be "real-time safe": no allocations or blocking should occur here.
    fn process(&mut self, input: &[f32], output: &mut [f32]);
}

/// Simple processor that scales the input and potentially inverts phase.
/// Used for Phase 2 (Inversion) experiments and basic volume (Gain) control.
pub struct GainProcessor {
    /// The volume multiplier.
    pub gain: f32,
    /// If true, multiplies the signal by -1.0 to flip the phase.
    pub invert: bool,
}

impl GainProcessor {
    /// Creates a new GainProcessor with a specific volume and inversion setting.
    pub fn new(gain: f32, invert: bool) -> Self {
        Self { gain, invert }
    }
}

impl AudioProcessor for GainProcessor {
    /// Implementation of the audio processing loop for gain and phase inversion.
    /// Each sample is multiplied by a calculated multiplier (gain * inversion-factor).
    fn process(&mut self, input: &[f32], output: &mut [f32]) {
        let multiplier = if self.invert { -self.gain } else { self.gain };
        for (i, &sample) in input.iter().enumerate() {
            if i < output.len() {
                output[i] = sample * multiplier;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that with gain=1.0 and invert=false, the output matches the input exactly.
    #[test]
    fn test_passthrough() {
        let mut processor = GainProcessor::new(1.0, false);
        let input = vec![0.5, -0.2, 0.0, 1.0];
        let mut output = vec![0.0; 4];
        
        processor.process(&input, &mut output);
        assert_eq!(input, output);
    }

    /// Verifies that invert=true correctly flips the sign of all samples.
    #[test]
    fn test_inversion() {
        let mut processor = GainProcessor::new(1.0, true);
        let input = vec![0.5, -0.2, 0.0, 1.0];
        let mut output = vec![0.0; 4];
        
        processor.process(&input, &mut output);
        assert_eq!(output, vec![-0.5, 0.2, -0.0, -1.0]);
    }

    /// Verifies that gain > 1.0 correctly increases the amplitude of samples.
    #[test]
    fn test_gain() {
        let mut processor = GainProcessor::new(2.0, false);
        let input = vec![0.1, -0.5];
        let mut output = vec![0.0; 2];
        
        processor.process(&input, &mut output);
        assert_eq!(output, vec![0.2, -1.0]);
    }
}
