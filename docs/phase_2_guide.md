# Phase 2: Phase Inversion Experiment - Step-by-Step Guide

This document provides a detailed walkthrough of Phase 2 implementation for **Hushr**. Phase 2 focuses on testing the basic hypothesis of noise cancellation through phase inversion.

## Objective
To build a functional real-time audio loop that captures microphone input, inverts its phase (multiplying by -1.0), and plays it back through headphones with minimal latency.

---

## Step 1: Define CLI Arguments
We need a way to control the engine at runtime. We will use `clap` to define the following parameters:
- `--gain`: Adjusts the output volume (default: 1.0).
- `--invert`: Toggles phase inversion (true for Phase 2).
- `--buffer`: Sets the desired buffer size in samples (smaller = lower latency).

## Step 2: Initialize Audio Hardware
Using the `cpal` crate, we will:
1. Enumerate available audio hosts (ALSA/PipeWire).
2. Identify the default input device (Microphone).
3. Identify the default output device (Headphones/Speakers).
4. Verify that both devices support the same sample format (f32) and sample rate (48kHz).

## Step 3: Set Up the DSP Engine
We will use the `AudioProcessor` trait and `GainProcessor` struct defined in `src/dsp.rs`.
- The `GainProcessor` will be initialized with the `--gain` and `--invert` values from the CLI.
- It will live in the main thread and be shared or moved into the audio processing thread.

## Step 4: Implement the Audio Callback
The "heart" of the system. This function runs at high priority and must never block.
1. The input stream captures samples from the mic.
2. The samples are passed through `GainProcessor::process`.
3. The processed samples (inverted) are sent to the output stream.
4. **Safety Rule:** No `println!`, no `new Vec`, no `Mutex` inside this loop.

## Step 5: Performance & Latency Tuning
To achieve the best results for Phase 2:
- We will request a small buffer size (e.g., 64 or 128 samples).
- We will use `48,000 Hz` as the standard sample rate.

## Step 6: Measurement & Observation
Once running, you should observe:
- **With `--invert false`**: You hear your own voice clearly (Side-tone).
- **With `--invert true`**: You hear your voice, but it might sound "thin" or "hollow" if the latency is low enough to cause interference.
- **Latency Measurement**: Use this phase to estimate the delay between the physical sound and the inverted playback.

---

## How to Run Phase 2
After implementation, use the following command:
```bash
cargo run -- --gain 1.0 --invert true --buffer 128
```

## Expected Results (The Reality Check)
Because we are using a standard Linux audio stack and analog hardware:
1. **Latency (15-40ms)** will be too high for significant acoustic cancellation.
2. You will likely hear an **echo** rather than silence.
3. This step is critical for calibrating the system before moving to **Phase 3 (Adjustable Delay)**.
