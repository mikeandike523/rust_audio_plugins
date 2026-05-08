mod adsr;
mod cache;
mod constants;
mod params;
mod pitch;
mod resample;
mod remote_logging;
mod tuning;
mod types;

use nih_plug::prelude::*;
use nih_plug_webview::*;
use serde_json::json;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use crate::tuning::TuningState;
use nih_plug::prelude::util;

use crate::cache::{
    default_webview_userdata_dir, effective_cache_dir, get_sample_dir, load_resampled_data,
    new_unique_cache_key, queue_folder_result, sample_cache_exists, sample_dir,
    send_cached_sample_if_available, save_sample_to_cache,
};
use crate::constants::{
    DEFAULT_RESAMPLE_QUALITY_INPUT, DEFAULT_RESAMPLE_QUALITY_PITCH,
    GUI_DEV_SERVER_URL, GUI_HEIGHT, GUI_PUBLISHED_URL, GUI_WIDTH,
};
use crate::params::TunableSamplerParams;
use crate::pitch::spawn_pitch_estimate;
use crate::remote_logging::RemoteLogger;
use crate::resample::{spawn_resample_task, resample_to_file, ResampleQuality};
use crate::types::{Action, FolderSelectionResult, PitchEvent, ResampleEvent};
use crate::adsr::{EnvelopeParams, is_finished, value_at};

const MAX_VOICES: usize = 32;

struct RuntimeSample {
    channels: usize,
    frames: usize,
    data: Vec<f32>,
}

struct Voice {
    active: bool,
    note: u8,
    sample_pos: f64,
    velocity_gain: f32,
    seq: u64,
    start_frame: usize,
    end_frame: usize,
    start_ts: u64,
    release_ts: Option<u64>,
}

impl Voice {
    fn idle() -> Self {
        Self {
            active: false,
            note: 0,
            sample_pos: 0.0,
            velocity_gain: 1.0,
            seq: 0,
            start_frame: 0,
            end_frame: 0,
            start_ts: 0,
            release_ts: None,
        }
    }
}

fn resolve_reference_frequency(raw_hz: f32, nudge_to_12edo: bool) -> f32 {
    if !nudge_to_12edo {
        return raw_hz;
    }

    let midi_float = 69.0 + 12.0 * (raw_hz / 440.0).log2();
    let midi_rounded = midi_float.round();
    440.0 * 2.0_f32.powf((midi_rounded - 69.0) / 12.0)
}

fn load_runtime_sample(
    params: &Arc<TunableSamplerParams>,
    target_sample_rate: u32,
) -> Result<Option<RuntimeSample>, String> {
    let Some(s_dir) = get_sample_dir(&params.cache_dir, &params.sample_uuid) else {
        return Ok(None);
    };
    if !sample_cache_exists(&s_dir) {
        return Ok(None);
    }

    let (metadata, data) = load_resampled_data(&s_dir)?;
    if metadata.sample_rate != target_sample_rate {
        return Ok(None);
    }

    Ok(Some(RuntimeSample {
        channels: metadata.channels as usize,
        frames: metadata.frames as usize,
        data,
    }))
}

pub struct TunableSampler {
    params: Arc<TunableSamplerParams>,
    pending_folder_result: Arc<Mutex<Option<FolderSelectionResult>>>,
    pending_folder_dirty: Arc<AtomicBool>,
    sample_rate_hz: Arc<AtomicU32>,
    sample_rate_dirty: Arc<AtomicBool>,
    resample_quality_input: Arc<AtomicU32>,
    resample_quality_pitch: Arc<AtomicU32>,
    resample_requested: Arc<AtomicBool>,
    resample_force: Arc<AtomicBool>,
    resample_in_progress: Arc<AtomicBool>,
    resample_events: Arc<Mutex<Vec<ResampleEvent>>>,
    resample_events_dirty: Arc<AtomicBool>,
    pitch_in_progress: Arc<AtomicBool>,
    pitch_events: Arc<Mutex<Vec<PitchEvent>>>,
    pitch_events_dirty: Arc<AtomicBool>,
    tuning_state: Arc<Mutex<TuningState>>,
    runtime_sample: Arc<Mutex<Option<RuntimeSample>>>,
    voices: Vec<Voice>,
    voice_seq: u64,
    sample_clock: u64,
    pitch_bend: f32,
    remote_logger: RemoteLogger,
    resample_was_in_progress: bool,
}

impl Default for TunableSampler {
    fn default() -> Self {
        Self {
            params: Arc::new(TunableSamplerParams::default()),
            pending_folder_result: Arc::new(Mutex::new(None)),
            pending_folder_dirty: Arc::new(AtomicBool::new(false)),
            sample_rate_hz: Arc::new(AtomicU32::new(0)),
            sample_rate_dirty: Arc::new(AtomicBool::new(false)),
            resample_quality_input: Arc::new(AtomicU32::new(DEFAULT_RESAMPLE_QUALITY_INPUT)),
            resample_quality_pitch: Arc::new(AtomicU32::new(DEFAULT_RESAMPLE_QUALITY_PITCH)),
            resample_requested: Arc::new(AtomicBool::new(false)),
            resample_force: Arc::new(AtomicBool::new(false)),
            resample_in_progress: Arc::new(AtomicBool::new(false)),
            resample_events: Arc::new(Mutex::new(Vec::new())),
            resample_events_dirty: Arc::new(AtomicBool::new(false)),
            pitch_in_progress: Arc::new(AtomicBool::new(false)),
            pitch_events: Arc::new(Mutex::new(Vec::new())),
            pitch_events_dirty: Arc::new(AtomicBool::new(false)),
            tuning_state: Arc::new(Mutex::new(TuningState::from_files(None, None))),
            runtime_sample: Arc::new(Mutex::new(None)),
            voices: (0..MAX_VOICES).map(|_| Voice::idle()).collect(),
            voice_seq: 0,
            sample_clock: 0,
            pitch_bend: 0.5,
            remote_logger: RemoteLogger::new(9099),
            resample_was_in_progress: false,
        }
    }
}

impl TunableSampler {
    fn send_state(
        ctx: &WindowHandler,
        params: &Arc<TunableSamplerParams>,
        sample_rate_hz: &Arc<AtomicU32>,
        resample_quality_input: &Arc<AtomicU32>,
        resample_quality_pitch: &Arc<AtomicU32>,
        tuning_state: &Arc<Mutex<TuningState>>,
    ) {
        let cache_dir_override = params.cache_dir.lock().ok().and_then(|g| g.clone());
        let effective_dir = effective_cache_dir(&cache_dir_override);
        let polyphony = params.polyphony.lock().ok().map(|g| *g).unwrap_or(16);
        let nudge_to_12edo = params.nudge_to_12edo.lock().ok().map(|g| *g).unwrap_or(false);
        let reference_frequency_hz = params
            .reference_frequency_hz
            .lock()
            .ok()
            .and_then(|g| *g);
        let tuning_status = tuning_state
            .lock()
            .ok()
            .map(|g| g.status.clone())
            .unwrap_or_default();
        let detected_pitch_hz = params.detected_pitch_hz.lock().ok().and_then(|g| *g);

        ctx.send_json(json!({
            "type": "State",
            "pluginVersion": env!("CARGO_PKG_VERSION"),
            "effectiveCacheDir": effective_dir.to_string_lossy(),
            "cacheDirOverride": cache_dir_override,
            "preamp": params.preamp.value(),
            "gain": params.gain.value(),
            "detune": params.detune.value(),
            "attack": params.attack.value(),
            "decay": params.decay.value(),
            "sustain": params.sustain.value(),
            "release": params.release.value(),
            "bendDepth": params.bend_depth.value(),
            "polyphony": polyphony,
            "nudgeTo12Edo": nudge_to_12edo,
            "referenceFrequencyHz": reference_frequency_hz,
            "detectedPitchHz": detected_pitch_hz,
            "tuningStatus": tuning_status,
            "sampleStart": params.sample_start.value(),
            "sampleEnd": params.sample_end.value(),
            "projectSampleRate": sample_rate_hz.load(Ordering::Relaxed),
            "resampleQualityInput": resample_quality_input.load(Ordering::Relaxed),
            "resampleQualityPitch": resample_quality_pitch.load(Ordering::Relaxed),
        }));
    }

    fn send_cached_sample_snapshot(
        ctx: &WindowHandler,
        params: &Arc<TunableSamplerParams>,
    ) -> Result<bool, String> {
        let Some(sample_dir) = get_sample_dir(&params.cache_dir, &params.sample_uuid) else {
            return Ok(false);
        };

        send_cached_sample_if_available(
            ctx,
            &sample_dir,
            params.sample_start.value(),
            params.sample_end.value(),
        )
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
        let scl_file = self.params.scl_file.lock().unwrap().clone();
        let kbm_file = self.params.kbm_file.lock().unwrap().clone();
        *self.tuning_state.lock().unwrap() = TuningState::from_files(scl_file.as_ref(), kbm_file.as_ref());
        let target_rate = self.sample_rate_hz.load(Ordering::Relaxed);
        match load_runtime_sample(&self.params, target_rate) {
            Ok(sample) => {
                *self.runtime_sample.lock().unwrap() = sample;
            }
            Err(_) => {}
        }
        // If no cached resample exists at this sample rate (common when the DAW renders at a
        // different rate than playback), block here and do it synchronously. initialize() is
        // called before any processing starts, so blocking is acceptable and ensures the first
        // note at tick 0 is never silent.
        if self.runtime_sample.lock().map(|g| g.is_none()).unwrap_or(false) {
            if let Some(s_dir) = get_sample_dir(&self.params.cache_dir, &self.params.sample_uuid) {
                if sample_cache_exists(&s_dir) {
                    let quality = ResampleQuality::from_u32(
                        self.resample_quality_input.load(Ordering::Relaxed),
                    );
                    let _ = resample_to_file(&s_dir, target_rate, quality, false, &mut |_| {});
                    if let Ok(sample) = load_runtime_sample(&self.params, target_rate) {
                        *self.runtime_sample.lock().unwrap() = sample;
                    }
                }
            }
        }
        for voice in &mut self.voices {
            *voice = Voice::idle();
        }
        self.pitch_bend = 0.5;
        self.sample_clock = 0;
        self.resample_was_in_progress = false;
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
            if let Ok(mut guard) = self.runtime_sample.lock() {
                *guard = None;
            }
        }

        // Mirror the editor's resample trigger so render (no editor) also works.
        if self.resample_requested.load(Ordering::Relaxed)
            && !self.resample_in_progress.load(Ordering::Relaxed)
        {
            if let Some(s_dir) =
                get_sample_dir(&self.params.cache_dir, &self.params.sample_uuid)
            {
                let target_rate = self.sample_rate_hz.load(Ordering::Relaxed);
                if target_rate > 0 && sample_cache_exists(&s_dir) {
                    self.resample_requested.store(false, Ordering::Relaxed);
                    let force = self.resample_force.swap(false, Ordering::Relaxed);
                    self.resample_in_progress.store(true, Ordering::Relaxed);
                    spawn_resample_task(
                        s_dir,
                        target_rate,
                        self.resample_quality_input.load(Ordering::Relaxed),
                        force,
                        self.resample_in_progress.clone(),
                        self.resample_events.clone(),
                        self.resample_events_dirty.clone(),
                    );
                }
            }
        }

        // Detect resample completion and reload runtime_sample without needing the editor.
        let now_in_progress = self.resample_in_progress.load(Ordering::Relaxed);
        if self.resample_was_in_progress && !now_in_progress {
            if let Ok(sample) = load_runtime_sample(
                &self.params,
                self.sample_rate_hz.load(Ordering::Relaxed),
            ) {
                if let Ok(mut guard) = self.runtime_sample.try_lock() {
                    *guard = sample;
                }
            }
        }
        self.resample_was_in_progress = now_in_progress;

        let mut events: Vec<NoteEvent<()>> = Vec::new();
        while let Some(e) = context.next_event() {
            events.push(e);
        }
        events.sort_by(|a, b| a.timing().cmp(&b.timing()));

        let runtime_sample_guard = self.runtime_sample.try_lock().ok();
        let runtime_sample = runtime_sample_guard.as_ref().and_then(|g| g.as_ref());

        let polyphony = self
            .params
            .polyphony
            .lock()
            .ok()
            .map(|g| (*g as usize).clamp(16, MAX_VOICES))
            .unwrap_or(16);
        let env = EnvelopeParams {
            attack: self.params.attack.value(),
            decay: self.params.decay.value(),
            sustain: self.params.sustain.value(),
            release: self.params.release.value(),
        };
        let detune_cents = self.params.detune.value();
        let bend_depth_cents = self.params.bend_depth.value();
        let reference_frequency_hz = self
            .params
            .reference_frequency_hz
            .lock()
            .ok()
            .and_then(|g| *g);
        let tuning_state = self
            .tuning_state
            .lock()
            .ok()
            .map(|g| g.clone())
            .unwrap_or_else(|| TuningState::from_files(None, None));

        for (sample_id, channels) in buffer.iter_samples().enumerate() {
            self.sample_clock = self.sample_clock.wrapping_add(1);

            for evt in events.iter().filter(|e| e.timing() as usize == sample_id) {
                match evt {
                    NoteEvent::MidiPitchBend { value, .. } => {
                        self.pitch_bend = *value;
                    }
                    NoteEvent::NoteOn { note, velocity, .. } if *velocity > 0.0 => {
                        if let Some(sample) = runtime_sample {
                            let start_norm = self.params.sample_start.value().clamp(0.0, 1.0);
                            let mut end_norm = self.params.sample_end.value().clamp(0.0, 1.0);
                            if end_norm <= start_norm {
                                end_norm = 1.0;
                            }
                            let start_frame =
                                ((start_norm * sample.frames as f32) as usize).min(sample.frames.saturating_sub(1));
                            let end_frame =
                                ((end_norm * sample.frames as f32) as usize).clamp(start_frame + 1, sample.frames);
                            let slot = self.voices[..polyphony]
                                .iter()
                                .position(|v| !v.active)
                                .unwrap_or_else(|| {
                                    self.voices[..polyphony]
                                        .iter()
                                        .enumerate()
                                        .min_by_key(|(_, v)| v.seq)
                                        .map(|(i, _)| i)
                                        .unwrap_or(0)
                                });

                            self.voices[slot] = Voice {
                                active: true,
                                note: *note,
                                sample_pos: start_frame as f64,
                                velocity_gain: *velocity,
                                seq: self.voice_seq,
                                start_frame,
                                end_frame,
                                start_ts: self.sample_clock,
                                release_ts: None,
                            };
                            self.voice_seq = self.voice_seq.wrapping_add(1);
                        }
                    }
                    NoteEvent::NoteOff { note, .. } => {
                        for voice in self.voices[..polyphony].iter_mut() {
                            if voice.active && voice.note == *note && voice.release_ts.is_none() {
                                voice.release_ts = Some(self.sample_clock);
                            }
                        }
                    }
                    NoteEvent::NoteOn { note, velocity, .. } if *velocity == 0.0 => {
                        for voice in self.voices[..polyphony].iter_mut() {
                            if voice.active && voice.note == *note && voice.release_ts.is_none() {
                                voice.release_ts = Some(self.sample_clock);
                            }
                        }
                    }
                    _ => {}
                }
            }

            let mut out = [0.0_f32; 2];

            if let Some(sample) = runtime_sample {
                for voice in self.voices[..polyphony].iter_mut() {
                    if !voice.active {
                        continue;
                    }

                    let t = (self.sample_clock - voice.start_ts) as f32 / sample_rate as f32;
                    let note_off = voice
                        .release_ts
                        .map(|off| (off - voice.start_ts) as f32 / sample_rate as f32);
                    if is_finished(t, note_off, &env) {
                        voice.active = false;
                        continue;
                    }

                    if voice.sample_pos >= voice.end_frame as f64 {
                        voice.active = false;
                        continue;
                    }

                    let tuned_freq = tuning_state.frequency_for_note(voice.note as f32);
                    let bend_signed = (self.pitch_bend - 0.5) * 2.0;
                    let total_cents = detune_cents + bend_signed * bend_depth_cents;
                    let target_frequency =
                        tuned_freq * 2.0_f32.powf(total_cents / 1200.0);
                    let step = match reference_frequency_hz {
                        Some(reference_hz) if reference_hz > 0.0 => {
                            (target_frequency / reference_hz) as f64
                        }
                        _ => 1.0,
                    };

                    let idx = voice.sample_pos.floor() as usize;
                    let frac = (voice.sample_pos - idx as f64) as f32;
                    let next_idx = (idx + 1).min(voice.end_frame.saturating_sub(1));
                    let frame0 = idx * sample.channels;
                    let frame1 = next_idx * sample.channels;
                    let s0_l = sample.data[frame0];
                    let s1_l = sample.data[frame1];
                    let left = s0_l + (s1_l - s0_l) * frac;
                    let right = if sample.channels > 1 {
                        let s0_r = sample.data[frame0 + 1];
                        let s1_r = sample.data[frame1 + 1];
                        s0_r + (s1_r - s0_r) * frac
                    } else {
                        left
                    };

                    let amp = value_at(t, note_off, &env) * voice.velocity_gain;
                    out[0] += left * amp;
                    out[1] += right * amp;
                    voice.sample_pos = (voice.sample_pos + step).max(voice.start_frame as f64);
                }
            }

            let gain = util::db_to_gain_fast(self.params.preamp.smoothed.next())
                * util::db_to_gain_fast(self.params.gain.smoothed.next());
            let mut ch = channels.into_iter();
            if let Some(s) = ch.next() {
                *s = out[0] * gain;
            }
            if let Some(s) = ch.next() {
                *s = out[1] * gain;
            }
        }

        ProcessStatus::Normal
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let pending_folder_result = self.pending_folder_result.clone();
        let pending_folder_dirty = self.pending_folder_dirty.clone();
        let sample_rate_hz = self.sample_rate_hz.clone();
        let sample_rate_dirty = self.sample_rate_dirty.clone();
        let resample_quality_input = self.resample_quality_input.clone();
        let resample_quality_pitch = self.resample_quality_pitch.clone();
        let resample_requested = self.resample_requested.clone();
        let resample_force = self.resample_force.clone();
        let resample_in_progress = self.resample_in_progress.clone();
        let resample_events = self.resample_events.clone();
        let resample_events_dirty = self.resample_events_dirty.clone();
        let pitch_in_progress = self.pitch_in_progress.clone();
        let pitch_events = self.pitch_events.clone();
        let pitch_events_dirty = self.pitch_events_dirty.clone();
        let tuning_state = self.tuning_state.clone();
        let runtime_sample = self.runtime_sample.clone();
        let remote_logger = self.remote_logger.clone();

        let source = HTMLSource::URL(Self::resolve_gui_url());
        // Tracks which UUID we last sent a cached sample for, to avoid re-sending.
        let last_sample_uuid: Mutex<Option<String>> = Mutex::new(None);
        let can_send_cached_sample = AtomicBool::new(false);
        let editor = WebViewEditor::new(source, (GUI_WIDTH, GUI_HEIGHT))
            .with_data_directory(default_webview_userdata_dir())
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
                                    &resample_quality_input,
                                    &resample_quality_pitch,
                                    &tuning_state,
                                );
                                let current_uuid =
                                    params.sample_uuid.lock().ok().and_then(|g| g.clone());
                                if let Ok(mut guard) = last_sample_uuid.lock() {
                                    *guard = current_uuid;
                                }
                                if let Err(message) =
                                    TunableSampler::send_cached_sample_snapshot(ctx, &params)
                                {
                                    ctx.send_json(json!({
                                        "type": "CachedSampleError",
                                        "message": message,
                                    }));
                                }
                            }
                            Action::RequestState => {
                                can_send_cached_sample.store(true, Ordering::Relaxed);
                                TunableSampler::send_state(
                                    ctx,
                                    &params,
                                    &sample_rate_hz,
                                    &resample_quality_input,
                                    &resample_quality_pitch,
                                    &tuning_state,
                                );
                                let current_uuid =
                                    params.sample_uuid.lock().ok().and_then(|g| g.clone());
                                if let Ok(mut guard) = last_sample_uuid.lock() {
                                    *guard = current_uuid;
                                }
                                if let Err(message) =
                                    TunableSampler::send_cached_sample_snapshot(ctx, &params)
                                {
                                    ctx.send_json(json!({
                                        "type": "CachedSampleError",
                                        "message": message,
                                    }));
                                }
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
                            Action::SetPreamp { value } => {
                                let clamped = value.clamp(-30.0, 15.0);
                                setter.begin_set_parameter(&params.preamp);
                                setter.set_parameter(&params.preamp, clamped);
                                setter.end_set_parameter(&params.preamp);
                                params.preamp_changed.store(false, Ordering::Relaxed);
                            }
                            Action::SetDetune { value } => {
                                let clamped = value.clamp(-100.0, 100.0);
                                setter.begin_set_parameter(&params.detune);
                                setter.set_parameter(&params.detune, clamped);
                                setter.end_set_parameter(&params.detune);
                                params.detune_changed.store(false, Ordering::Relaxed);
                            }
                            Action::SetAttack { value } => {
                                let clamped = value.clamp(0.0, 5.0);
                                setter.begin_set_parameter(&params.attack);
                                setter.set_parameter(&params.attack, clamped);
                                setter.end_set_parameter(&params.attack);
                                params.attack_changed.store(false, Ordering::Relaxed);
                            }
                            Action::SetDecay { value } => {
                                let clamped = value.clamp(0.0, 5.0);
                                setter.begin_set_parameter(&params.decay);
                                setter.set_parameter(&params.decay, clamped);
                                setter.end_set_parameter(&params.decay);
                                params.decay_changed.store(false, Ordering::Relaxed);
                            }
                            Action::SetSustain { value } => {
                                let clamped = value.clamp(0.0, 1.0);
                                setter.begin_set_parameter(&params.sustain);
                                setter.set_parameter(&params.sustain, clamped);
                                setter.end_set_parameter(&params.sustain);
                                params.sustain_changed.store(false, Ordering::Relaxed);
                            }
                            Action::SetRelease { value } => {
                                let clamped = value.clamp(0.0, 10.0);
                                setter.begin_set_parameter(&params.release);
                                setter.set_parameter(&params.release, clamped);
                                setter.end_set_parameter(&params.release);
                                params.release_changed.store(false, Ordering::Relaxed);
                            }
                            Action::SetBendDepth { value } => {
                                let clamped = value.clamp(100.0, 400.0);
                                setter.begin_set_parameter(&params.bend_depth);
                                setter.set_parameter(&params.bend_depth, clamped);
                                setter.end_set_parameter(&params.bend_depth);
                                params.bend_depth_changed.store(false, Ordering::Relaxed);
                            }
                            Action::SetPolyphony { voices } => {
                                let clamped = match voices {
                                    24 => 24,
                                    32 => 32,
                                    _ => 16,
                                };
                                *params.polyphony.lock().unwrap() = clamped;
                                ctx.send_json(json!({
                                    "type": "State",
                                    "polyphony": clamped,
                                }));
                            }
                            Action::SetNudgeTo12Edo { enabled } => {
                                *params.nudge_to_12edo.lock().unwrap() = enabled;
                                if let Some(raw_hz) =
                                    params.detected_pitch_hz.lock().ok().and_then(|g| *g)
                                {
                                    let resolved = resolve_reference_frequency(raw_hz, enabled);
                                    *params.reference_frequency_hz.lock().unwrap() = Some(resolved);
                                }
                                ctx.send_json(json!({
                                    "type": "State",
                                    "nudgeTo12Edo": enabled,
                                    "referenceFrequencyHz": params.reference_frequency_hz.lock().ok().and_then(|g| *g),
                                }));
                            }
                            Action::SetSclFile { name, contents } => {
                                *params.scl_file.lock().unwrap() =
                                    Some(crate::tuning::TuningFile { name, contents });
                                let scl_file = params.scl_file.lock().unwrap().clone();
                                let kbm_file = params.kbm_file.lock().unwrap().clone();
                                let new_state =
                                    TuningState::from_files(scl_file.as_ref(), kbm_file.as_ref());
                                let status = new_state.status.clone();
                                *tuning_state.lock().unwrap() = new_state;
                                ctx.send_json(json!({
                                    "type": "State",
                                    "tuningStatus": status,
                                }));
                            }
                            Action::SetKbmFile { name, contents } => {
                                *params.kbm_file.lock().unwrap() =
                                    Some(crate::tuning::TuningFile { name, contents });
                                let scl_file = params.scl_file.lock().unwrap().clone();
                                let kbm_file = params.kbm_file.lock().unwrap().clone();
                                let new_state =
                                    TuningState::from_files(scl_file.as_ref(), kbm_file.as_ref());
                                let status = new_state.status.clone();
                                *tuning_state.lock().unwrap() = new_state;
                                ctx.send_json(json!({
                                    "type": "State",
                                    "tuningStatus": status,
                                }));
                            }
                            Action::ClearSclFile => {
                                *params.scl_file.lock().unwrap() = None;
                                let scl_file = params.scl_file.lock().unwrap().clone();
                                let kbm_file = params.kbm_file.lock().unwrap().clone();
                                let new_state =
                                    TuningState::from_files(scl_file.as_ref(), kbm_file.as_ref());
                                let status = new_state.status.clone();
                                *tuning_state.lock().unwrap() = new_state;
                                ctx.send_json(json!({
                                    "type": "State",
                                    "tuningStatus": status,
                                }));
                            }
                            Action::ClearKbmFile => {
                                *params.kbm_file.lock().unwrap() = None;
                                let scl_file = params.scl_file.lock().unwrap().clone();
                                let kbm_file = params.kbm_file.lock().unwrap().clone();
                                let new_state =
                                    TuningState::from_files(scl_file.as_ref(), kbm_file.as_ref());
                                let status = new_state.status.clone();
                                *tuning_state.lock().unwrap() = new_state;
                                ctx.send_json(json!({
                                    "type": "State",
                                    "tuningStatus": status,
                                }));
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
                            Action::SetResampleQualityInput { quality } => {
                                resample_quality_input.store(quality, Ordering::Relaxed);
                                resample_requested.store(true, Ordering::Relaxed);
                                ctx.send_json(json!({
                                    "type": "State",
                                    "resampleQualityInput": quality,
                                }));
                            }
                            Action::SetResampleQualityPitch { quality } => {
                                resample_quality_pitch.store(quality, Ordering::Relaxed);
                                ctx.send_json(json!({
                                    "type": "State",
                                    "resampleQualityPitch": quality,
                                }));
                            }
                            Action::ForceResample => {
                                resample_force.store(true, Ordering::Relaxed);
                                resample_requested.store(true, Ordering::Relaxed);
                            }
                            Action::RequestPitchEstimate { sample_start } => {
                                remote_logger.log_step(
                                    "pitch_request",
                                    format!("sample_start={sample_start:.4} in_progress={}", pitch_in_progress.load(Ordering::Relaxed)),
                                );
                                if !pitch_in_progress.load(Ordering::Relaxed) {
                                    if let Some(s_dir) = get_sample_dir(&params.cache_dir, &params.sample_uuid) {
                                        remote_logger.log_step(
                                            "pitch_request_path",
                                            format!("path={} exists={}", s_dir.display(), sample_cache_exists(&s_dir)),
                                        );
                                        if sample_cache_exists(&s_dir) {
                                            pitch_in_progress.store(true, Ordering::Relaxed);
                                            ctx.send_json(json!({ "type": "PitchEstimating" }));
                                            spawn_pitch_estimate(
                                                s_dir,
                                                sample_start,
                                                pitch_in_progress.clone(),
                                                pitch_events.clone(),
                                                pitch_events_dirty.clone(),
                                                remote_logger.clone(),
                                            );
                                        } else {
                                            remote_logger.log_step("pitch_request_skip", "sample cache missing".to_string());
                                        }
                                    } else {
                                        remote_logger.log_step("pitch_request_skip", "no sample dir".to_string());
                                    }
                                } else {
                                    remote_logger.log_step("pitch_request_skip", "already in progress".to_string());
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

                if params.preamp_changed.swap(false, Ordering::Relaxed) {
                    ctx.send_json(json!({
                        "type": "State",
                        "preamp": params.preamp.value(),
                    }));
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
                if params.attack_changed.swap(false, Ordering::Relaxed) {
                    ctx.send_json(json!({
                        "type": "State",
                        "attack": params.attack.value(),
                    }));
                }
                if params.decay_changed.swap(false, Ordering::Relaxed) {
                    ctx.send_json(json!({
                        "type": "State",
                        "decay": params.decay.value(),
                    }));
                }
                if params.sustain_changed.swap(false, Ordering::Relaxed) {
                    ctx.send_json(json!({
                        "type": "State",
                        "sustain": params.sustain.value(),
                    }));
                }
                if params.release_changed.swap(false, Ordering::Relaxed) {
                    ctx.send_json(json!({
                        "type": "State",
                        "release": params.release.value(),
                    }));
                }
                if params.bend_depth_changed.swap(false, Ordering::Relaxed) {
                    ctx.send_json(json!({
                        "type": "State",
                        "bendDepth": params.bend_depth.value(),
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
                            match send_cached_sample_if_available(
                                ctx,
                                &s_dir,
                                params.sample_start.value(),
                                params.sample_end.value(),
                            ) {
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
                                    if let Ok(sample) = load_runtime_sample(
                                        &params,
                                        sample_rate_hz.load(Ordering::Relaxed),
                                    ) {
                                        *runtime_sample.lock().unwrap() = sample;
                                    }
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
                                    remote_logger.log_step("pitch_event_detected", format!("hz={hz:.3}"));
                                    *params.detected_pitch_hz.lock().unwrap() = Some(hz as f32);
                                    let nudge_to_12edo =
                                        params.nudge_to_12edo.lock().ok().map(|g| *g).unwrap_or(false);
                                    let resolved_reference_hz =
                                        resolve_reference_frequency(hz as f32, nudge_to_12edo);
                                    *params.reference_frequency_hz.lock().unwrap() =
                                        Some(resolved_reference_hz);
                                    ctx.send_json(json!({
                                        "type": "PitchDetected",
                                        "hz": hz,
                                    }));
                                    ctx.send_json(json!({
                                        "type": "State",
                                        "referenceFrequencyHz": resolved_reference_hz,
                                    }));
                                }
                                PitchEvent::NoResult => {
                                    remote_logger.log_step("pitch_event_no_result", "queue drained to no result".to_string());
                                    ctx.send_json(json!({ "type": "PitchNoResult" }));
                                }
                                PitchEvent::Error { message } => {
                                    remote_logger.log_step("pitch_event_error", message.clone());
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
                            let force = resample_force.swap(false, Ordering::Relaxed);
                            resample_in_progress.store(true, Ordering::Relaxed);
                            spawn_resample_task(
                                s_dir,
                                target_rate,
                                resample_quality_input.load(Ordering::Relaxed),
                                force,
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
