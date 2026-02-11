# Project: Rust Real-Time Adaptive Noise Suppression (Analog Jack, Linux)

## Overview

This project is an experimental real-time adaptive noise suppression engine written in Rust for Linux, designed specifically for:

* Analog 3.5mm TRRS wired headphones
* Microphone positioned near the mouth
* Linux audio stack (ALSA + PipeWire/PulseAudio)

Because the headset is analog and uses the laptop’s internal ADC/DAC, true hardware-grade Active Noise Cancellation (ANC) is physically infeasible due to:

* ADC/DAC conversion latency
* OS buffering
* Single microphone (no external reference mic)
* No internal ear feedback loop

Therefore, this project focuses on:

> Real-time adaptive noise suppression and experimental phase-based cancellation under analog hardware constraints.

This is a research/learning systems project.

---

# Hardware & Signal Constraints

## Physical Setup

* Wired 3.5mm TRRS headset
* Single analog microphone near mouth
* Audio routed through laptop sound card
* No external ambient reference mic
* No internal ear feedback mic

## Real Signal Path

```
Mic (Analog)
  ↓
Laptop ADC
  ↓
ALSA
  ↓
PipeWire / PulseAudio
  ↓
Rust DSP Engine
  ↓
PipeWire / PulseAudio
  ↓
ALSA
  ↓
Laptop DAC
  ↓
Headphones
```

Each stage introduces latency.

Expected round-trip latency baseline (before DSP):

> 10–30ms

This prevents perfect phase-aligned ANC.

---

# Project Goals (Adjusted for Reality)

## Primary Goal

Reduce perceived ambient noise energy in real time during bus rides.

## Secondary Goals

* Learn real-time audio programming in Rust
* Explore low-latency Linux audio
* Implement adaptive DSP algorithms
* Understand physical limits of software ANC
* Build a real-time safe audio engine

## Success Criteria

* Subjective improvement in perceived noise
* Reduced low-frequency rumble
* No audio glitches
* Stable real-time performance

Not required:

* Perfect silence
* Commercial ANC quality

---

# Technical Strategy

Because true ANC is infeasible, we will implement:

## Phase 1 – Real-Time Audio Pipeline

Objective:
Build a stable, low-latency mic → process → headphone loop.

Requirements:

* No heap allocations in audio callback
* No mutex locking
* No logging in callback
* Pre-allocated buffers
* Lock-free ring buffer (if needed)

Crates:

* `cpal`
* `ringbuf`
* `clap`

---

## Phase 2 – Phase Inversion Experiment

Test basic cancellation hypothesis:

```
output = -gain * input
```

Expected:
Minimal cancellation due to latency misalignment.

Purpose:
Measure actual system latency and observe behavior.

---

## Phase 3 – Adjustable Delay & Gain

Introduce:

* Manual delay offset (in samples)
* Runtime gain adjustment

Goal:
Experiment with phase alignment.

Even if imperfect, this builds understanding of:

* Buffer timing
* Sample alignment
* Audio drift

---

## Phase 4 – Band-Limited Noise Suppression

Bus noise is typically:

* Low frequency rumble (20–200 Hz)
* Broadband mechanical vibration

Implement:

* High-pass filter (to reduce rumble)
* Optional notch filters
* Simple FIR or IIR filters

Goal:
Suppress dominant low-frequency components.

---

## Phase 5 – Adaptive LMS Filter

Implement Least Mean Squares adaptive filter.

Concept:

The filter dynamically adjusts weights to minimize output energy.

Pseudo-formula:

```
y[n] = Σ w[k] * x[n-k]
w[k] += μ * e[n] * x[n-k]
```

Where:

* μ = learning rate
* w = filter coefficients
* x = mic input

Goal:
Continuously adapt to steady-state noise patterns.

Expected:
Better suppression of persistent bus noise.

Risk:
Instability if μ is too high.

---

## Phase 6 – Optional Spectral Suppression

Using `rustfft`:

* Perform short FFT windows
* Estimate noise floor
* Reduce stable frequency bins
* Reconstruct via inverse FFT

This becomes real-time spectral subtraction.

More computationally expensive.

---

# Latency Optimization Strategy

## Target Configuration

Sample rate: 48000 Hz
Buffer size: 32–128 samples

Latency math:

64 samples @ 48kHz ≈ 1.33ms per buffer

Realistic total roundtrip:

15–40ms

True 2–5ms end-to-end likely unattainable with analog jack hardware.

---

## Linux Optimization

* Prefer PipeWire over PulseAudio
* Reduce buffer size via system settings
* Set real-time thread priority (`SCHED_FIFO`)
* Pin DSP thread to CPU core (optional)
* Disable CPU frequency scaling (optional)

---

# CLI Interface

Example usage:

```
cargo run -- \
  --buffer 64 \
  --mode lms \
  --gain 1.0 \
  --learning-rate 0.00005 \
  --filter-length 64
```

Modes:

* invert
* delay
* filter
* lms
* spectral

---

# Real-Time Safety Rules

Inside audio callback:

DO NOT:

* Allocate memory
* Use println!
* Use mutex locks
* Perform blocking calls
* Resize vectors

DO:

* Use preallocated arrays
* Reuse buffers
* Keep computation deterministic
* Avoid branching where possible

---

# Known Limitations

* Mouth-positioned mic hears mixed signal
* User voice will also be suppressed
* High latency prevents perfect phase cancellation
* Adaptive filters may become unstable
* OS scheduling may cause dropouts

These are acceptable.

---

# Stretch Goals

* Terminal waveform visualizer
* Spectrogram view in ASCII
* Real-time RMS energy meter
* Auto noise floor detection
* Dynamic learning rate adjustment
* Record before/after samples
* Measure actual roundtrip latency experimentally

---

# Educational Outcomes

By completing this project, you will deeply understand:

* Real-time systems constraints
* Linux audio stack behavior
* DSP fundamentals
* Adaptive filtering theory
* Audio buffer scheduling
* Rust performance in time-critical environments
* The physical limits of software-only ANC

---

# Project Philosophy

This project is not about competing with AirPods.

It is about:

* Challenging hardware assumptions
* Testing physical limits
* Writing real-time safe Rust
* Understanding why commercial ANC requires specialized hardware
* Engineering curiosity

---
