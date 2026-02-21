# Implementation Plan - Phase 2: Phase Inversion Experiment

This plan outlines the steps to implement Phase 2 of the Hushr project, which involves setting up a stable audio pipeline and applying phase inversion for experimental noise cancellation.

## Phase 1 Readiness Check (Implicit)
Since `main.rs` is currently a skeleton, we must first establish the core audio pipeline (Phase 1) before we can apply Phase 2 logic.

## Proposed Steps

### 1. Document Phase 2 Procedures
Create `docs/phase_2_implementation.md` with detailed instructions for the user on how Phase 2 works, how to test it, and what to expect.

### 2. Update `main.rs` - CLI & Audio Logic
- Implement CLI argument parsing using `clap` (Mode, Gain, Invert).
- Initialize `CPAL` for audio I/O.
- Select default Host and Terminal devices (Mic and Headphones).
- Build and run the Input/Output streams.

### 3. Integrate `dsp.rs`
- Instantiate `GainProcessor` based on CLI flags.
- Pass audio buffers through the processor in the `cpal` callback.

### 4. Real-Time Safety Verification
- Ensure no allocations, locks, or I/O happen within the audio callback.
- Use `cpal`'s stream configuration to minimize latency.

### 5. Testing & Validation
- Run existing tests in `dsp.rs`.
- Build the project to ensure all dependencies are correctly linked.
