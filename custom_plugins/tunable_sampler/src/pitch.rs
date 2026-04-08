use crate::cache::load_sample_data;
use crate::remote_logging::RemoteLogger;
use crate::types::PitchEvent;
use pitch_detection::detector::mcleod::McLeodDetector;
use pitch_detection::detector::PitchDetector;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const DETECT_SIZE: usize = 4096;
const HOP_SIZE: usize = 1024;
const MAX_WINDOWS: usize = 8;
const POWER_THRESHOLD: f64 = 5.0 / DETECT_SIZE as f64;
const CLARITY_THRESHOLD: f64 = 0.55;

pub fn spawn_pitch_estimate(
    sample_dir: PathBuf,
    sample_start: f32,
    pitch_in_progress: Arc<AtomicBool>,
    events: Arc<Mutex<Vec<PitchEvent>>>,
    events_dirty: Arc<AtomicBool>,
    logger: RemoteLogger,
) {
    std::thread::spawn(move || {
        logger.log_step(
            "pitch_thread_start",
            format!("sample_start={sample_start:.4} path={}", sample_dir.display()),
        );
        let result = estimate(&sample_dir, sample_start, &logger);
        let event = match result {
            Ok(Some(hz)) => PitchEvent::Detected { hz },
            Ok(None) => PitchEvent::NoResult,
            Err(msg) => PitchEvent::Error { message: msg },
        };
        match &event {
            PitchEvent::Detected { hz } => {
                logger.log_step("pitch_thread_detected", format!("hz={hz:.3}"));
            }
            PitchEvent::NoResult => {
                logger.log_step("pitch_thread_no_result", "no pitch detected".to_string());
            }
            PitchEvent::Error { message } => {
                logger.log_step("pitch_thread_error", message.clone());
            }
        }
        if let Ok(mut g) = events.lock() {
            g.push(event);
        }
        events_dirty.store(true, Ordering::Relaxed);
        pitch_in_progress.store(false, Ordering::Relaxed);
    });
}

fn estimate(
    sample_dir: &std::path::Path,
    sample_start: f32,
    logger: &RemoteLogger,
) -> Result<Option<f64>, String> {
    let (metadata, data) = load_sample_data(sample_dir)?;

    let channels = metadata.channels as usize;
    let total_frames = metadata.frames as usize;
    if total_frames == 0 || channels == 0 {
        return Ok(None);
    }

    let start_frame = ((sample_start * total_frames as f32) as usize).min(total_frames.saturating_sub(1));
    let available = total_frames - start_frame;

    if available < 64 {
        logger.log_step(
            "pitch_too_short",
            format!("start_frame={start_frame} available={available} total_frames={total_frames}"),
        );
        return Ok(None);
    }

    let sample_rate = metadata.sample_rate as usize;
    logger.log_step(
        "pitch_estimate_begin",
        format!(
            "sample_rate={} channels={} total_frames={} start_frame={} available={}",
            sample_rate, channels, total_frames, start_frame, available
        ),
    );

    for window_index in 0..MAX_WINDOWS {
        let offset = window_index * HOP_SIZE;
        if start_frame + offset >= total_frames {
            break;
        }

        let window_start = start_frame + offset;
        let window_frames = (total_frames - window_start).min(DETECT_SIZE);
        if window_frames < 64 {
            break;
        }

        let mut signal = vec![0.0f64; DETECT_SIZE];
        let mut peak = 0.0f32;
        let mut rms_acc = 0.0f64;
        for i in 0..window_frames {
            let frame = window_start + i;
            let sum: f32 = (0..channels).map(|ch| data[frame * channels + ch]).sum();
            let mono = sum / channels as f32;
            peak = peak.max(mono.abs());
            rms_acc += (mono as f64) * (mono as f64);
            signal[i] = mono as f64;
        }
        let rms = (rms_acc / window_frames as f64).sqrt();

        logger.log_step(
            "pitch_window",
            format!(
                "index={} start={} frames={} peak={:.6} rms={:.6}",
                window_index, window_start, window_frames, peak, rms
            ),
        );

        let mut detector = McLeodDetector::<f64>::new(DETECT_SIZE, DETECT_SIZE / 2);
        if let Some(pitch) =
            detector.get_pitch(&signal, sample_rate, POWER_THRESHOLD, CLARITY_THRESHOLD)
        {
            logger.log_step(
                "pitch_window_hit",
                format!(
                    "index={} hz={:.3} clarity={:.4}",
                    window_index, pitch.frequency, pitch.clarity
                ),
            );
            return Ok(Some(pitch.frequency));
        }
    }

    Ok(None)
}
