use audio_io::{AudioDevice, AudioDeviceResult, AudioDeviceTrait, Config};

const AMPLITUDE: f32 = 0.5;
const FREQ_HZ: f32 = 440.0;
const SAMPLE_RATE: u32 = 48000;

fn main() -> AudioDeviceResult<()> {
    let mut device = AudioDevice::new()?;

    let mut phasor = 0.0;
    let phasor_inc = FREQ_HZ / SAMPLE_RATE as f32;

    // start audio device
    device
        .start(
            Config {
                num_input_channels: 2,
                num_output_channels: 2,
                sample_rate: SAMPLE_RATE,
                num_frames: 1024,
            },
            move |_, mut output| {
                for frame in output.frames_mut() {
                    let val = (phasor * std::f32::consts::TAU).sin() * AMPLITUDE;
                    phasor = (phasor + phasor_inc).fract();

                    for sample in frame {
                        *sample = val;
                    }
                }
            },
        )
        .unwrap();

    std::thread::sleep(std::time::Duration::from_secs(2));

    // stop audio device
    device.stop().unwrap();

    Ok(())
}
