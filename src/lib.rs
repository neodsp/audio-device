mod compile_error;

#[cfg(feature = "cpal")]
pub mod backend_cpal;
#[cfg(feature = "juce")]
pub mod backend_juce;
#[cfg(feature = "rtaudio")]
pub mod backend_rtaudio;

#[cfg(feature = "cpal")]
pub use backend_cpal::AudioHost;
#[cfg(feature = "juce")]
pub use backend_juce::AudioHost;
#[cfg(feature = "rtaudio")]
pub use backend_rtaudio::AudioHost;

#[derive(thiserror::Error, Debug)]
pub enum AudioHostError {
    #[error("Device or API not found")]
    NotFound,
    #[error("Backend error: {0}")]
    Backend(#[source] Box<dyn std::error::Error + Send + Sync>),
}

pub type Block<'a> = InterleavedView<'a, f32>;
pub type BlockMut<'a> = InterleavedViewMut<'a, f32>;

pub use audio_blocks::*;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DeviceInfo {
    pub name: String,
    pub num_channels: u16,
}

#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Config {
    pub num_input_channels: u16,
    pub num_output_channels: u16,
    pub sample_rate: u32,
    pub num_frames: usize,
}

/// Trait defining the common interface for audio devices
pub trait AudioHostTrait {
    /// Create a new audio device with default settings
    fn new() -> Result<Self, AudioHostError>
    where
        Self: Sized;

    /// Get the current API/host name
    fn api(&self) -> String;

    /// Get all available APIs/hosts
    fn apis(&self) -> Vec<String>;

    /// Get the current input device name
    fn input(&self) -> String;

    /// Get the current output device name
    fn output(&self) -> String;

    /// Get all available input devices
    fn inputs(&self) -> Vec<DeviceInfo>;

    /// Get all available output devices
    fn outputs(&self) -> Vec<DeviceInfo>;

    /// Set the API/host by name
    fn set_api(&mut self, name: &str) -> Result<(), AudioHostError>;

    /// Set the input device by name
    fn set_input(&mut self, input: &str) -> Result<(), AudioHostError>;

    /// Set the output device by name
    fn set_output(&mut self, output: &str) -> Result<(), AudioHostError>;

    /// Start the audio stream with the given configuration and process callback
    fn start(
        &mut self,
        config: Config,
        process_fn: impl FnMut(Block, BlockMut) + Send + 'static,
    ) -> Result<(), AudioHostError>;

    /// Stop the audio stream
    fn stop(&mut self) -> Result<(), AudioHostError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_device() {
        let mut host = AudioHost::new().unwrap();
        dbg!(host.apis());
        dbg!(host.inputs());
        dbg!(host.outputs());

        dbg!(host.api());
        dbg!(host.input());
        dbg!(host.output());

        host.set_api(&host.api()).unwrap();
        host.set_input(&host.input()).unwrap();
        host.set_output(&host.output()).unwrap();

        let num_frames = 1024;

        host.start(
            Config {
                num_input_channels: 2,
                num_output_channels: 2,
                sample_rate: 48000,
                num_frames,
            },
            move |input, mut output| {
                if output.copy_from_block(&input).is_some() {
                    eprintln!("Input and Output buffer did not have a similar size");
                }
            },
        )
        .unwrap();

        std::thread::sleep(std::time::Duration::from_secs(3));

        host.stop().unwrap();
    }
}
