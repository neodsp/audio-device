use std::fmt::Debug;

use audio_blocks::{AudioBlockInterleavedView, AudioBlockInterleavedViewMut};
use rtaudio::{DeviceParams, Host, StreamHandle, StreamOptions};

pub type AudioDeviceResult<T> = Result<T, Box<dyn std::error::Error>>;

pub type Block<'a> = AudioBlockInterleavedView<'a, f32>;
pub type BlockMut<'a> = AudioBlockInterleavedViewMut<'a, f32>;

#[derive(Debug, Default)]
pub struct Config {
    pub num_input_channels: u16,
    pub num_output_channels: u16,
    pub sample_rate: u32,
    pub num_frames: usize,
}

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

pub struct AudioDevice {
    api: rtaudio::Api,
    input_device: rtaudio::DeviceInfo,
    output_device: rtaudio::DeviceInfo,
    stream_handle: Option<StreamHandle>,
}

impl Debug for AudioDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioDevice")
            .field("backend", &"RtAudio")
            .field("is_running", &self.stream_handle.is_some())
            .field("apis", &self.apis())
            .field("inputs", &self.inputs())
            .field("outputs", &self.outputs())
            .finish()
    }
}

impl AudioDevice {
    pub fn new() -> AudioDeviceResult<Self> {
        let host = Host::new(rtaudio::Api::Unspecified)?;

        Ok(Self {
            api: host.api(),
            input_device: host.default_input_device()?,
            output_device: host.default_output_device()?,
            stream_handle: None,
        })
    }

    pub fn api(&self) -> String {
        self.api.get_display_name()
    }

    pub fn apis(&self) -> Vec<String> {
        rtaudio::compiled_apis()
            .iter()
            .map(|a| a.get_display_name())
            .collect()
    }

    pub fn input(&self) -> String {
        self.input_device.name.clone()
    }

    pub fn output(&self) -> String {
        self.output_device.name.clone()
    }

    pub fn inputs(&self) -> Vec<DeviceInfo> {
        Host::new(self.api.clone())
            .unwrap()
            .iter_input_devices()
            .map(|i| DeviceInfo {
                name: i.name,
                num_channels: i.input_channels as u16,
            })
            .collect()
    }

    pub fn outputs(&self) -> Vec<DeviceInfo> {
        Host::new(self.api.clone())
            .unwrap()
            .iter_output_devices()
            .map(|i| DeviceInfo {
                name: i.name,
                num_channels: i.output_channels as u16,
            })
            .collect()
    }

    pub fn set_api(&mut self, name: &str) -> AudioDeviceResult<()> {
        self.api = rtaudio::compiled_apis()
            .iter()
            .find(|api| api.get_display_name().contains(name))
            .ok_or(AudioDeviceError::NotAvailable)?
            .clone();

        // update defaults
        let host = Host::new(self.api)?;
        self.input_device = host.default_input_device()?;
        self.output_device = host.default_output_device()?;

        Ok(())
    }

    pub fn set_input(&mut self, input: &str) -> AudioDeviceResult<()> {
        self.input_device = Host::new(self.api.clone())
            .unwrap()
            .iter_input_devices()
            .find(|device| device.name.contains(input))
            .ok_or(AudioDeviceError::NotAvailable)?
            .clone();
        Ok(())
    }

    pub fn set_output(&mut self, output: &str) -> AudioDeviceResult<()> {
        self.output_device = Host::new(self.api.clone())
            .unwrap()
            .iter_input_devices()
            .find(|device| device.name.contains(output))
            .ok_or(AudioDeviceError::NotAvailable)?
            .clone();
        Ok(())
    }

    pub fn start(
        &mut self,
        config: Config,
        mut process_fn: impl FnMut(Block, BlockMut) + Send + 'static,
    ) -> AudioDeviceResult<()> {
        self.stream_handle = Some(
            Host::new(self.api.clone())?
                .open_stream(
                    Some(DeviceParams {
                        device_id: self.output_device.id,
                        num_channels: config.num_output_channels as u32,
                        first_channel: 0,
                    }),
                    Some(DeviceParams {
                        device_id: self.input_device.id,
                        num_channels: config.num_input_channels as u32,
                        first_channel: 0,
                    }),
                    rtaudio::SampleFormat::Float32,
                    config.sample_rate,
                    config.num_frames as u32,
                    StreamOptions::default(),
                    move |_error| {},
                )
                .map_err(|(_, err)| err)?,
        );
        self.stream_handle
            .as_mut()
            .map(|handle| {
                handle.start(
                    move |buffers: rtaudio::Buffers<'_>,
                          info: &rtaudio::StreamInfo,
                          _status: rtaudio::StreamStatus| {
                        if let rtaudio::Buffers::Float32 { output, input } = buffers {
                            let input = Block::from_slice(
                                input,
                                info.in_channels as u16,
                                input.len() / info.in_channels,
                            );
                            let output = BlockMut::from_slice(
                                output,
                                info.out_channels as u16,
                                output.len() / info.out_channels,
                            );
                            process_fn(input, output);
                        }
                    },
                )
            })
            .transpose()?;

        Ok(())
    }

    pub fn stop(&mut self) -> AudioDeviceResult<()> {
        if let Some(mut stream_handle) = self.stream_handle.take() {
            stream_handle.stop();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use audio_blocks::{AudioBlock, AudioBlockOps};

    use super::*;

    #[test]
    fn rtaudio_test() {
        let mut device = AudioDevice::new().unwrap();
        dbg!(device.apis());
        dbg!(device.inputs());
        dbg!(device.outputs());

        dbg!(device.api());
        dbg!(device.input());
        dbg!(device.output());

        device.set_api(&device.api()).unwrap();
        device.set_input(&device.input()).unwrap();
        device.set_output(&device.output()).unwrap();

        device
            .start(
                Config {
                    num_input_channels: 2,
                    num_output_channels: 2,
                    sample_rate: 48000,
                    num_frames: 512,
                },
                |input, mut output| {
                    assert_eq!(input.num_frames(), 512);
                    assert_eq!(input.num_channels(), 2);
                    assert_eq!(output.num_frames(), 512);
                    assert_eq!(output.num_channels(), 2);
                    output.copy_from_block(&input);
                },
            )
            .unwrap();

        std::thread::sleep(std::time::Duration::from_secs(3));

        device.stop().unwrap();
    }
}
