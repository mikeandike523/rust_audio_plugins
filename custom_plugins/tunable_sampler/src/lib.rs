mod cache;
mod constants;
mod params;
mod pitch;
mod resample;
mod types;

use nih_plug::prelude::*;
use nih_plug_webview::*;
use serde_json::json;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use crate::cache::{
    effective_cache_dir, get_sample_dir, new_unique_cache_key, queue_folder_result,
    sample_cache_exists, sample_dir, send_cached_sample_if_available, save_sample_to_cache,
};
use crate::constants::{
    DEFAULT_RESAMPLE_POINTS, GUI_DEV_SERVER_URL, GUI_HEIGHT, GUI_PUBLISHED_URL, GUI_WIDTH,
};
use crate::params::TunableSamplerParams;
use crate::pitch::spawn_pitch_estimate;
use crate::resample::spawn_resample_task;
use crate::types::{Action, FolderSelectionResult, PitchEvent, ResampleEvent};

pub struct TunableSampler {
    params: Arc<TunableSamplerParams>,
    pending_folder_result: Arc<Mutex<Option<FolderSelectionResult>>>,
    pending_folder_dirty: Arc<AtomicBool>,
    sample_rate_hz: Arc<AtomicU32>,
    sample_rate_dirty: Arc<AtomicBool>,
    resample_points_input: Arc<AtomicU32>,
    resample_points_pitch: Arc<AtomicU32>,
    resample_requested: Arc<AtomicBool>,
    resample_in_progress: Arc<AtomicBool>,
    resample_events: Arc<Mutex<Vec<ResampleEvent>>>,
    resample_events_dirty: Arc<AtomicBool>,
    pitch_in_progress: Arc<AtomicBool>,
    pitch_events: Arc<Mutex<Vec<PitchEvent>>>,
    pitch_events_dirty: Arc<AtomicBool>,
}

impl Default for TunableSampler {
    fn default() -> Self {
        Self {
            params: Arc::new(TunableSamplerParams::default()),
            pending_folder_result: Arc::new(Mutex::new(None)),
            pending_folder_dirty: Arc::new(AtomicBool::new(false)),
            sample_rate_hz: Arc::new(AtomicU32::new(0)),
            sample_rate_dirty: Arc::new(AtomicBool::new(false)),
            resample_points_input: Arc::new(AtomicU32::new(DEFAULT_RESAMPLE_POINTS)),
            resample_points_pitch: Arc::new(AtomicU32::new(DEFAULT_RESAMPLE_POINTS)),
            resample_requested: Arc::new(AtomicBool::new(false)),
            resample_in_progress: Arc::new(AtomicBool::new(false)),
            resample_events: Arc::new(Mutex::new(Vec::new())),
            resample_events_dirty: Arc::new(AtomicBool::new(false)),
            pitch_in_progress: Arc::new(AtomicBool::new(false)),
            pitch_events: Arc::new(Mutex::new(Vec::new())),
            pitch_events_dirty: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl TunableSampler {
    fn send_state(
        ctx: &WindowHandler,
        params: &Arc<TunableSamplerParams>,
        sample_rate_hz: &Arc<AtomicU32>,
        resample_points_input: &Arc<AtomicU32>,
        resample_points_pitch: &Arc<AtomicU32>,
    ) {
        let cache_dir_override = params.cache_dir.lock().ok().and_then(|g| g.clone());
        let effective_dir = effective_cache_dir(&cache_dir_override);

        ctx.send_json(json!({
            "type": "State",
            "pluginVersion": env!("CARGO_PKG_VERSION"),
            "effectiveCacheDir": effective_dir.to_string_lossy(),
            "cacheDirOverride": cache_dir_override,
            "gain": params.gain.value(),
            "detune": params.detune.value(),
            "sampleStart": params.sample_start.value(),
            "sampleEnd": params.sample_end.value(),
            "projectSampleRate": sample_rate_hz.load(Ordering::Relaxed),
            "resamplePointsInput": resample_points_input.load(Ordering::Relaxed),
            "resamplePointsPitch": resample_points_pitch.load(Ordering::Relaxed),
        }));
    }

    fn resolve_gui_url() -> &'static str {
        match std::thread::spawn(move || {
            use std::time::Duration;
            let client = std::sync::Arc::new(
                ureq::AgentBuilder::new()
                    .timeout_connect(Duration::from_millis(500))
                    .timeout_read(Duration::from_millis(500))
                    .build(),
            );

            client.get(GUI_DEV_SERVER_URL).call()
        })
        .join()
        {
            Ok(Ok(_)) => GUI_DEV_SERVER_URL,
            _ => {
                println!(
                    "Local dev server not available, using production URL: {}",
                    GUI_PUBLISHED_URL
                );
                GUI_PUBLISHED_URL
            }
        }
    }
}

impl Plugin for TunableSampler {
    const NAME: &'static str = "Tunable Sampler";
    const VENDOR: &'static str = "WTH Plugins";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate_hz
            .store(buffer_config.sample_rate.round() as u32, Ordering::Relaxed);
        self.sample_rate_dirty.store(true, Ordering::Relaxed);
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let sample_rate = context.transport().sample_rate.round() as u32;
        if sample_rate != self.sample_rate_hz.load(Ordering::Relaxed) {
            self.sample_rate_hz.store(sample_rate, Ordering::Relaxed);
            self.sample_rate_dirty.store(true, Ordering::Relaxed);
            self.resample_requested.store(true, Ordering::Relaxed);
        }

        for channel in buffer.as_slice().iter_mut() {
            channel.fill(0.0);
        }

        ProcessStatus::Normal
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let pending_folder_result = self.pending_folder_result.clone();
        let pending_folder_dirty = self.pending_folder_dirty.clone();
        let sample_rate_hz = self.sample_rate_hz.clone();
        let sample_rate_dirty = self.sample_rate_dirty.clone();
        let resample_points_input = self.resample_points_input.clone();
        let resample_points_pitch = self.resample_points_pitch.clone();
        let resample_requested = self.resample_requested.clone();
        let resample_in_progress = self.resample_in_progress.clone();
        let resample_events = self.resample_events.clone();
        let resample_events_dirty = self.resample_events_dirty.clone();
        let pitch_in_progress = self.pitch_in_progress.clone();
        let pitch_events = self.pitch_events.clone();
        let pitch_events_dirty = self.pitch_events_dirty.clone();

        let source = HTMLSource::URL(Self::resolve_gui_url());
        // Tracks which UUID we last sent a cached sample for, to avoid re-sending.
        let last_sample_uuid: Mutex<Option<String>> = Mutex::new(None);
        let can_send_cached_sample = AtomicBool::new(false);
        let editor = WebViewEditor::new(source, (GUI_WIDTH, GUI_HEIGHT))
            .with_developer_mode(true)
            .with_event_loop(move |ctx, setter, _window| {
                while let Ok(value) = ctx.next_event() {
                    match serde_json::from_value::<Action>(value) {
                        Ok(action) => match action {
                            Action::Init => {
                                can_send_cached_sample.store(true, Ordering::Relaxed);
                                TunableSampler::send_state(
                                    ctx,
                                    &params,
                                    &sample_rate_hz,
                                    &resample_points_input,
                                    &resample_points_pitch,
                                );
                            }
                            Action::RequestState => {
                                can_send_cached_sample.store(true, Ordering::Relaxed);
                                TunableSampler::send_state(
                                    ctx,
                                    &params,
                                    &sample_rate_hz,
                                    &resample_points_input,
                                    &resample_points_pitch,
                                );
                            }
                            Action::PickCacheDir => {
                                let cache_dir = params.cache_dir.clone();
                                let pending_folder_result = pending_folder_result.clone();
                                let pending_folder_dirty = pending_folder_dirty.clone();
                                std::thread::spawn(move || {
                                    let current =
                                        cache_dir.lock().ok().and_then(|g| g.clone());
                                    let selection = tinyfiledialogs::select_folder_dialog(
                                        "Select cache folder",
                                        current.as_deref().unwrap_or(""),
                                    );
                                    let result = match selection {
                                        Some(path) => FolderSelectionResult::Selected {
                                            path: std::path::PathBuf::from(path),
                                        },
                                        None => FolderSelectionResult::Canceled,
                                    };
                                    queue_folder_result(
                                        &pending_folder_result,
                                        &pending_folder_dirty,
                                        result,
                                    );
                                });
                            }
                            Action::SetCacheDir { path } => {
                                *params.cache_dir.lock().unwrap() = Some(path.clone());
                                let effective = effective_cache_dir(&Some(path.clone()));
                                ctx.send_json(json!({
                                    "type": "State",
                                    "effectiveCacheDir": effective.to_string_lossy(),
                                    "cacheDirOverride": path,
                                }));
                            }
                            Action::ClearCacheDir => {
                                *params.cache_dir.lock().unwrap() = None;
                                let effective = effective_cache_dir(&None);
                                ctx.send_json(json!({
                                    "type": "State",
                                    "effectiveCacheDir": effective.to_string_lossy(),
                                    "cacheDirOverride": serde_json::Value::Null,
                                }));
                            }
                            Action::SetGain { value } => {
                                setter.begin_set_parameter(&params.gain);
                                setter.set_parameter(&params.gain, value);
                                setter.end_set_parameter(&params.gain);
                            }
                            Action::SetDetune { value } => {
                                let clamped = value.clamp(-100.0, 100.0);
                                setter.begin_set_parameter(&params.detune);
                                setter.set_parameter(&params.detune, clamped);
                                setter.end_set_parameter(&params.detune);
                                params.detune_changed.store(false, Ordering::Relaxed);
                            }
                            Action::SetSampleStart { value } => {
                                let clamped = value.clamp(0.0, 1.0);
                                setter.begin_set_parameter(&params.sample_start);
                                setter.set_parameter(&params.sample_start, clamped);
                                setter.end_set_parameter(&params.sample_start);
                                // Suppress echo: the GUI already has the correct value, echoing
                                // it back causes visual handle resets during throttled drags.
                                params.sample_start_changed.store(false, Ordering::Relaxed);
                            }
                            Action::SetSampleEnd { value } => {
                                let clamped = value.clamp(0.0, 1.0);
                                setter.begin_set_parameter(&params.sample_end);
                                setter.set_parameter(&params.sample_end, clamped);
                                setter.end_set_parameter(&params.sample_end);
                                // Suppress echo: same reason as SetSampleStart above.
                                params.sample_end_changed.store(false, Ordering::Relaxed);
                            }
                            Action::SetResamplePointsInput { points } => {
                                resample_points_input.store(points, Ordering::Relaxed);
                                resample_requested.store(true, Ordering::Relaxed);
                                ctx.send_json(json!({
                                    "type": "State",
                                    "resamplePointsInput": points,
                                }));
                            }
                            Action::SetResamplePointsPitch { points } => {
                                resample_points_pitch.store(points, Ordering::Relaxed);
                                ctx.send_json(json!({
                                    "type": "State",
                                    "resamplePointsPitch": points,
                                }));
                            }
                            Action::RequestPitchEstimate { sample_start } => {
                                if !pitch_in_progress.load(Ordering::Relaxed) {
                                    if let Some(s_dir) = get_sample_dir(&params.cache_dir, &params.sample_uuid) {
                                        if sample_cache_exists(&s_dir) {
                                            pitch_in_progress.store(true, Ordering::Relaxed);
                                            ctx.send_json(json!({ "type": "PitchEstimating" }));
                                            spawn_pitch_estimate(
                                                s_dir,
                                                sample_start,
                                                pitch_in_progress.clone(),
                                                pitch_events.clone(),
                                                pitch_events_dirty.clone(),
                                            );
                                        }
                                    }
                                }
                            }
                            Action::SaveSample {
                                name,
                                sample_rate,
                                channels,
                                frames,
                                data_base64,
                            } => {
                                let cache_dir_override =
                                    params.cache_dir.lock().ok().and_then(|g| g.clone());
                                let effective_dir = effective_cache_dir(&cache_dir_override);

                                // Assign a UUID on first save; reuse on subsequent saves.
                                let uuid = {
                                    let mut uuid_guard = params.sample_uuid.lock().unwrap();
                                    if uuid_guard.is_none() {
                                        *uuid_guard =
                                            Some(new_unique_cache_key(&effective_dir));
                                    }
                                    uuid_guard.clone().unwrap()
                                };

                                let s_dir = sample_dir(&effective_dir, &uuid);

                                match save_sample_to_cache(
                                    &s_dir,
                                    &name,
                                    sample_rate,
                                    channels,
                                    frames,
                                    &data_base64,
                                ) {
                                    Ok(()) => {
                                        ctx.send_json(json!({
                                            "type": "SampleSaved",
                                            "name": name,
                                        }));
                                        resample_requested.store(true, Ordering::Relaxed);
                                    }
                                    Err(message) => {
                                        ctx.send_json(json!({
                                            "type": "SampleSaveError",
                                            "message": message,
                                        }));
                                    }
                                }
                            }
                        },
                        Err(err) => {
                            eprintln!("tunable_sampler: failed to parse event: {err}");
                        }
                    }
                }

                if pending_folder_dirty.swap(false, Ordering::Relaxed) {
                    if let Ok(mut guard) = pending_folder_result.lock() {
                        if let Some(result) = guard.take() {
                            match result {
                                FolderSelectionResult::Selected { path } => {
                                    let path_str = path.to_string_lossy().to_string();
                                    *params.cache_dir.lock().unwrap() = Some(path_str.clone());
                                    let effective =
                                        effective_cache_dir(&Some(path_str.clone()));
                                    ctx.send_json(json!({
                                        "type": "State",
                                        "effectiveCacheDir": effective.to_string_lossy(),
                                        "cacheDirOverride": path_str,
                                    }));
                                    // If this instance already has a sample UUID, check if
                                    // the data is present in the new cache dir.
                                    if let Some(s_dir) =
                                        get_sample_dir(&params.cache_dir, &params.sample_uuid)
                                    {
                                        if sample_cache_exists(&s_dir) {
                                            resample_requested.store(true, Ordering::Relaxed);
                                        }
                                    }
                                }
                                FolderSelectionResult::Error { message } => {
                                    ctx.send_json(json!({
                                        "type": "CacheDirError",
                                        "message": message,
                                    }));
                                }
                                FolderSelectionResult::Canceled => {
                                    ctx.send_json(json!({
                                        "type": "CacheDirCanceled",
                                    }));
                                }
                            }
                        }
                    }
                }

                if params.gain_changed.swap(false, Ordering::Relaxed) {
                    ctx.send_json(json!({
                        "type": "State",
                        "gain": params.gain.value(),
                    }));
                }

                if params.detune_changed.swap(false, Ordering::Relaxed) {
                    ctx.send_json(json!({
                        "type": "State",
                        "detune": params.detune.value(),
                    }));
                }

                if params.sample_start_changed.swap(false, Ordering::Relaxed) {
                    ctx.send_json(json!({
                        "type": "State",
                        "sampleStart": params.sample_start.value(),
                    }));
                }

                if params.sample_end_changed.swap(false, Ordering::Relaxed) {
                    ctx.send_json(json!({
                        "type": "State",
                        "sampleEnd": params.sample_end.value(),
                    }));
                }

                if sample_rate_dirty.swap(false, Ordering::Relaxed) {
                    ctx.send_json(json!({
                        "type": "State",
                        "projectSampleRate": sample_rate_hz.load(Ordering::Relaxed),
                    }));
                }

                // Send cached sample to GUI when the plugin first connects or the UUID changes.
                if can_send_cached_sample.load(Ordering::Relaxed) {
                    let current_uuid =
                        params.sample_uuid.lock().ok().and_then(|g| g.clone());
                    let mut should_check = false;
                    if let Ok(mut guard) = last_sample_uuid.lock() {
                        if guard.as_ref() != current_uuid.as_ref() {
                            should_check = true;
                            *guard = current_uuid.clone();
                        }
                    }
                    if should_check {
                        if let Some(uuid) = current_uuid {
                            let cache_dir_override =
                                params.cache_dir.lock().ok().and_then(|g| g.clone());
                            let effective_dir = effective_cache_dir(&cache_dir_override);
                            let s_dir = sample_dir(&effective_dir, &uuid);
                            match send_cached_sample_if_available(ctx, &s_dir) {
                                Ok(found) => {
                                    if found {
                                        resample_requested.store(true, Ordering::Relaxed);
                                    }
                                }
                                Err(message) => {
                                    ctx.send_json(json!({
                                        "type": "CachedSampleError",
                                        "message": message,
                                    }));
                                }
                            }
                        }
                    }
                }

                if resample_events_dirty.swap(false, Ordering::Relaxed) {
                    if let Ok(mut guard) = resample_events.lock() {
                        let events: Vec<ResampleEvent> = guard.drain(..).collect();
                        drop(guard);
                        for event in events {
                            match event {
                                ResampleEvent::Started { label } => {
                                    ctx.send_json(json!({
                                        "type": "ResampleStarted",
                                        "label": label,
                                        "progress": 0.0,
                                    }));
                                }
                                ResampleEvent::Progress { progress } => {
                                    ctx.send_json(json!({
                                        "type": "ResampleProgress",
                                        "progress": progress,
                                    }));
                                }
                                ResampleEvent::Completed { message } => {
                                    ctx.send_json(json!({
                                        "type": "ResampleComplete",
                                        "message": message,
                                    }));
                                }
                                ResampleEvent::Error { message } => {
                                    ctx.send_json(json!({
                                        "type": "ResampleError",
                                        "message": message,
                                    }));
                                }
                            }
                        }
                    }
                }

                if pitch_events_dirty.swap(false, Ordering::Relaxed) {
                    if let Ok(mut guard) = pitch_events.lock() {
                        let events: Vec<PitchEvent> = guard.drain(..).collect();
                        drop(guard);
                        for event in events {
                            match event {
                                PitchEvent::Detected { hz } => {
                                    ctx.send_json(json!({
                                        "type": "PitchDetected",
                                        "hz": hz,
                                    }));
                                }
                                PitchEvent::NoResult => {
                                    ctx.send_json(json!({ "type": "PitchNoResult" }));
                                }
                                PitchEvent::Error { message } => {
                                    ctx.send_json(json!({
                                        "type": "PitchEstimateError",
                                        "message": message,
                                    }));
                                }
                            }
                        }
                    }
                }

                if resample_requested.load(Ordering::Relaxed)
                    && !resample_in_progress.load(Ordering::Relaxed)
                {
                    if let Some(s_dir) =
                        get_sample_dir(&params.cache_dir, &params.sample_uuid)
                    {
                        let target_rate = sample_rate_hz.load(Ordering::Relaxed);
                        if target_rate > 0 && sample_cache_exists(&s_dir) {
                            resample_requested.store(false, Ordering::Relaxed);
                            resample_in_progress.store(true, Ordering::Relaxed);
                            spawn_resample_task(
                                s_dir,
                                target_rate,
                                resample_points_input.load(Ordering::Relaxed),
                                resample_in_progress.clone(),
                                resample_events.clone(),
                                resample_events_dirty.clone(),
                            );
                        }
                    }
                }
            });

        Some(Box::new(editor))
    }
}

impl Vst3Plugin for TunableSampler {
    const VST3_CLASS_ID: [u8; 16] = *b"WTH_TunableSampl";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth, Vst3SubCategory::Sampler];
}

nih_export_vst3!(TunableSampler);

impl ClapPlugin for TunableSampler {
    const CLAP_ID: &'static str = "wthplugins.tunable_sampler";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Tunable sampler instrument (work in progress)");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;

    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Sampler,
        ClapFeature::Stereo,
    ];
}

nih_export_clap!(TunableSampler);
