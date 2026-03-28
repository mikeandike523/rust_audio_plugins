use base64::{engine::general_purpose, Engine as _};
use nih_plug::prelude::*;
use nih_plug_webview::*;
use serde::Deserialize;
use serde_json::json;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

const GUI_WIDTH: u32 = 960;
const GUI_HEIGHT: u32 = 640;
const GUI_DEV_SERVER_PROBE_URL: &str = "http://localhost:5173/wth-dispatch";
const GUI_PUBLISHED_URL: &str = "https://dispatch-web-gui.vercel.app";

const PAD_COUNT: usize = 16;
const PAD_MIDI_BASE: u8 = 36;
const MAX_VOICES: usize = 64;
const RESAMPLE_POINTS: usize = 64;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct Config {
    cache_dir: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self { cache_dir: None }
    }
}

fn config_file_path() -> Option<PathBuf> {
    let base = if cfg!(target_os = "windows") {
        std::env::var("APPDATA").ok().map(PathBuf::from)
    } else {
        std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join(".config"))
    };
    base.map(|b| b.join("dispatch").join("config.json"))
}

fn load_config() -> Config {
    config_file_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_config(config: &Config) {
    let Some(path) = config_file_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let _ = std::fs::write(&path, json);
    }
}

// ---------------------------------------------------------------------------
// In-memory pad audio (at project sample rate)
// ---------------------------------------------------------------------------

struct PadData {
    name: String,
    channels: usize,
    frames: usize,
    data: Vec<f32>,
}

// ---------------------------------------------------------------------------
// Voice
// ---------------------------------------------------------------------------

struct Voice {
    active: bool,
    pad_index: usize,
    sample_pos: usize,
    velocity_gain: f32,
    seq: u64,
}

impl Voice {
    fn idle() -> Self {
        Voice {
            active: false,
            pad_index: 0,
            sample_pos: 0,
            velocity_gain: 1.0,
            seq: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// UI event queue (plugin → UI)
// ---------------------------------------------------------------------------

enum UiEvent {
    SampleLoaded { pad_index: usize, name: String },
    SampleError { pad_index: usize, message: String },
}

// ---------------------------------------------------------------------------
// Params
// ---------------------------------------------------------------------------

#[derive(Params)]
struct DispatchParams {}

impl Default for DispatchParams {
    fn default() -> Self {
        Self {}
    }
}

// ---------------------------------------------------------------------------
// IPC: actions from UI
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Action {
    Init,
    SetCacheDir {
        path: String,
    },
    ClearCacheDir,
    SaveSample {
        #[serde(rename = "padIndex")]
        pad_index: usize,
        name: String,
        #[serde(rename = "sampleRate")]
        sample_rate: u32,
        channels: u16,
        frames: u32,
        #[serde(rename = "dataBase64")]
        data_base64: String,
    },
    SetPolyphony {
        voices: u32,
    },
    SetRetrigger {
        enabled: bool,
    },
}

// ---------------------------------------------------------------------------
// Disk helpers
// ---------------------------------------------------------------------------

fn pad_dir(cache_dir: &str, pad_index: usize) -> PathBuf {
    PathBuf::from(cache_dir).join(format!("pad_{pad_index}"))
}

fn save_pad_raw(
    cache_dir: &str,
    pad_index: usize,
    name: &str,
    sample_rate: u32,
    channels: u16,
    frames: u32,
    data: &[f32],
) -> Result<(), String> {
    let dir = pad_dir(cache_dir, pad_index);
    std::fs::create_dir_all(&dir).map_err(|e| format!("create pad dir: {e}"))?;

    let mut bytes = Vec::with_capacity(data.len() * 4);
    for &v in data {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    std::fs::write(dir.join("sample.array"), &bytes)
        .map_err(|e| format!("write sample.array: {e}"))?;

    let meta = json!({
        "name": name,
        "sample_rate": sample_rate,
        "channels": channels,
        "frames": frames,
    });
    std::fs::write(
        dir.join("sample.json"),
        serde_json::to_vec_pretty(&meta).unwrap(),
    )
    .map_err(|e| format!("write sample.json: {e}"))?;

    Ok(())
}

#[derive(serde::Deserialize)]
struct RawMeta {
    name: String,
    sample_rate: u32,
    channels: u16,
    #[allow(dead_code)]
    frames: u32,
}

#[derive(serde::Deserialize)]
struct ResampledMeta {
    name: String,
    sample_rate: u32,
    channels: u16,
    frames: u32,
    source_sample_rate: u32,
}

fn load_raw_meta(cache_dir: &str, pad_index: usize) -> Option<RawMeta> {
    let bytes = std::fs::read(pad_dir(cache_dir, pad_index).join("sample.json")).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn load_resampled(cache_dir: &str, pad_index: usize, project_rate: u32) -> Option<PadData> {
    let dir = pad_dir(cache_dir, pad_index);
    let meta_bytes = std::fs::read(dir.join("resampled.json")).ok()?;
    let meta: ResampledMeta = serde_json::from_slice(&meta_bytes).ok()?;
    let raw_meta = load_raw_meta(cache_dir, pad_index)?;
    if meta.sample_rate != project_rate || meta.source_sample_rate != raw_meta.sample_rate {
        return None;
    }
    let array_bytes = std::fs::read(dir.join("resampled.array")).ok()?;
    if array_bytes.len() != meta.frames as usize * meta.channels as usize * 4 {
        return None;
    }
    let mut data = Vec::with_capacity(array_bytes.len() / 4);
    for chunk in array_bytes.chunks_exact(4) {
        data.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    Some(PadData {
        name: meta.name,
        channels: meta.channels as usize,
        frames: meta.frames as usize,
        data,
    })
}

// ---------------------------------------------------------------------------
// Sinc resampler (adapted from tunable_sampler)
// ---------------------------------------------------------------------------

fn resample_sinc(input: &[f32], channels: usize, in_rate: u32, out_rate: u32) -> Vec<f32> {
    if in_rate == out_rate {
        return input.to_vec();
    }
    let in_frames = input.len() / channels;
    let out_frames = ((in_frames as f64) * (out_rate as f64) / (in_rate as f64))
        .round()
        .max(1.0) as usize;
    let taps = RESAMPLE_POINTS.max(2);
    let half = taps as i32 / 2;
    let pi = std::f64::consts::PI;
    let mut output = vec![0.0f32; out_frames * channels];

    for out_idx in 0..out_frames {
        let pos = out_idx as f64 * (in_rate as f64) / (out_rate as f64);
        let start = pos.floor() as i32 - half + 1;
        for ch in 0..channels {
            let (mut acc, mut norm) = (0.0f64, 0.0f64);
            for tap in 0..taps {
                let idx = start + tap as i32;
                if idx < 0 || idx >= in_frames as i32 {
                    continue;
                }
                let t = pos - idx as f64;
                let sinc = if t == 0.0 {
                    1.0
                } else {
                    (pi * t).sin() / (pi * t)
                };
                let window = if half > 0 && t.abs() <= half as f64 {
                    0.5 * (1.0 + (pi * t / half as f64).cos())
                } else {
                    0.0
                };
                let w = sinc * window;
                acc += input[idx as usize * channels + ch] as f64 * w;
                norm += w;
            }
            if norm != 0.0 {
                acc /= norm;
            }
            output[out_idx * channels + ch] = acc as f32;
        }
    }
    output
}

// ---------------------------------------------------------------------------
// Background resample task for one pad
// ---------------------------------------------------------------------------

fn spawn_pad_resample(
    cache_dir: String,
    pad_index: usize,
    project_rate: u32,
    pad_data: Arc<Vec<Mutex<Option<PadData>>>>,
    resample_in_progress: Arc<Vec<AtomicBool>>,
    ui_events: Arc<Mutex<Vec<UiEvent>>>,
    ui_dirty: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        let result = (|| -> Result<PadData, String> {
            let dir = pad_dir(&cache_dir, pad_index);
            let meta_bytes = std::fs::read(dir.join("sample.json"))
                .map_err(|e| format!("sample.json missing: {e}"))?;
            let meta: RawMeta = serde_json::from_slice(&meta_bytes)
                .map_err(|e| format!("sample.json parse: {e}"))?;
            let array_bytes = std::fs::read(dir.join("sample.array"))
                .map_err(|e| format!("sample.array missing: {e}"))?;
            let mut raw: Vec<f32> = Vec::with_capacity(array_bytes.len() / 4);
            for chunk in array_bytes.chunks_exact(4) {
                raw.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }

            let resampled = resample_sinc(&raw, meta.channels as usize, meta.sample_rate, project_rate);
            let out_frames = resampled.len() / meta.channels as usize;

            let mut out_bytes = Vec::with_capacity(resampled.len() * 4);
            for &v in &resampled {
                out_bytes.extend_from_slice(&v.to_le_bytes());
            }
            std::fs::write(dir.join("resampled.array"), &out_bytes)
                .map_err(|e| format!("write resampled.array: {e}"))?;

            let rmeta = json!({
                "name": meta.name,
                "sample_rate": project_rate,
                "channels": meta.channels,
                "frames": out_frames,
                "source_sample_rate": meta.sample_rate,
            });
            std::fs::write(
                dir.join("resampled.json"),
                serde_json::to_vec_pretty(&rmeta).unwrap(),
            )
            .map_err(|e| format!("write resampled.json: {e}"))?;

            Ok(PadData {
                name: meta.name,
                channels: meta.channels as usize,
                frames: out_frames,
                data: resampled,
            })
        })();

        resample_in_progress[pad_index].store(false, Ordering::Relaxed);

        let event = match result {
            Ok(data) => {
                let name = data.name.clone();
                *pad_data[pad_index].lock().unwrap() = Some(data);
                UiEvent::SampleLoaded { pad_index, name }
            }
            Err(message) => UiEvent::SampleError { pad_index, message },
        };

        if let Ok(mut guard) = ui_events.lock() {
            guard.push(event);
        }
        ui_dirty.store(true, Ordering::Relaxed);
    });
}

// ---------------------------------------------------------------------------
// Plugin struct
// ---------------------------------------------------------------------------

pub struct Dispatch {
    params: Arc<DispatchParams>,
    config: Arc<Mutex<Config>>,

    voices: Vec<Voice>,
    voice_seq: u64,

    pad_data: Arc<Vec<Mutex<Option<PadData>>>>,
    project_sample_rate: Arc<AtomicU32>,
    sample_rate_changed: Arc<AtomicBool>,
    max_voices: Arc<AtomicU32>,
    retrigger: Arc<AtomicBool>,

    ui_events: Arc<Mutex<Vec<UiEvent>>>,
    ui_events_dirty: Arc<AtomicBool>,

    resample_in_progress: Arc<Vec<AtomicBool>>,
}

impl Default for Dispatch {
    fn default() -> Self {
        Self {
            params: Arc::new(DispatchParams::default()),
            config: Arc::new(Mutex::new(load_config())),
            voices: Vec::new(),
            voice_seq: 0,
            pad_data: Arc::new((0..PAD_COUNT).map(|_| Mutex::new(None)).collect()),
            project_sample_rate: Arc::new(AtomicU32::new(0)),
            sample_rate_changed: Arc::new(AtomicBool::new(false)),
            max_voices: Arc::new(AtomicU32::new(16)),
            retrigger: Arc::new(AtomicBool::new(true)),
            ui_events: Arc::new(Mutex::new(Vec::new())),
            ui_events_dirty: Arc::new(AtomicBool::new(false)),
            resample_in_progress: Arc::new(
                (0..PAD_COUNT).map(|_| AtomicBool::new(false)).collect(),
            ),
        }
    }
}

impl Dispatch {
    fn trigger_pad(&mut self, pad_index: usize, velocity: f32) {
        let max_v = (self.max_voices.load(Ordering::Relaxed) as usize).min(MAX_VOICES);
        let retrigger = self.retrigger.load(Ordering::Relaxed);

        if retrigger {
            if let Some(v) = self.voices[..max_v]
                .iter_mut()
                .find(|v| v.active && v.pad_index == pad_index)
            {
                v.sample_pos = 0;
                v.velocity_gain = velocity;
                v.seq = self.voice_seq;
                self.voice_seq += 1;
                return;
            }
        }

        let active_count = self.voices[..max_v].iter().filter(|v| v.active).count();
        let slot = if active_count < max_v {
            self.voices[..max_v]
                .iter()
                .position(|v| !v.active)
                .unwrap_or(0)
        } else {
            self.voices[..max_v]
                .iter()
                .enumerate()
                .filter(|(_, v)| v.active)
                .min_by_key(|(_, v)| v.seq)
                .map(|(i, _)| i)
                .unwrap_or(0)
        };

        self.voices[slot] = Voice {
            active: true,
            pad_index,
            sample_pos: 0,
            velocity_gain: velocity,
            seq: self.voice_seq,
        };
        self.voice_seq += 1;
    }

    fn resolve_gui_url() -> &'static str {
        match std::thread::spawn(|| {
            use std::time::Duration;
            ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_millis(500))
                .timeout_read(Duration::from_millis(500))
                .build()
                .get(GUI_DEV_SERVER_PROBE_URL)
                .call()
        })
        .join()
        {
            Ok(Ok(ref r))
                if r.header("Content-Type").unwrap_or("").starts_with("text/") =>
            {
                GUI_DEV_SERVER_PROBE_URL
            }
            _ => GUI_PUBLISHED_URL,
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin trait
// ---------------------------------------------------------------------------

impl Plugin for Dispatch {
    const NAME: &'static str = "Dispatch";
    const VENDOR: &'static str = "WTH Plugins";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

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
        let sr = buffer_config.sample_rate.round() as u32;
        self.project_sample_rate.store(sr, Ordering::Relaxed);
        self.sample_rate_changed.store(true, Ordering::Relaxed);
        self.voices.resize_with(MAX_VOICES, Voice::idle);
        true
    }

    fn reset(&mut self) {
        for v in &mut self.voices {
            v.active = false;
        }
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let current_sr = context.transport().sample_rate.round() as u32;
        if current_sr > 0 && current_sr != self.project_sample_rate.load(Ordering::Relaxed) {
            self.project_sample_rate.store(current_sr, Ordering::Relaxed);
            self.sample_rate_changed.store(true, Ordering::Relaxed);
        }

        let mut events: Vec<NoteEvent<()>> = Vec::new();
        while let Some(e) = context.next_event() {
            events.push(e);
        }

        for (sample_id, channels) in buffer.iter_samples().enumerate() {
            for evt in events.iter().filter(|e| e.timing() as usize == sample_id) {
                match evt {
                    NoteEvent::NoteOn { note, velocity, .. } if *velocity > 0.0 => {
                        let n = *note;
                        if n >= PAD_MIDI_BASE {
                            let pad_idx = (n - PAD_MIDI_BASE) as usize;
                            if pad_idx < PAD_COUNT {
                                self.trigger_pad(pad_idx, *velocity);
                            }
                        }
                    }
                    _ => {}
                }
            }

            let mut out = [0.0f32; 2];
            for v in self.voices.iter_mut() {
                if !v.active {
                    continue;
                }
                if let Ok(guard) = self.pad_data[v.pad_index].try_lock() {
                    if let Some(ref data) = *guard {
                        if v.sample_pos < data.frames {
                            let fi = v.sample_pos * data.channels;
                            let l = data.data[fi] * v.velocity_gain;
                            let r = if data.channels > 1 {
                                data.data[fi + 1] * v.velocity_gain
                            } else {
                                l
                            };
                            out[0] += l;
                            out[1] += r;
                            v.sample_pos += 1;
                        } else {
                            v.active = false;
                        }
                    }
                }
            }

            let mut ch = channels.into_iter();
            if let Some(s) = ch.next() {
                *s = out[0];
            }
            if let Some(s) = ch.next() {
                *s = out[1];
            }
        }

        ProcessStatus::Normal
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let config = self.config.clone();
        let pad_data = self.pad_data.clone();
        let project_sample_rate = self.project_sample_rate.clone();
        let sample_rate_changed = self.sample_rate_changed.clone();
        let max_voices = self.max_voices.clone();
        let retrigger = self.retrigger.clone();
        let ui_events = self.ui_events.clone();
        let ui_events_dirty = self.ui_events_dirty.clone();
        let resample_in_progress = self.resample_in_progress.clone();

        let source = HTMLSource::URL(Self::resolve_gui_url());

        let editor = WebViewEditor::new(source, (GUI_WIDTH, GUI_HEIGHT))
            .with_developer_mode(true)
            .with_event_loop(move |ctx, _setter, _window| {
                while let Ok(value) = ctx.next_event() {
                    let action = match serde_json::from_value::<Action>(value) {
                        Ok(a) => a,
                        Err(e) => {
                            eprintln!("dispatch: bad UI event: {e}");
                            continue;
                        }
                    };

                    match action {
                        Action::Init => {
                            let (cache_dir, needs_cache_dir) = {
                                let cfg = config.lock().unwrap();
                                (cfg.cache_dir.clone(), cfg.cache_dir.is_none())
                            };
                            ctx.send_json(json!({
                                "type": "State",
                                "cacheDir": cache_dir,
                                "needsCacheDir": needs_cache_dir,
                                "pluginVersion": env!("CARGO_PKG_VERSION"),
                                "maxVoices": max_voices.load(Ordering::Relaxed),
                                "retrigger": retrigger.load(Ordering::Relaxed),
                            }));

                            if let Some(ref cd) = cache_dir {
                                let pr = project_sample_rate.load(Ordering::Relaxed);
                                for pad_idx in 0..PAD_COUNT {
                                    let Some(meta) = load_raw_meta(cd, pad_idx) else {
                                        continue;
                                    };
                                    ctx.send_json(json!({
                                        "type": "PadName",
                                        "padIndex": pad_idx,
                                        "name": meta.name,
                                    }));
                                    if pr == 0
                                        || resample_in_progress[pad_idx].load(Ordering::Relaxed)
                                    {
                                        continue;
                                    }
                                    if let Some(data) = load_resampled(cd, pad_idx, pr) {
                                        *pad_data[pad_idx].lock().unwrap() = Some(data);
                                        ctx.send_json(json!({
                                            "type": "SampleLoaded",
                                            "padIndex": pad_idx,
                                            "name": meta.name,
                                        }));
                                    } else {
                                        resample_in_progress[pad_idx]
                                            .store(true, Ordering::Relaxed);
                                        spawn_pad_resample(
                                            cd.clone(),
                                            pad_idx,
                                            pr,
                                            pad_data.clone(),
                                            resample_in_progress.clone(),
                                            ui_events.clone(),
                                            ui_events_dirty.clone(),
                                        );
                                    }
                                }
                            }
                        }

                        Action::SetCacheDir { path } => {
                            let mut cfg = config.lock().unwrap();
                            cfg.cache_dir = Some(path);
                            save_config(&cfg);
                            ctx.send_json(json!({
                                "type": "State",
                                "cacheDir": cfg.cache_dir,
                                "needsCacheDir": false,
                            }));
                        }

                        Action::ClearCacheDir => {
                            {
                                let mut cfg = config.lock().unwrap();
                                cfg.cache_dir = None;
                                save_config(&cfg);
                            }
                            for slot in pad_data.iter() {
                                *slot.lock().unwrap() = None;
                            }
                            ctx.send_json(json!({
                                "type": "State",
                                "cacheDir": null,
                                "needsCacheDir": true,
                            }));
                        }

                        Action::SaveSample {
                            pad_index,
                            name,
                            sample_rate,
                            channels,
                            frames,
                            data_base64,
                        } => {
                            if pad_index >= PAD_COUNT {
                                continue;
                            }

                            let decoded =
                                match general_purpose::STANDARD.decode(data_base64.as_bytes()) {
                                    Ok(d) => d,
                                    Err(e) => {
                                        ctx.send_json(json!({
                                            "type": "SampleError",
                                            "padIndex": pad_index,
                                            "message": format!("base64 decode: {e}"),
                                        }));
                                        continue;
                                    }
                                };

                            let expected = frames as usize * channels as usize * 4;
                            if decoded.len() != expected {
                                ctx.send_json(json!({
                                    "type": "SampleError",
                                    "padIndex": pad_index,
                                    "message": "sample data size mismatch",
                                }));
                                continue;
                            }

                            let mut raw: Vec<f32> = Vec::with_capacity(decoded.len() / 4);
                            for chunk in decoded.chunks_exact(4) {
                                raw.push(f32::from_le_bytes([
                                    chunk[0], chunk[1], chunk[2], chunk[3],
                                ]));
                            }

                            let cache_dir = config.lock().unwrap().cache_dir.clone();
                            let Some(ref cd) = cache_dir else {
                                ctx.send_json(json!({
                                    "type": "SampleError",
                                    "padIndex": pad_index,
                                    "message": "no cache directory set",
                                }));
                                continue;
                            };

                            if let Err(e) = save_pad_raw(
                                cd,
                                pad_index,
                                &name,
                                sample_rate,
                                channels,
                                frames,
                                &raw,
                            ) {
                                ctx.send_json(json!({
                                    "type": "SampleError",
                                    "padIndex": pad_index,
                                    "message": e,
                                }));
                                continue;
                            }

                            let pr = project_sample_rate.load(Ordering::Relaxed);
                            if pr > 0 && sample_rate == pr {
                                *pad_data[pad_index].lock().unwrap() = Some(PadData {
                                    name: name.clone(),
                                    channels: channels as usize,
                                    frames: frames as usize,
                                    data: raw,
                                });
                                ctx.send_json(json!({
                                    "type": "SampleLoaded",
                                    "padIndex": pad_index,
                                    "name": name,
                                }));
                            } else if pr > 0
                                && !resample_in_progress[pad_index].load(Ordering::Relaxed)
                            {
                                resample_in_progress[pad_index].store(true, Ordering::Relaxed);
                                spawn_pad_resample(
                                    cd.clone(),
                                    pad_index,
                                    pr,
                                    pad_data.clone(),
                                    resample_in_progress.clone(),
                                    ui_events.clone(),
                                    ui_events_dirty.clone(),
                                );
                            }
                        }

                        Action::SetPolyphony { voices } => {
                            let clamped = voices.clamp(1, MAX_VOICES as u32);
                            max_voices.store(clamped, Ordering::Relaxed);
                            ctx.send_json(json!({ "type": "State", "maxVoices": clamped }));
                        }

                        Action::SetRetrigger { enabled } => {
                            retrigger.store(enabled, Ordering::Relaxed);
                            ctx.send_json(json!({ "type": "State", "retrigger": enabled }));
                        }
                    }
                }

                // Sample rate change: re-resample all loaded pads
                if sample_rate_changed.swap(false, Ordering::Relaxed) {
                    let new_rate = project_sample_rate.load(Ordering::Relaxed);
                    if new_rate > 0 {
                        let cd = config.lock().unwrap().cache_dir.clone();
                        if let Some(ref cd) = cd {
                            for pad_idx in 0..PAD_COUNT {
                                if load_raw_meta(cd, pad_idx).is_none() {
                                    continue;
                                }
                                if resample_in_progress[pad_idx].load(Ordering::Relaxed) {
                                    continue;
                                }
                                if let Some(data) = load_resampled(cd, pad_idx, new_rate) {
                                    *pad_data[pad_idx].lock().unwrap() = Some(data);
                                } else {
                                    resample_in_progress[pad_idx].store(true, Ordering::Relaxed);
                                    spawn_pad_resample(
                                        cd.clone(),
                                        pad_idx,
                                        new_rate,
                                        pad_data.clone(),
                                        resample_in_progress.clone(),
                                        ui_events.clone(),
                                        ui_events_dirty.clone(),
                                    );
                                }
                            }
                        }
                    }
                }

                // Forward background thread events to UI
                if ui_events_dirty.swap(false, Ordering::Relaxed) {
                    let events: Vec<UiEvent> = ui_events.lock().unwrap().drain(..).collect();
                    for event in events {
                        match event {
                            UiEvent::SampleLoaded { pad_index, name } => {
                                ctx.send_json(json!({
                                    "type": "SampleLoaded",
                                    "padIndex": pad_index,
                                    "name": name,
                                }));
                            }
                            UiEvent::SampleError { pad_index, message } => {
                                ctx.send_json(json!({
                                    "type": "SampleError",
                                    "padIndex": pad_index,
                                    "message": message,
                                }));
                            }
                        }
                    }
                }
            });

        Some(Box::new(editor))
    }
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

impl Vst3Plugin for Dispatch {
    const VST3_CLASS_ID: [u8; 16] = *b"WTH_Dispatch____";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Sampler];
}

nih_export_vst3!(Dispatch);

impl ClapPlugin for Dispatch {
    const CLAP_ID: &'static str = "wthplugins.dispatch";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Visual drum rack with drag-and-drop samples");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Sampler,
        ClapFeature::Stereo,
    ];
}

nih_export_clap!(Dispatch);
