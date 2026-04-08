use crate::cache::load_sample_data;
use crate::types::PitchEvent;
use pitch_detection::detector::mcleod::McLeodDetector;
use pitch_detection::detector::PitchDetector;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const DETECT_SIZE: usize = 4096;
const POWER_THRESHOLD: f64 = 5.0 / DETECT_SIZE as f64;
const CLARITY_THRESHOLD: f64 = 0.7;

pub fn spawn_pitch_estimate(
    sample_dir: PathBuf,
    sample_start: f32,
    pitch_in_progress: Arc<AtomicBool>,
    events: Arc<Mutex<Vec<PitchEvent>>>,
    events_dirty: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        let result = estimate(&sample_dir, sample_start);
        let event = match result {
            Ok(Some(hz)) => PitchEvent::Detected { hz },
            Ok(None) => PitchEvent::NoResult,
            Err(msg) => PitchEvent::Error { message: msg },
        };
        if let Ok(mut g) = events.lock() {
            g.push(event);
        }
        events_dirty.store(true, Ordering::Relaxed);
        pitch_in_progress.store(false, Ordering::Relaxed);
    });
}

fn estimate(sample_dir: &std::path::Path, sample_start: f32) -> Result<Option<f64>, String> {
    let (metadata, data) = load_sample_data(sample_dir)?;

    let channels = metadata.channels as usize;
    let total_frames = metadata.frames as usize;
    let start_frame = ((sample_start * total_frames as f32) as usize).min(total_frames.saturating_sub(1));
    let available = total_frames - start_frame;

    if available < 64 {
        return Ok(None);
    }

    // Build a mono f64 window of exactly DETECT_SIZE, zero-padded if needed.
    let window_frames = available.min(DETECT_SIZE);
    let mut signal = vec![0.0f64; DETECT_SIZE];
    for i in 0..window_frames {
        let frame = start_frame + i;
        let sum: f32 = (0..channels).map(|ch| data[frame * channels + ch]).sum();
        signal[i] = (sum / channels as f32) as f64;
    }

    let sample_rate = metadata.sample_rate as usize;
    let mut detector = McLeodDetector::<f64>::new(DETECT_SIZE, DETECT_SIZE / 2);
    let pitch = detector.get_pitch(&signal, sample_rate, POWER_THRESHOLD, CLARITY_THRESHOLD);
    Ok(pitch.map(|p| p.frequency))
}
