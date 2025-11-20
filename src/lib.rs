#[cfg(feature = "cpal")]
pub mod device_cpal;
#[cfg(feature = "rtaudio")]
pub mod device_rtaudio;

pub type AudioDeviceResult<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(thiserror::Error, Debug)]
pub enum AudioDeviceError {
    #[error("Wanted setting not available, leaving at default")]
    NotAvailable,
}

#[derive(Debug)]
pub struct DeviceInfo {
    pub name: String,
    pub num_channels: u16,
}

#[derive(Debug, Default)]
pub struct Config {
    pub num_input_channels: u16,
    pub num_output_channels: u16,
    pub sample_rate: u32,
    pub num_frames: usize,
}
