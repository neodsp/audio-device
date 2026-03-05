use audio_io::{AudioBackend, AudioHost, AudioHostError};

fn main() -> Result<(), AudioHostError> {
    let device = AudioHost::new()?;

    // get available devices
    println!("APIs: {:#?}\n", device.apis());
    println!("Inputs: {:#?}\n", device.inputs());
    println!("Outputs: {:#?}\n", device.outputs());

    // get current selected devices
    println!("Selected API: {:#?}", device.api());
    println!("Selected Input: {:#?}", device.input());
    println!("Selected Output: {:#?}", device.output());

    Ok(())
}
