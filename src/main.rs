use clap::Parser;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::{traits::*, HeapRb};
use std::time::Duration;

mod dsp;
use dsp::{AudioProcessor, GainProcessor};

/// Hushr: Real-time adaptive noise suppression engine.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Output gain (volume multiplier).
    #[arg(short, long, default_value_t = 1.0)]
    gain: f32,

    /// Invert phase (multiply by -1.0).
    #[arg(short, long, default_value_t = false)]
    invert: bool,

    /// Buffer size in samples.
    #[arg(short, long, default_value_t = 128)]
    buffer: u32,

    /// Sample rate in Hz.
    #[arg(short, long, default_value_t = 48000)]
    sample_rate: u32,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    println!("Starting Hushr Phase 2...");
    println!("Settings: gain={}, invert={}, buffer={}, sample_rate={}", 
             args.gain, args.invert, args.buffer, args.sample_rate);

    // 1. Setup Audio Host & Devices
    let host = cpal::default_host();
    let input_device = host.default_input_device()
        .ok_or("Failed to get default input device")?;
    let output_device = host.default_output_device()
        .ok_or("Failed to get default output device")?;

    println!("Input: {}", input_device.name()?);
    println!("Output: {}", output_device.name()?);

    // 2. Configure Stream
    let config = cpal::StreamConfig {
        channels: 1, // Mono for processing
        sample_rate: args.sample_rate.into(),
        buffer_size: cpal::BufferSize::Fixed(args.buffer),
    };

    // 3. Create Processors
    let mut processor = GainProcessor::new(args.gain, args.invert);

    // 4. Setup Ring Buffer for Inter-stream communication
    // We need enough space to handle some jitter, but not so much that we add latency.
    // 2x buffer size is a safe minimum.
    let latency_samples = (args.buffer as usize) * 2;
    let rb = HeapRb::<f32>::new(latency_samples);
    let (mut producer, mut consumer) = rb.split();

    // 5. Build Input Stream
    let input_data_config = config.clone();
    let input_stream = input_device.build_input_stream(
        &input_data_config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            // Real-time safe processing: Avoid heap allocations.
            // We use a stack-allocated buffer for processing. 
            // 1024 is a reasonable upper bound for low-latency audio buffers.
            let mut scratch = [0.0f32; 1024];
            let len = data.len().min(scratch.len());
            let input_slice = &data[..len];
            let output_slice = &mut scratch[..len];

            processor.process(input_slice, output_slice);
            
            for &sample in output_slice.iter() {
                if producer.try_push(sample).is_err() {
                    // Buffer overflow
                }
            }
        },
        |err| eprintln!("Input stream error: {}", err),
        None
    )?;

    // 6. Build Output Stream
    let output_data_config = config.clone();
    let output_stream = output_device.build_output_stream(
        &output_data_config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            for sample in data.iter_mut() {
                *sample = consumer.try_pop().unwrap_or(0.0);
            }
        },
        |err| eprintln!("Output stream error: {}", err),
        None
    )?;

    // 7. Start Streams
    input_stream.play()?;
    output_stream.play()?;

    println!("Engine running. Press Ctrl+C to stop.");
    
    // Keep the main thread alive
    loop {
        std::thread::sleep(Duration::from_millis(100));
    }
}
