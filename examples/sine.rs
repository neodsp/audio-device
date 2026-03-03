use audio_io::{AudioBlockOpsMut, AudioDevice, AudioDeviceResult, AudioDeviceTrait, Config};

struct Oscillator {
    phasor: f32,
    phasor_inc: f32,
}

impl Oscillator {
    fn new(sample_rate: u32, frequency: f32) -> Self {
        Self {
            phasor: 0.0,
            phasor_inc: frequency / sample_rate as f32,
        }
    }

    fn next_sample(&mut self) -> f32 {
        let sample = (self.phasor * std::f32::consts::TAU).sin();
        self.phasor = (self.phasor + self.phasor_inc).fract();
        sample
    }
}

fn main() -> AudioDeviceResult<()> {
    let mut device = AudioDevice::new()?;

    let sample_rate = 48000;
    let mut osc = Oscillator::new(sample_rate, 440.0);

    device
        .start(
            Config {
                num_input_channels: 0,
                num_output_channels: 2,
                sample_rate,
                num_frames: 1024,
            },
            move |_, mut output| {
                for frame in output.frames_mut() {
                    frame.fill(osc.next_sample());
                }
                output.gain(0.5);
            },
        )
        .unwrap();

    std::thread::sleep(std::time::Duration::from_secs(2));

    device.stop().unwrap();

    Ok(())
}
