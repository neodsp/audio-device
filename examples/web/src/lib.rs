use std::cell::RefCell;

use audio_device::web_audio::WebAudio;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::{HtmlButtonElement, HtmlSelectElement, console, window};

#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    console::log_1(&"WASM loaded".into());
}

#[wasm_bindgen]
pub fn request_permission() {
    spawn_local(async {
        match WebAudio::request_permission().await {
            Ok(_) => {
                console::log_1(&"Permission granted!".into());
                // Now enumerate devices with full labels
                populate_devices().await;
            }
            Err(e) => {
                console::error_1(&format!("Permission denied: {}", e).into());
            }
        }
    });
}

async fn populate_devices() {
    let window = window().unwrap();
    let document = window.document().unwrap();

    match WebAudio::enumerate_devices().await {
        Ok((inputs, outputs)) => {
            // Populate input select
            let input_select: HtmlSelectElement = document
                .get_element_by_id("input-device")
                .unwrap()
                .dyn_into()
                .unwrap();

            // Clear existing options
            input_select.set_inner_html("");

            for device in &inputs {
                let option = document.create_element("option").unwrap();
                option.set_attribute("value", &device.device_id).unwrap();
                option.set_text_content(Some(if device.label.is_empty() {
                    &device.device_id
                } else {
                    &device.label
                }));
                input_select.append_child(&option).unwrap();
            }

            // Populate output select
            let output_select: HtmlSelectElement = document
                .get_element_by_id("output-device")
                .unwrap()
                .dyn_into()
                .unwrap();

            // Clear existing options
            output_select.set_inner_html("");

            for device in &outputs {
                let option = document.create_element("option").unwrap();
                option.set_attribute("value", &device.device_id).unwrap();
                option.set_text_content(Some(if device.label.is_empty() {
                    &device.device_id
                } else {
                    &device.label
                }));
                output_select.append_child(&option).unwrap();
            }

            console::log_1(
                &format!("Found {} inputs, {} outputs", inputs.len(), outputs.len()).into(),
            );
        }
        Err(e) => {
            console::error_1(&format!("Failed to enumerate devices: {}", e).into());
        }
    }
}

// Store the audio instance globally so we can stop it
thread_local! {
    static AUDIO: RefCell<Option<WebAudio>> = RefCell::new(None);
}

#[wasm_bindgen]
pub fn start_audio() {
    let window = window().unwrap();
    let document = window.document().unwrap();

    // Get selected devices
    let input_select: HtmlSelectElement = document
        .get_element_by_id("input-device")
        .unwrap()
        .dyn_into()
        .unwrap();
    let output_select: HtmlSelectElement = document
        .get_element_by_id("output-device")
        .unwrap()
        .dyn_into()
        .unwrap();

    let input_device_id = input_select.value();
    let output_device_id = output_select.value();

    console::log_1(
        &format!(
            "Starting with input: {}, output: {}",
            input_device_id, output_device_id
        )
        .into(),
    );

    spawn_local(async move {
        // Create audio instance
        let mut audio = match WebAudio::new().await {
            Ok(a) => a,
            Err(e) => {
                console::error_1(&format!("Failed to create WebAudio: {}", e).into());
                return;
            }
        };

        console::log_1(&format!("Sample rate: {} Hz", audio.sample_rate()).into());

        // Set devices
        if !input_device_id.is_empty() {
            audio.set_input_device(Some(input_device_id));
        }
        if !output_device_id.is_empty() {
            if let Err(e) = audio.set_output_device(Some(output_device_id)).await {
                console::warn_1(&format!("Could not set output device: {}", e).into());
            }
        }

        // Start with feedback loop (echo input to output)
        match audio
            .start(1024, 2, |input, output| {
                // Simple feedback: copy input to output
                output.copy_from_slice(input);
            })
            .await
        {
            Ok(_) => {
                console::log_1(&"Audio started! You should hear yourself.".into());

                // Update UI
                update_button_state(true);

                // Store audio instance
                AUDIO.with(|a| {
                    *a.borrow_mut() = Some(audio);
                });
            }
            Err(e) => {
                console::error_1(&format!("Failed to start audio: {}", e).into());
            }
        }
    });
}

#[wasm_bindgen]
pub fn stop_audio() {
    AUDIO.with(|a| {
        if let Some(mut audio) = a.borrow_mut().take() {
            match audio.stop() {
                Ok(_) => {
                    console::log_1(&"Audio stopped".into());
                    update_button_state(false);
                }
                Err(e) => {
                    console::error_1(&format!("Failed to stop audio: {}", e).into());
                }
            }
        }
    });
}

fn update_button_state(is_running: bool) {
    let window = window().unwrap();
    let document = window.document().unwrap();

    let start_btn: HtmlButtonElement = document
        .get_element_by_id("start-btn")
        .unwrap()
        .dyn_into()
        .unwrap();
    let stop_btn: HtmlButtonElement = document
        .get_element_by_id("stop-btn")
        .unwrap()
        .dyn_into()
        .unwrap();

    start_btn.set_disabled(is_running);
    stop_btn.set_disabled(!is_running);
}
