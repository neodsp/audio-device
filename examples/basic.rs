use std::error::Error;

use audio_blocks::AudioBlockOpsMut;
use audio_device::{AudioDevice, AudioDeviceTrait, Config};

fn main() -> Result<(), Box<dyn Error>> {
    let mut device = AudioDevice::new()?;

    // get available devices
    println!("{:#?}", device.apis());
    println!("{:#?}", device.inputs());
    println!("{:#?}", device.outputs());

    // get current selected devices
    println!("{:#?}", device.api());
    println!("{:#?}", device.input());
    println!("{:#?}", device.output());

    // select new devices
    device.set_api(&device.api()).unwrap();
    device.set_input(&device.input()).unwrap();
    device.set_output(&device.output()).unwrap();

    // start audio device
    device
        .start(
            Config {
                num_input_channels: 2,
                num_output_channels: 2,
                sample_rate: 48000,
                num_frames: 1024,
            },
            move |input, mut output| {
                if output.copy_from_block(&input).is_some() {
                    eprintln!("Input and Output buffer did not have a similar size");
                }
            },
        )
        .unwrap();

    std::thread::sleep(std::time::Duration::from_secs(10));

    // stop audio device
    device.stop().unwrap();

    Ok(())
}
