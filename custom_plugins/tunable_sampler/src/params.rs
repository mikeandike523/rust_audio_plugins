use nih_plug::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Params)]
pub struct TunableSamplerParams {
    #[id = "gain"]
    pub gain: FloatParam,
    pub gain_changed: Arc<AtomicBool>,
    #[id = "sample_start"]
    pub sample_start: FloatParam,
    pub sample_start_changed: Arc<AtomicBool>,
    #[id = "sample_end"]
    pub sample_end: FloatParam,
    pub sample_end_changed: Arc<AtomicBool>,
    #[persist = "project_folder"]
    pub project_folder: Arc<Mutex<Option<String>>>,
}

impl Default for TunableSamplerParams {
    fn default() -> Self {
        let gain_changed = Arc::new(AtomicBool::new(false));
        let gain_changed_cb = gain_changed.clone();
        let gain_callback = Arc::new(move |_: f32| {
            gain_changed_cb.store(true, Ordering::Relaxed);
        });
        let sample_start_changed = Arc::new(AtomicBool::new(false));
        let sample_start_changed_cb = sample_start_changed.clone();
        let sample_start_callback = Arc::new(move |_: f32| {
            sample_start_changed_cb.store(true, Ordering::Relaxed);
        });
        let sample_end_changed = Arc::new(AtomicBool::new(false));
        let sample_end_changed_cb = sample_end_changed.clone();
        let sample_end_callback = Arc::new(move |_: f32| {
            sample_end_changed_cb.store(true, Ordering::Relaxed);
        });

        Self {
            gain: FloatParam::new(
                "Gain",
                0.0,
                FloatRange::Linear {
                    min: -24.0,
                    max: 24.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(5.0))
            .with_step_size(0.1)
            .with_unit(" dB")
            .with_callback(gain_callback),
            gain_changed,
            sample_start: FloatParam::new(
                "Sample Start",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_step_size(0.001)
            .with_callback(sample_start_callback),
            sample_start_changed,
            sample_end: FloatParam::new(
                "Sample End",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_step_size(0.001)
            .with_callback(sample_end_callback),
            sample_end_changed,
            project_folder: Arc::new(Mutex::new(None)),
        }
    }
}
