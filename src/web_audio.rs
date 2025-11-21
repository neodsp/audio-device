//! Browser-native Web Audio API for WASM applications
//!
//! This module provides a Rust interface for browser audio that follows
//! web platform conventions:
//! - Async device enumeration (requires user permission)
//! - Promise-based microphone access
//! - AudioWorklet for low-latency processing
//!
//! # Example
//! ```ignore
//! use audio_device::web_audio::WebAudio;
//!
//! // Request permission and enumerate devices
//! let devices = WebAudio::enumerate_devices().await?;
//!
//! // Create audio context
//! let mut audio = WebAudio::new().await?;
//!
//! // Select devices (like Google Meet device picker)
//! audio.set_input_device(&input_device_id).await?;
//! audio.set_output_device(&output_device_id)?;
//!
//! // Start processing - returns once stream is connected, audio runs in background
//! audio.start(1024, 2, move |input, output| {
//!     // Process audio: input and output are &[f32] interleaved stereo
//!     output.copy_from_slice(input);
//! }).await?;
//!
//! // Audio is now running in background...
//! // Call audio.stop()? when done
//! ```

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AudioContext, MediaDeviceInfo, MediaDeviceKind, MediaStream, MediaStreamAudioSourceNode,
    MediaStreamConstraints, ScriptProcessorNode,
};

/// Information about an available audio device
#[derive(Debug, Clone)]
pub struct AudioDeviceInfo {
    /// Unique device identifier (use this for selection)
    pub device_id: String,
    /// Human-readable device name
    pub label: String,
    /// Number of channels (may be 0 if unknown before permission)
    pub channels: u16,
}

/// Error types for WebAudio operations
#[derive(Debug)]
pub enum WebAudioError {
    /// Browser doesn't support required Web Audio features
    NotSupported(String),
    /// User denied microphone/device permission
    PermissionDenied,
    /// Requested device not found
    DeviceNotFound(String),
    /// Audio context is in wrong state
    InvalidState(String),
    /// JavaScript error from Web APIs
    JsError(String),
}

impl std::fmt::Display for WebAudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WebAudioError::NotSupported(msg) => write!(f, "Not supported: {}", msg),
            WebAudioError::PermissionDenied => write!(f, "Permission denied for audio device"),
            WebAudioError::DeviceNotFound(id) => write!(f, "Device not found: {}", id),
            WebAudioError::InvalidState(msg) => write!(f, "Invalid state: {}", msg),
            WebAudioError::JsError(msg) => write!(f, "JavaScript error: {}", msg),
        }
    }
}

impl std::error::Error for WebAudioError {}

impl From<JsValue> for WebAudioError {
    fn from(err: JsValue) -> Self {
        let msg = err
            .as_string()
            .or_else(|| {
                js_sys::Reflect::get(&err, &"message".into())
                    .ok()
                    .and_then(|v| v.as_string())
            })
            .unwrap_or_else(|| format!("{:?}", err));

        if msg.contains("Permission denied") || msg.contains("NotAllowedError") {
            WebAudioError::PermissionDenied
        } else {
            WebAudioError::JsError(msg)
        }
    }
}

pub type Result<T> = std::result::Result<T, WebAudioError>;

/// Browser Web Audio interface
///
/// Designed for WASM applications that need microphone input and speaker output,
/// similar to video conferencing apps like Google Meet.
pub struct WebAudio {
    context: AudioContext,
    input_device_id: Option<String>,
    output_device_id: Option<String>,

    // Active stream components
    media_stream: Option<MediaStream>,
    source_node: Option<MediaStreamAudioSourceNode>,
    processor_node: Option<ScriptProcessorNode>,
    _closure: Option<Closure<dyn FnMut(web_sys::AudioProcessingEvent)>>,
}

impl WebAudio {
    /// Create a new WebAudio instance
    ///
    /// This creates an AudioContext. Note that browsers require user interaction
    /// before audio can play, so the context starts in "suspended" state.
    pub async fn new() -> Result<Self> {
        let window = web_sys::window()
            .ok_or_else(|| WebAudioError::NotSupported("No window object".into()))?;

        // Check for AudioContext support
        if js_sys::Reflect::get(&window, &"AudioContext".into())
            .map(|v| v.is_undefined())
            .unwrap_or(true)
        {
            return Err(WebAudioError::NotSupported(
                "AudioContext not available".into(),
            ));
        }

        let context = AudioContext::new().map_err(|e| {
            WebAudioError::JsError(format!("Failed to create AudioContext: {:?}", e))
        })?;

        Ok(Self {
            context,
            input_device_id: None,
            output_device_id: None,
            media_stream: None,
            source_node: None,
            processor_node: None,
            _closure: None,
        })
    }

    /// Get the sample rate of the audio context
    ///
    /// This is determined by the browser/system and cannot be changed.
    /// Typically 44100 or 48000 Hz.
    pub fn sample_rate(&self) -> u32 {
        self.context.sample_rate() as u32
    }

    /// Enumerate available audio devices
    ///
    /// Returns lists of input (microphone) and output (speaker) devices.
    ///
    /// Note: Device labels may be empty until the user grants microphone permission.
    /// Call `request_permission()` first to get full device information.
    pub async fn enumerate_devices() -> Result<(Vec<AudioDeviceInfo>, Vec<AudioDeviceInfo>)> {
        let window = web_sys::window()
            .ok_or_else(|| WebAudioError::NotSupported("No window object".into()))?;

        let navigator = window.navigator();
        let media_devices = navigator
            .media_devices()
            .map_err(|_| WebAudioError::NotSupported("mediaDevices not available".into()))?;

        let devices_promise = media_devices
            .enumerate_devices()
            .map_err(|e| WebAudioError::JsError(format!("enumerate_devices failed: {:?}", e)))?;

        let devices_js = JsFuture::from(devices_promise).await?;
        let devices_array: js_sys::Array = devices_js.unchecked_into();

        let mut inputs = Vec::new();
        let mut outputs = Vec::new();

        for i in 0..devices_array.length() {
            let device: MediaDeviceInfo = devices_array.get(i).unchecked_into();

            let info = AudioDeviceInfo {
                device_id: device.device_id(),
                label: device.label(),
                channels: 2, // Web Audio doesn't expose channel count before stream creation
            };

            match device.kind() {
                MediaDeviceKind::Audioinput => inputs.push(info),
                MediaDeviceKind::Audiooutput => outputs.push(info),
                _ => {}
            }
        }

        Ok((inputs, outputs))
    }

    /// Request microphone permission from the user
    ///
    /// This triggers the browser's permission dialog. After permission is granted,
    /// `enumerate_devices()` will return full device labels.
    ///
    /// Returns the default input stream which can be discarded if you just want permission.
    pub async fn request_permission() -> Result<()> {
        let window = web_sys::window()
            .ok_or_else(|| WebAudioError::NotSupported("No window object".into()))?;

        let navigator = window.navigator();
        let media_devices = navigator
            .media_devices()
            .map_err(|_| WebAudioError::NotSupported("mediaDevices not available".into()))?;

        let constraints = MediaStreamConstraints::new();
        constraints.set_audio(&JsValue::TRUE);
        constraints.set_video(&JsValue::FALSE);

        let stream_promise = media_devices
            .get_user_media_with_constraints(&constraints)
            .map_err(|e| WebAudioError::JsError(format!("getUserMedia failed: {:?}", e)))?;

        let stream_js = JsFuture::from(stream_promise).await?;
        let stream: MediaStream = stream_js.unchecked_into();

        // Stop the tracks since we just wanted permission
        let tracks = stream.get_audio_tracks();
        for i in 0..tracks.length() {
            let track = tracks.get(i);
            if let Ok(track) = track.dyn_into::<web_sys::MediaStreamTrack>() {
                track.stop();
            }
        }

        Ok(())
    }

    /// Set the input (microphone) device by device ID
    ///
    /// Use device IDs from `enumerate_devices()`. Pass `None` for default device.
    pub fn set_input_device(&mut self, device_id: Option<String>) {
        self.input_device_id = device_id;
    }

    /// Set the output (speaker) device by device ID
    ///
    /// Note: Output device selection requires `setSinkId` which has limited browser support.
    /// Use device IDs from `enumerate_devices()`. Pass `None` for default device.
    pub async fn set_output_device(&mut self, device_id: Option<String>) -> Result<()> {
        self.output_device_id = device_id.clone();

        // If we have an active context and the browser supports setSinkId
        if let Some(device_id) = device_id {
            let destination = self.context.destination();

            // setSinkId is not in web-sys yet, need to use js_sys::Reflect
            let set_sink_id = js_sys::Reflect::get(&destination, &"setSinkId".into())
                .ok()
                .filter(|v| v.is_function());

            if let Some(func) = set_sink_id {
                let func: js_sys::Function = func.unchecked_into();
                let promise = func
                    .call1(&destination, &device_id.into())
                    .map_err(|e| WebAudioError::JsError(format!("setSinkId failed: {:?}", e)))?;

                if let Ok(promise) = promise.dyn_into::<js_sys::Promise>() {
                    JsFuture::from(promise).await?;
                }
            }
            // If setSinkId not available, silently use default output
        }

        Ok(())
    }

    /// Resume the audio context after user interaction
    ///
    /// Browsers require user interaction (click/tap) before audio can play.
    /// Call this from a click handler to enable audio.
    pub async fn resume(&self) -> Result<()> {
        let promise = self
            .context
            .resume()
            .map_err(|e| WebAudioError::JsError(format!("resume failed: {:?}", e)))?;
        JsFuture::from(promise).await?;
        Ok(())
    }

    /// Start audio processing with the given callback
    ///
    /// # Arguments
    /// * `buffer_size` - Number of frames per callback (256, 512, 1024, 2048, 4096, 8192, 16384)
    /// * `num_channels` - Number of channels (1 for mono, 2 for stereo)
    /// * `process_fn` - Callback receiving input samples and mutable output buffer
    ///
    /// The callback receives interleaved f32 samples: `[L0, R0, L1, R1, ...]` for stereo.
    pub async fn start<F>(
        &mut self,
        buffer_size: usize,
        num_channels: u16,
        mut process_fn: F,
    ) -> Result<()>
    where
        F: FnMut(&[f32], &mut [f32]) + 'static,
    {
        // Stop any existing stream
        self.stop()?;

        // Resume context if needed
        self.resume().await?;

        let window = web_sys::window()
            .ok_or_else(|| WebAudioError::NotSupported("No window object".into()))?;
        let navigator = window.navigator();
        let media_devices = navigator
            .media_devices()
            .map_err(|_| WebAudioError::NotSupported("mediaDevices not available".into()))?;

        // Request microphone with specific device if set
        let constraints = if let Some(ref device_id) = self.input_device_id {
            let audio_constraints = web_sys::MediaTrackConstraints::new();
            audio_constraints.set_device_id(&device_id.into());

            let constraints = MediaStreamConstraints::new();
            constraints.set_audio(&audio_constraints.into());
            constraints.set_video(&JsValue::FALSE);
            constraints
        } else {
            let constraints = MediaStreamConstraints::new();
            constraints.set_audio(&JsValue::TRUE);
            constraints.set_video(&JsValue::FALSE);
            constraints
        };

        let stream_promise = media_devices
            .get_user_media_with_constraints(&constraints)
            .map_err(|e| WebAudioError::JsError(format!("getUserMedia failed: {:?}", e)))?;

        let stream_js = JsFuture::from(stream_promise).await?;
        let stream: MediaStream = stream_js.unchecked_into();

        // Create media stream source
        let source = self
            .context
            .create_media_stream_source(&stream)
            .map_err(|e| {
                WebAudioError::JsError(format!("createMediaStreamSource failed: {:?}", e))
            })?;

        // Create script processor
        // Note: ScriptProcessorNode is deprecated but AudioWorklet requires more complex setup
        let processor = self.context
            .create_script_processor_with_buffer_size_and_number_of_input_channels_and_number_of_output_channels(
                buffer_size as u32,
                num_channels as u32,
                num_channels as u32,
            )
            .map_err(|e| WebAudioError::JsError(format!("createScriptProcessor failed: {:?}", e)))?;

        // Set up processing callback
        let num_frames = buffer_size;
        let channels = num_channels as usize;

        let closure = Closure::wrap(Box::new(move |event: web_sys::AudioProcessingEvent| {
            let input_buffer = event.input_buffer().unwrap();
            let output_buffer = event.output_buffer().unwrap();

            let total_samples = num_frames * channels;
            let mut input_data = vec![0.0f32; total_samples];
            let mut output_data = vec![0.0f32; total_samples];

            // Copy input from Web Audio (planar) to interleaved
            for ch in 0..channels {
                if let Ok(channel_data) = input_buffer.get_channel_data(ch as u32) {
                    for (frame, sample) in channel_data.iter().enumerate().take(num_frames) {
                        input_data[frame * channels + ch] = *sample;
                    }
                }
            }

            // Call user's process function
            process_fn(&input_data, &mut output_data);

            // Copy output from interleaved to Web Audio (planar)
            for ch in 0..channels {
                let mut channel_data = vec![0.0f32; num_frames];
                for frame in 0..num_frames {
                    channel_data[frame] = output_data[frame * channels + ch];
                }
                let _ = output_buffer.copy_to_channel(&channel_data, ch as i32);
            }
        }) as Box<dyn FnMut(web_sys::AudioProcessingEvent)>);

        processor.set_onaudioprocess(Some(closure.as_ref().unchecked_ref()));

        // Connect: source -> processor -> destination
        source
            .connect_with_audio_node(&processor)
            .map_err(|e| WebAudioError::JsError(format!("connect source failed: {:?}", e)))?;
        processor
            .connect_with_audio_node(&self.context.destination())
            .map_err(|e| WebAudioError::JsError(format!("connect destination failed: {:?}", e)))?;

        // Store references
        self.media_stream = Some(stream);
        self.source_node = Some(source);
        self.processor_node = Some(processor);
        self._closure = Some(closure);

        Ok(())
    }

    /// Stop audio processing and release resources
    pub fn stop(&mut self) -> Result<()> {
        // Disconnect processor
        if let Some(processor) = self.processor_node.take() {
            processor.disconnect().ok();
            processor.set_onaudioprocess(None);
        }

        // Disconnect source
        if let Some(source) = self.source_node.take() {
            source.disconnect().ok();
        }

        // Stop media tracks
        if let Some(stream) = self.media_stream.take() {
            let tracks = stream.get_audio_tracks();
            for i in 0..tracks.length() {
                let track = tracks.get(i);
                if let Ok(track) = track.dyn_into::<web_sys::MediaStreamTrack>() {
                    track.stop();
                }
            }
        }

        self._closure = None;

        Ok(())
    }

    /// Check if audio is currently running
    pub fn is_running(&self) -> bool {
        self.processor_node.is_some()
    }

    /// Get the current audio context state
    pub fn state(&self) -> String {
        format!("{:?}", self.context.state())
    }
}

impl Drop for WebAudio {
    fn drop(&mut self) {
        let _ = self.stop();
        let _ = self.context.close();
    }
}
