use crate::cache::{load_resampled_metadata, load_sample_data};
use crate::types::ResampleEvent;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

fn write_f32_interleaved(path: &Path, data: &[f32]) -> Result<(), String> {
    let mut bytes = Vec::with_capacity(data.len() * 4);
    for value in data {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    std::fs::write(path, bytes).map_err(|err| format!("Failed to write {:?}: {err}", path))
}

fn resample_interleaved_sinc(
    input: &[f32],
    channels: usize,
    input_rate: u32,
    output_rate: u32,
    points: usize,
    mut progress_cb: impl FnMut(f32),
) -> Vec<f32> {
    let input_frames = input.len() / channels;
    let output_frames = ((input_frames as f64) * (output_rate as f64) / (input_rate as f64))
        .round()
        .max(1.0) as usize;
    let taps = points.max(2);
    let half = taps as i32 / 2;
    let report_stride = std::cmp::max(1, output_frames / 100);

    let mut output = vec![0.0f32; output_frames * channels];
    let pi = std::f64::consts::PI;

    for out_idx in 0..output_frames {
        let pos = out_idx as f64 * (input_rate as f64) / (output_rate as f64);
        let base = pos.floor() as i32;
        let start = base - half + 1;

        for ch in 0..channels {
            let mut acc = 0.0f64;
            let mut norm = 0.0f64;

            for tap in 0..taps {
                let idx = start + tap as i32;
                if idx < 0 || idx >= input_frames as i32 {
                    continue;
                }
                let t = pos - idx as f64;
                let sinc = if t == 0.0 {
                    1.0
                } else {
                    (pi * t).sin() / (pi * t)
                };
                let window = if t.abs() <= half as f64 && half > 0 {
                    let frac = t / (half as f64);
                    0.5 * (1.0 + (pi * frac).cos())
                } else {
                    0.0
                };
                let weight = sinc * window;
                let sample = input[idx as usize * channels + ch] as f64;
                acc += sample * weight;
                norm += weight;
            }

            if norm != 0.0 {
                acc /= norm;
            }
            output[out_idx * channels + ch] = acc as f32;
        }

        if out_idx % report_stride == 0 {
            progress_cb(out_idx as f32 / output_frames as f32);
        }
    }

    progress_cb(1.0);
    output
}

pub fn queue_resample_event(
    events: &Arc<Mutex<Vec<ResampleEvent>>>,
    dirty: &Arc<AtomicBool>,
    event: ResampleEvent,
) {
    if let Ok(mut guard) = events.lock() {
        guard.push(event);
    }
    dirty.store(true, Ordering::Relaxed);
}

pub fn spawn_resample_task(
    cache_folder: PathBuf,
    target_sample_rate: u32,
    points: u32,
    resample_in_progress: Arc<AtomicBool>,
    events: Arc<Mutex<Vec<ResampleEvent>>>,
    events_dirty: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        let result = (|| -> Result<String, String> {
            if target_sample_rate == 0 {
                return Err("Project sample rate cannot be zero.".to_string());
            }
            if points == 0 {
                return Err("Resample points cannot be zero.".to_string());
            }

            queue_resample_event(
                &events,
                &events_dirty,
                ResampleEvent::Started {
                    label: format!("Resampling to {target_sample_rate} Hz ({points} pts)"),
                },
            );

            let (metadata, data) = load_sample_data(&cache_folder)?;

            if let Some(existing) = load_resampled_metadata(&cache_folder)? {
                if existing.sample_rate == target_sample_rate
                    && existing.points == points
                    && existing.source_sample_rate == metadata.sample_rate
                    && existing.source_frames == metadata.frames
                {
                    return Ok("Resample cache is already up to date.".to_string());
                }
            }

            let channels = metadata.channels as usize;
            let output = resample_interleaved_sinc(
                &data,
                channels,
                metadata.sample_rate,
                target_sample_rate,
                points as usize,
                |progress| {
                    queue_resample_event(
                        &events,
                        &events_dirty,
                        ResampleEvent::Progress { progress },
                    );
                },
            );

            let output_frames = (output.len() / channels) as u32;
            let array_path = cache_folder.join("resampled.array");
            write_f32_interleaved(&array_path, &output)?;

            let json_path = cache_folder.join("resampled.json");
            let metadata_json = json!({
                "name": metadata.name,
                "sample_rate": target_sample_rate,
                "channels": metadata.channels,
                "frames": output_frames,
                "length_seconds": output_frames as f32 / target_sample_rate as f32,
                "format": "f32le",
                "layout": "interleaved",
                "source_sample_rate": metadata.sample_rate,
                "source_frames": metadata.frames,
                "points": points,
            });
            let json_bytes = serde_json::to_vec_pretty(&metadata_json)
                .map_err(|err| format!("Failed to serialize resampled.json: {err}"))?;
            std::fs::write(&json_path, json_bytes)
                .map_err(|err| format!("Failed to write resampled.json: {err}"))?;

            Ok("Resample complete.".to_string())
        })();

        match result {
            Ok(message) => {
                queue_resample_event(
                    &events,
                    &events_dirty,
                    ResampleEvent::Completed { message },
                );
            }
            Err(message) => {
                queue_resample_event(
                    &events,
                    &events_dirty,
                    ResampleEvent::Error { message },
                );
            }
        }

        resample_in_progress.store(false, Ordering::Relaxed);
    });
}
