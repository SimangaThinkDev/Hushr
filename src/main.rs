use clap::Parser;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::{traits::*, HeapRb};
use std::time::Duration;

mod dsp;
use dsp::{AudioProcessor, DelayProcessor, FilterProcessor, GainProcessor, LmsProcessor, PipelineProcessor, SpectralProcessor};

/// Hushr: Real-time adaptive noise suppression engine.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Processing mode: passthrough | invert | delay | filter | lms | spectral
    #[arg(short, long, default_value = "passthrough")]
    mode: String,

    /// Output gain (volume multiplier).
    #[arg(short, long, default_value_t = 1.0)]
    gain: f32,

    /// Invert phase — applies in invert and delay modes.
    #[arg(short, long, default_value_t = false)]
    invert: bool,

    /// Delay offset in samples (delay mode).
    #[arg(short, long, default_value_t = 0)]
    delay: usize,

    /// High-pass cutoff frequency in Hz (filter mode).
    #[arg(long, default_value_t = 200.0)]
    hp_cutoff: f32,

    /// Notch filter frequencies in Hz (filter mode). Repeat for multiple: --notch 50 --notch 100
    #[arg(long)]
    notch: Vec<f32>,

    /// Notch filter Q sharpness (filter mode).
    #[arg(long, default_value_t = 10.0)]
    notch_q: f32,

    /// LMS filter length in taps (lms mode).
    #[arg(long, default_value_t = 64)]
    filter_length: usize,

    /// LMS learning rate μ (lms mode). Too high causes instability.
    #[arg(long, default_value_t = 0.00005)]
    learning_rate: f32,

    /// FFT window size, must be power of two (spectral mode).
    #[arg(long, default_value_t = 1024)]
    fft_size: usize,

    /// Hop size in samples, typically fft_size/4 (spectral mode).
    #[arg(long, default_value_t = 256)]
    hop_size: usize,

    /// Noise floor smoothing factor 0–1 (spectral mode). Higher = slower adaptation.
    #[arg(long, default_value_t = 0.95)]
    alpha: f32,

    /// Spectral floor ratio — minimum per-bin gain after subtraction (spectral mode).
    #[arg(long, default_value_t = 0.1)]
    floor_ratio: f32,

    /// Buffer size in samples.
    #[arg(short, long, default_value_t = 128)]
    buffer: u32,

    /// Sample rate in Hz.
    #[arg(short, long, default_value_t = 48000)]
    sample_rate: u32,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    println!("Starting Hushr [mode={}]...", args.mode);

    // 1. Setup Audio Host & Devices
    let host = cpal::default_host();
    let input_device = host
        .default_input_device()
        .ok_or("Failed to get default input device")?;
    let output_device = host
        .default_output_device()
        .ok_or("Failed to get default output device")?;

    println!("Input:  {}", input_device.name()?);
    println!("Output: {}", output_device.name()?);

    // 2. Configure Stream
    let config = cpal::StreamConfig {
        channels: 1,
        sample_rate: args.sample_rate.into(),
        buffer_size: cpal::BufferSize::Fixed(args.buffer),
    };

    // 3. Create Processor
    let sr = args.sample_rate as f32;
    let processor: Box<dyn AudioProcessor> = match args.mode.as_str() {
        "invert" => {
            println!("gain={}, invert=true", args.gain);
            Box::new(GainProcessor::new(args.gain, true))
        }
        "delay" => {
            println!("gain={}, invert={}, delay={} samples", args.gain, args.invert, args.delay);
            Box::new(DelayProcessor::new(args.gain, args.invert, args.delay, sr as usize))
        }
        "filter" => {
            let notch_params: Vec<(f32, f32)> = args.notch.iter().map(|&hz| (hz, args.notch_q)).collect();
            println!("gain={}, hp_cutoff={}Hz, notches={:?}", args.gain, args.hp_cutoff, notch_params);
            Box::new(FilterProcessor::new(args.gain, args.hp_cutoff, &notch_params, sr))
        }
        "lms" => {
            println!("filter_length={}, learning_rate={}", args.filter_length, args.learning_rate);
            Box::new(LmsProcessor::new(args.filter_length, args.learning_rate))
        }
        "spectral" => {
            println!(
                "fft_size={}, hop_size={}, alpha={}, floor_ratio={}",
                args.fft_size, args.hop_size, args.alpha, args.floor_ratio
            );
            Box::new(SpectralProcessor::new(
                args.gain, args.fft_size, args.hop_size, args.alpha, args.floor_ratio,
            ))
        }
        "all" => {
            let notch_params: Vec<(f32, f32)> = args.notch.iter().map(|&hz| (hz, args.notch_q)).collect();
            println!("pipeline: filter → lms → spectral");
            Box::new(PipelineProcessor::new(
                vec![
                    Box::new(FilterProcessor::new(1.0, args.hp_cutoff, &notch_params, sr)),
                    Box::new(LmsProcessor::new(args.filter_length, args.learning_rate)),
                    Box::new(SpectralProcessor::new(args.gain, args.fft_size, args.hop_size, args.alpha, args.floor_ratio)),
                ],
                1024,
            ))
        }
        _ => {
            println!("gain={}", args.gain);
            Box::new(GainProcessor::new(args.gain, false))
        }
    };

    // 4. Ring Buffer
    let latency_samples = (args.buffer as usize) * 2;
    let rb = HeapRb::<f32>::new(latency_samples);
    let (mut producer, mut consumer) = rb.split();

    // 5. Input Stream
    let input_data_config = config.clone();
    let mut processor = processor; // move into closure
    let input_stream = input_device.build_input_stream(
        &input_data_config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let mut scratch = [0.0f32; 1024];
            let len = data.len().min(scratch.len());
            processor.process(&data[..len], &mut scratch[..len]);
            for &sample in &scratch[..len] {
                let _ = producer.try_push(sample);
            }
        },
        |err| eprintln!("Input stream error: {}", err),
        None,
    )?;

    // 6. Output Stream
    let output_data_config = config.clone();
    let output_stream = output_device.build_output_stream(
        &output_data_config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            for sample in data.iter_mut() {
                *sample = consumer.try_pop().unwrap_or(0.0);
            }
        },
        |err| eprintln!("Output stream error: {}", err),
        None,
    )?;

    // 7. Start
    input_stream.play()?;
    output_stream.play()?;

    println!("Engine running. Press Ctrl+C to stop.");
    loop {
        std::thread::sleep(Duration::from_millis(100));
    }
}
