# Hushr Developer Guide

Welcome to **Hushr**, a real-time adaptive noise suppression engine. This document provides the necessary steps to set up your environment and contribute to the project.

> **⚠️ NOTE:** This project is strictly **Linux-based**. It relies on the Linux audio stack (ALSA/PipeWire) and low-level system headers that are not available on Windows or macOS.

---

## 1. System Requirements

Before you can compile the Rust code, you must install the necessary system-level libraries for audio interfacing on Linux.

### Audio Dependencies (Debian/Ubuntu/Zorin)
The project uses `cpal`, which requires the ALSA development headers to communicate with your sound hardware.

Run the following command:
```bash
sudo apt-get update && sudo apt-get install -y libasound2-dev libjack-dev pkg-config
```

*   `libasound2-dev`: Development files for the Advanced Linux Sound Architecture (ALSA).
*   `libjack-dev`: Development files for the JACK Audio Connection Kit (optional but recommended for low-latency routing).
*   `pkg-config`: Required by Cargo to locate these system libraries during the build.

---

## 2. Setting Up Rust

We recommend using `rustup` to manage your Rust installation.

1.  **Install Rust:**
    ```bash
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    ```
2.  **Configure PATH:**
    ```bash
    source $HOME/.cargo/env
    ```
3.  **Update to latest stable:**
    ```bash
    rustup update stable
    ```

---

## 3. Project Structure

- `src/main.rs`: The entry point. Handles CLI arguments and the live audio loop.
- `src/dsp.rs`: The core DSP logic. Contains the `AudioProcessor` trait and implementations like `GainProcessor`.
- `Cargo.toml`: Project configuration and Rust dependencies (`cpal`, `ringbuf`, `rustfft`).

---

## 4. Development Workflow

### Running the Program
To build and run the program in debug mode (use this for local development):
```bash
cargo run
```

### Running Tests
We use strict unit testing for all DSP algorithms to ensure mathematical correctness before deploying to live hardware.
```bash
cargo test
```

### Generating Documentation
You can generate and view the internal API documentation (which includes our docstrings) by running:
```bash
cargo doc --open
```

---

## 5. Coding Standards: Real-Time Safety

When contributing to the audio processing loop (anything inside `AudioProcessor::process`):

1.  **NO ALLOCATIONS:** Do not use `Vec`, `String`, `Box`, or anything that allocates memory on the heap. This causes non-deterministic pauses (clicks/pops in audio).
2.  **NO LOCKS:** Avoid `Mutex` or `RwLock`. Use lock-free buffers (like `ringbuf`) if you need to communicate between threads.
3.  **NO I/O:** Do not use `println!` or file operations inside the audio callback.
4.  **DETERMINISTIC MATH:** Ensure your algorithms have a predictable execution time to avoid buffer underruns.

---

## 6. Architecture Overview

This project follows a trait-based architecture. To add a new algorithm (e.g., Phase 4 Band-Limited Filters):
1. Create a new struct in `dsp.rs`.
2. Implement the `AudioProcessor` trait for it.
3. Write unit tests to verify the math.
4. Integrate it into the `main.rs` selection logic.
