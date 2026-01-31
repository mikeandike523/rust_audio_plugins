use nih_plug::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Params)]
pub struct TunableSamplerParams {
    #[id = "gain"]
    pub gain: FloatParam,
    pub gain_changed: Arc<AtomicBool>,
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
            project_folder: Arc::new(Mutex::new(None)),
        }
    }
}
