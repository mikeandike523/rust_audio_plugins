use rubato::{Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction};

use crate::cache::{load_resampled_metadata, load_sample_data};
use crate::types::ResampleEvent;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Quality preset for sinc resampling via rubato.
///
/// Each preset maps to a (sinc_len, oversampling_factor) pair that mirrors the
/// quantised phase-bank approach used in real-time resamplers.  Higher presets
/// use longer kernels and more phase banks, trading CPU/memory for fidelity.
///
/// | Preset    | sinc_len | oversampling_factor (phase banks) |
/// |-----------|----------|-----------------------------------|
/// | Normal    | 32       | 64                                |
/// | High      | 64       | 128                               |
/// | UltraHigh | 128      | 256                               |
#[derive(Copy, Clone)]
pub enum ResampleQuality {
    Normal,
    High,
    UltraHigh,
}

impl ResampleQuality {
    pub fn from_u32(v: u32) -> Self {
        match v {
            0 => ResampleQuality::Normal,
            2 => ResampleQuality::UltraHigh,
            _ => ResampleQuality::High,
        }
    }

    pub fn as_u32(self) -> u32 {
        match self {
            ResampleQuality::Normal => 0,
            ResampleQuality::High => 1,
            ResampleQuality::UltraHigh => 2,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ResampleQuality::Normal => "Normal",
            ResampleQuality::High => "High",
            ResampleQuality::UltraHigh => "Ultra High",
        }
    }

    fn sinc_len(self) -> usize {
        match self {
            ResampleQuality::Normal => 32,
            ResampleQuality::High => 64,
            ResampleQuality::UltraHigh => 128,
        }
    }

    fn oversampling_factor(self) -> usize {
        match self {
            ResampleQuality::Normal => 64,
            ResampleQuality::High => 128,
            ResampleQuality::UltraHigh => 256,
        }
    }
}

fn write_f32_interleaved(path: &Path, data: &[f32]) -> Result<(), String> {
    let mut bytes = Vec::with_capacity(data.len() * 4);
    for value in data {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    std::fs::write(path, bytes).map_err(|err| format!("Failed to write {:?}: {err}", path))
}

/// Resample `input` (interleaved f32) using rubato's windowed-sinc resampler.
///
/// Linear interpolation between phase banks is used — this matches the style of
/// real-time phase-bank resamplers while still being high quality offline.
fn resample_rubato(
    input: &[f32],
    channels: usize,
    input_rate: u32,
    output_rate: u32,
    quality: ResampleQuality,
    mut progress_cb: impl FnMut(f32),
) -> Result<Vec<f32>, String> {
    if input_rate == output_rate {
        progress_cb(1.0);
        return Ok(input.to_vec());
    }

    let ratio = output_rate as f64 / input_rate as f64;
    let input_frames = input.len() / channels;
    let expected_output_frames = (input_frames as f64 * ratio).round().max(1.0) as usize;

    let params = SincInterpolationParameters {
        sinc_len: quality.sinc_len(),
        f_cutoff: 0.95,
        oversampling_factor: quality.oversampling_factor(),
        interpolation: SincInterpolationType::Linear,
        window: WindowFunction::BlackmanHarris2,
    };

    const CHUNK_SIZE: usize = 4096;

    let mut resampler = SincFixedIn::<f32>::new(ratio, 2.0, params, CHUNK_SIZE, channels)
        .map_err(|e| e.to_string())?;

    // De-interleave into per-channel Vecs.
    let ch_in: Vec<Vec<f32>> = (0..channels)
        .map(|ch| (0..input_frames).map(|i| input[i * channels + ch]).collect())
        .collect();

    let mut ch_out: Vec<Vec<f32>> = (0..channels)
        .map(|_| Vec::with_capacity(expected_output_frames + CHUNK_SIZE))
        .collect();

    let mut pos = 0_usize;
    while pos < input_frames {
        let end = (pos + CHUNK_SIZE).min(input_frames);
        let frames_this_chunk = end - pos;

        // Build a CHUNK_SIZE-padded chunk for each channel.
        let chunk_in: Vec<Vec<f32>> = (0..channels)
            .map(|ch| {
                let mut v = ch_in[ch][pos..end].to_vec();
                v.resize(CHUNK_SIZE, 0.0);
                v
            })
            .collect();

        let out = resampler.process(&chunk_in, None).map_err(|e| e.to_string())?;

        for (ch, ch_data) in out.iter().enumerate() {
            ch_out[ch].extend_from_slice(ch_data);
        }

        pos += frames_this_chunk;
        progress_cb(pos as f32 / input_frames as f32);
    }

    // Trim to expected length (padding the last chunk may produce a few extra frames).
    let actual_out_frames = ch_out[0].len().min(expected_output_frames);

    // Re-interleave.
    let mut output = vec![0.0f32; actual_out_frames * channels];
    for (ch, ch_data) in ch_out.iter().enumerate() {
        for (i, &sample) in ch_data.iter().take(actual_out_frames).enumerate() {
            output[i * channels + ch] = sample;
        }
    }

    progress_cb(1.0);
    Ok(output)
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
    quality: u32,
    force: bool,
    resample_in_progress: Arc<AtomicBool>,
    events: Arc<Mutex<Vec<ResampleEvent>>>,
    events_dirty: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        let quality = ResampleQuality::from_u32(quality);

        let result = (|| -> Result<String, String> {
            if target_sample_rate == 0 {
                return Err("Project sample rate cannot be zero.".to_string());
            }

            queue_resample_event(
                &events,
                &events_dirty,
                ResampleEvent::Started {
                    label: format!(
                        "Resampling to {target_sample_rate} Hz ({})",
                        quality.label()
                    ),
                },
            );

            let (metadata, data) = load_sample_data(&cache_folder)?;

            // Skip if the cached resample is still valid (unless the caller forced a fresh run).
            if !force {
                if let Some(existing) = load_resampled_metadata(&cache_folder)? {
                    if existing.sample_rate == target_sample_rate
                        && existing.quality == quality.as_u32()
                        && existing.source_sample_rate == metadata.sample_rate
                        && existing.source_frames == metadata.frames
                    {
                        return Ok("Resample cache is already up to date.".to_string());
                    }
                }
            }

            let channels = metadata.channels as usize;
            let output = resample_rubato(
                &data,
                channels,
                metadata.sample_rate,
                target_sample_rate,
                quality,
                |progress| {
                    queue_resample_event(
                        &events,
                        &events_dirty,
                        ResampleEvent::Progress { progress },
                    );
                },
            )?;

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
                "quality": quality.as_u32(),
            });
            let json_bytes = serde_json::to_vec_pretty(&metadata_json)
                .map_err(|err| format!("Failed to serialize resampled.json: {err}"))?;
            std::fs::write(&json_path, json_bytes)
                .map_err(|err| format!("Failed to write resampled.json: {err}"))?;

            Ok("Resample complete.".to_string())
        })();

        match result {
            Ok(message) => {
                queue_resample_event(&events, &events_dirty, ResampleEvent::Completed { message });
            }
            Err(message) => {
                queue_resample_event(&events, &events_dirty, ResampleEvent::Error { message });
            }
        }

        resample_in_progress.store(false, Ordering::Relaxed);
    });
}
