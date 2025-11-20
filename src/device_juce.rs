use std::{fmt::Debug, pin::Pin};

use audio_blocks::{AudioBlock, AudioBlockInterleaved, AudioBlockMut};
use cxx_juce::{
    JUCE,
    juce_audio_basics::AudioSampleBuffer,
    juce_audio_devices::{
        AudioDeviceCallback, AudioDeviceManager, AudioDeviceType, ChannelCount,
        device_manager::AudioCallbackHandle,
    },
};

use crate::{AudioDeviceResult, AudioDeviceTrait, Block, BlockMut, Config, DeviceInfo};

pub struct AudioDevice {
    _juce: JUCE,
    apis: Vec<String>,
    device_manager: AudioDeviceManager,
    handle: Option<AudioCallbackHandle>,
}

impl Debug for AudioDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioDevice")
            .field("backend", &"JUCE")
            .field("is_running", &self.handle.is_some())
            .field("apis", &self.apis())
            .field("inputs", &self.inputs())
            .field("outputs", &self.outputs())
            .finish()
    }
}

impl AudioDeviceTrait for AudioDevice {
    fn new() -> AudioDeviceResult<Self> {
        let juce = JUCE::initialise();
        let mut device_manager = AudioDeviceManager::new(&juce);
        device_manager.initialise(256, 256)?;
        let mut apis = Vec::new();
        for api in device_manager.device_types() {
            apis.push(api.name());
        }
        Ok(Self {
            _juce: juce,
            apis,
            device_manager,
            handle: None,
        })
    }

    fn api(&self) -> String {
        let device_type = self.device_manager.current_device_type().unwrap();
        device_type.name()
    }

    fn apis(&self) -> Vec<String> {
        self.apis.clone()
    }

    fn input(&self) -> String {
        self.device_manager
            .audio_device_setup()
            .input_device_name()
            .into()
    }

    fn output(&self) -> String {
        self.device_manager
            .audio_device_setup()
            .output_device_name()
            .into()
    }

    fn inputs(&self) -> Vec<DeviceInfo> {
        let device_type = self.device_manager.current_device_type().unwrap();
        device_type
            .input_devices()
            .iter()
            .map(|d| DeviceInfo {
                name: d.clone(),
                num_channels: 0,
            })
            .collect()
    }

    fn outputs(&self) -> Vec<DeviceInfo> {
        let device_type = self.device_manager.current_device_type().unwrap();
        device_type
            .output_devices()
            .iter()
            .map(|d| DeviceInfo {
                name: d.clone(),
                num_channels: 0,
            })
            .collect()
    }

    fn set_api(&mut self, name: &str) -> AudioDeviceResult<()> {
        self.device_manager
            .set_current_audio_device_type(name, true);
        Ok(())
    }

    fn set_input(&mut self, input: &str) -> AudioDeviceResult<()> {
        // self.input_device = Host::new(self.api.clone())
        //     .unwrap()
        //     .iter_input_devices()
        //     .find(|device| device.name.contains(input))
        //     .ok_or(AudioDeviceError::NotAvailable)?
        //     .clone();
        Ok(())
    }

    fn set_output(&mut self, output: &str) -> AudioDeviceResult<()> {
        // self.output_device = Host::new(self.api.clone())
        //     .unwrap()
        //     .iter_output_devices()
        //     .find(|device| device.name.contains(output))
        //     .ok_or(AudioDeviceError::NotAvailable)?
        //     .clone();
        Ok(())
    }

    fn start(
        &mut self,
        config: Config,
        process_fn: impl FnMut(Block, BlockMut) + Send + 'static,
    ) -> AudioDeviceResult<()> {
        let mut setup = self.device_manager.audio_device_setup();
        setup = setup.with_input_channels(ChannelCount::Custom(config.num_input_channels as i32));
        setup = setup.with_output_channels(ChannelCount::Custom(config.num_output_channels as i32));
        setup = setup.with_sample_rate(config.sample_rate as f64);
        setup = setup.with_buffer_size(config.num_frames);

        self.device_manager.set_audio_device_setup(&setup, true)?;

        self.handle = Some(
            self.device_manager
                .add_audio_callback(AudioCallback::new(process_fn)),
        );

        Ok(())
    }

    fn stop(&mut self) -> AudioDeviceResult<()> {
        if let Some(handle) = self.handle.take() {
            self.device_manager.remove_audio_callback(handle);
        }
        Ok(())
    }
}

struct AudioCallback<F: FnMut(Block, BlockMut) + Send + 'static> {
    process_fn: F,
    input_block: AudioBlockInterleaved<f32>,
    output_block: AudioBlockInterleaved<f32>,
}

impl<F: FnMut(Block, BlockMut) + Send + 'static> AudioCallback<F> {
    pub fn new(process_fn: F) -> Self {
        Self {
            process_fn,
            input_block: AudioBlockInterleaved::new(0, 0),
            output_block: AudioBlockInterleaved::new(0, 0),
        }
    }
}

impl<F: FnMut(Block, BlockMut) + Send + 'static> AudioDeviceCallback for AudioCallback<F> {
    fn about_to_start(&mut self, device: &mut dyn cxx_juce::juce_audio_devices::AudioDevice) {
        let num_input_channels = device.input_channels() as u16;
        let num_output_channels = device.output_channels() as u16;
        let num_frames = device.buffer_size() as usize;
        self.input_block = AudioBlockInterleaved::new(num_input_channels, num_frames);
        self.output_block = AudioBlockInterleaved::new(num_output_channels, num_frames);
    }

    fn process_block(
        &mut self,
        input: &AudioSampleBuffer,
        mut output: Pin<&mut AudioSampleBuffer>,
    ) {
        // resize buffers
        self.input_block.set_active_size(
            input.get_num_channels() as u16,
            input.get_num_samples() as usize,
        );
        self.output_block.set_active_size(
            output.get_num_channels() as u16,
            output.get_num_samples() as usize,
        );

        // copy input
        for ch in 0..input.get_num_channels() {
            let channel = input.get_read_slice(ch);
            for frame in 0..input.get_num_samples() {
                *self.input_block.sample_mut(ch as u16, frame as usize) = channel[frame as usize];
            }
        }

        // user process
        (self.process_fn)(self.input_block.view(), self.output_block.view_mut());

        // copy output
        let num_samples = output.get_num_samples();
        for ch in 0..output.get_num_channels() {
            let channel = output.as_mut().get_write_slice(ch);
            for frame in 0..num_samples {
                channel[frame as usize] = self.output_block.sample(ch as u16, frame as usize);
            }
        }
    }

    fn stopped(&mut self) {}
}
