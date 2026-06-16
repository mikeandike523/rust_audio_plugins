use nih_plug::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use crate::tuning::TuningFile;

#[derive(Params)]
pub struct TunableSamplerParams {
    #[id = "preamp_l"]
    pub preamp_l: FloatParam,
    pub preamp_l_changed: Arc<AtomicBool>,
    #[id = "preamp_r"]
    pub preamp_r: FloatParam,
    pub preamp_r_changed: Arc<AtomicBool>,
    /// Channel routing mode: 0=Stereo, 1=MixMean, 2=Left, 3=Right.
    #[persist = "channel_mode"]
    pub channel_mode: Arc<Mutex<u32>>,
    /// Legacy global preamp, retained for backward compatibility with projects saved
    /// before preamp was split into L/R. Applied equally to both channels as a
    /// post-routing scalar. Defaults to 0 dB (no effect); the UI only surfaces it when
    /// a non-default value was loaded from an old project.
    #[id = "preamp"]
    pub preamp: FloatParam,
    pub preamp_changed: Arc<AtomicBool>,
    #[id = "gain"]
    pub gain: FloatParam,
    pub gain_changed: Arc<AtomicBool>,
    #[id = "sample_start"]
    pub sample_start: FloatParam,
    pub sample_start_changed: Arc<AtomicBool>,
    #[id = "sample_end"]
    pub sample_end: FloatParam,
    pub sample_end_changed: Arc<AtomicBool>,
    /// Fine-tune offset in cents (−100 to +100). Not yet wired to DSP.
    #[id = "detune"]
    pub detune: FloatParam,
    pub detune_changed: Arc<AtomicBool>,
    #[id = "attack"]
    pub attack: FloatParam,
    pub attack_changed: Arc<AtomicBool>,
    #[id = "decay"]
    pub decay: FloatParam,
    pub decay_changed: Arc<AtomicBool>,
    #[id = "sustain"]
    pub sustain: FloatParam,
    pub sustain_changed: Arc<AtomicBool>,
    #[id = "release"]
    pub release: FloatParam,
    pub release_changed: Arc<AtomicBool>,
    #[id = "bend_depth"]
    pub bend_depth: FloatParam,
    pub bend_depth_changed: Arc<AtomicBool>,
    #[persist = "polyphony"]
    pub polyphony: Arc<Mutex<u32>>,
    #[persist = "nudge_to_12edo"]
    pub nudge_to_12edo: Arc<Mutex<bool>>,
    #[persist = "reference_frequency_hz"]
    pub reference_frequency_hz: Arc<Mutex<Option<f32>>>,
    #[persist = "detected_pitch_hz"]
    pub detected_pitch_hz: Arc<Mutex<Option<f32>>>,
    #[persist = "scl_file"]
    pub scl_file: Arc<Mutex<Option<TuningFile>>>,
    #[persist = "kbm_file"]
    pub kbm_file: Arc<Mutex<Option<TuningFile>>>,
    /// Optional custom cache directory override. None = use platform default.
    #[persist = "cache_dir"]
    pub cache_dir: Arc<Mutex<Option<String>>>,
    /// Unique ID for this instance's sample data subfolder inside the cache dir.
    /// None = no sample has been loaded yet.
    #[persist = "sample_uuid"]
    pub sample_uuid: Arc<Mutex<Option<String>>>,
}

impl Default for TunableSamplerParams {
    fn default() -> Self {
        let preamp_l_changed = Arc::new(AtomicBool::new(false));
        let preamp_l_changed_cb = preamp_l_changed.clone();
        let preamp_l_callback = Arc::new(move |_: f32| {
            preamp_l_changed_cb.store(true, Ordering::Relaxed);
        });
        let preamp_r_changed = Arc::new(AtomicBool::new(false));
        let preamp_r_changed_cb = preamp_r_changed.clone();
        let preamp_r_callback = Arc::new(move |_: f32| {
            preamp_r_changed_cb.store(true, Ordering::Relaxed);
        });
        let preamp_changed = Arc::new(AtomicBool::new(false));
        let preamp_changed_cb = preamp_changed.clone();
        let preamp_callback = Arc::new(move |_: f32| {
            preamp_changed_cb.store(true, Ordering::Relaxed);
        });
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
        let detune_changed = Arc::new(AtomicBool::new(false));
        let detune_changed_cb = detune_changed.clone();
        let detune_callback = Arc::new(move |_: f32| {
            detune_changed_cb.store(true, Ordering::Relaxed);
        });
        let attack_changed = Arc::new(AtomicBool::new(false));
        let attack_changed_cb = attack_changed.clone();
        let attack_callback = Arc::new(move |_: f32| {
            attack_changed_cb.store(true, Ordering::Relaxed);
        });
        let decay_changed = Arc::new(AtomicBool::new(false));
        let decay_changed_cb = decay_changed.clone();
        let decay_callback = Arc::new(move |_: f32| {
            decay_changed_cb.store(true, Ordering::Relaxed);
        });
        let sustain_changed = Arc::new(AtomicBool::new(false));
        let sustain_changed_cb = sustain_changed.clone();
        let sustain_callback = Arc::new(move |_: f32| {
            sustain_changed_cb.store(true, Ordering::Relaxed);
        });
        let release_changed = Arc::new(AtomicBool::new(false));
        let release_changed_cb = release_changed.clone();
        let release_callback = Arc::new(move |_: f32| {
            release_changed_cb.store(true, Ordering::Relaxed);
        });
        let bend_depth_changed = Arc::new(AtomicBool::new(false));
        let bend_depth_changed_cb = bend_depth_changed.clone();
        let bend_depth_callback = Arc::new(move |_: f32| {
            bend_depth_changed_cb.store(true, Ordering::Relaxed);
        });

        Self {
            preamp_l: FloatParam::new(
                "Preamp L",
                0.0,
                FloatRange::Linear {
                    min: -30.0,
                    max: 15.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(5.0))
            .with_step_size(0.1)
            .with_unit(" dB")
            .with_callback(preamp_l_callback),
            preamp_l_changed,
            preamp_r: FloatParam::new(
                "Preamp R",
                0.0,
                FloatRange::Linear {
                    min: -30.0,
                    max: 15.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(5.0))
            .with_step_size(0.1)
            .with_unit(" dB")
            .with_callback(preamp_r_callback),
            preamp_r_changed,
            channel_mode: Arc::new(Mutex::new(0)),
            preamp: FloatParam::new(
                "Preamp (legacy)",
                0.0,
                FloatRange::Linear {
                    min: -30.0,
                    max: 15.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(5.0))
            .with_step_size(0.1)
            .with_unit(" dB")
            .with_callback(preamp_callback),
            preamp_changed,
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
                1.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_step_size(0.001)
            .with_callback(sample_end_callback),
            sample_end_changed,
            detune: FloatParam::new(
                "Detune",
                0.0,
                FloatRange::Linear {
                    min: -100.0,
                    max: 100.0,
                },
            )
            .with_step_size(0.1)
            .with_unit(" ¢")
            .with_callback(detune_callback),
            detune_changed,
            attack: FloatParam::new(
                "Attack",
                0.01,
                FloatRange::Linear {
                    min: 0.0,
                    max: 5.0,
                },
            )
            .with_step_size(0.001)
            .with_unit(" s")
            .with_callback(attack_callback),
            attack_changed,
            decay: FloatParam::new(
                "Decay",
                0.1,
                FloatRange::Linear {
                    min: 0.0,
                    max: 5.0,
                },
            )
            .with_step_size(0.001)
            .with_unit(" s")
            .with_callback(decay_callback),
            decay_changed,
            sustain: FloatParam::new(
                "Sustain",
                1.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_step_size(0.001)
            .with_callback(sustain_callback),
            sustain_changed,
            release: FloatParam::new(
                "Release",
                0.25,
                FloatRange::Linear {
                    min: 0.0,
                    max: 10.0,
                },
            )
            .with_step_size(0.001)
            .with_unit(" s")
            .with_callback(release_callback),
            release_changed,
            bend_depth: FloatParam::new(
                "Bend Depth",
                200.0,
                FloatRange::Linear {
                    min: 100.0,
                    max: 400.0,
                },
            )
            .with_step_size(1.0)
            .with_unit(" cents")
            .with_callback(bend_depth_callback),
            bend_depth_changed,
            polyphony: Arc::new(Mutex::new(16)),
            nudge_to_12edo: Arc::new(Mutex::new(false)),
            reference_frequency_hz: Arc::new(Mutex::new(None)),
            detected_pitch_hz: Arc::new(Mutex::new(None)),
            scl_file: Arc::new(Mutex::new(None)),
            kbm_file: Arc::new(Mutex::new(None)),
            cache_dir: Arc::new(Mutex::new(None)),
            sample_uuid: Arc::new(Mutex::new(None)),
        }
    }
}
