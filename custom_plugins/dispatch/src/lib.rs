use base64::{engine::general_purpose, Engine as _};
use directories::ProjectDirs;
use nih_plug::prelude::*;
use nih_plug_webview::*;
use serde::Deserialize;
use serde_json::json;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

const GUI_WIDTH: u32 = 1140;
const GUI_HEIGHT: u32 = 780;
const GUI_DEV_SERVER_PROBE_URL: &str = "http://localhost:5173/wth-dispatch";
const GUI_PUBLISHED_URL: &str = "https://dispatch-web-gui.vercel.app";

const PAD_COUNT: usize = 16;
const PAD_MIDI_BASE: u8 = 36;
/// Capacity pre-allocated for infinite polyphony to reduce runtime heap allocations.
const INFINITE_POLY_PREALLOCATE: usize = 128;
/// Maximum voice slots for finite polyphony (largest base_voices option).
const MAX_FINITE_VOICES: usize = 64;
const RESAMPLE_POINTS: usize = 64;

// ---------------------------------------------------------------------------
// Default cache directory (cross-platform)
// ---------------------------------------------------------------------------

fn default_cache_dir() -> PathBuf {
    if let Some(proj) = ProjectDirs::from("com", "WTH Plugins", "Dispatch") {
        proj.data_local_dir().join("cache")
    } else {
        // Last-resort fallback: working directory
        PathBuf::from("dispatch_cache")
    }
}

fn default_webview_userdata_dir() -> PathBuf {
    if let Some(proj) = ProjectDirs::from("com", "WTH Plugins", "Dispatch") {
        proj.data_local_dir().join("webview_userdata")
    } else {
        std::env::temp_dir().join("dispatch_webview_userdata")
    }
}

// ---------------------------------------------------------------------------
// UUID generation — 8 lowercase hex chars, collision-checked against cache dir
// ---------------------------------------------------------------------------

fn random_hex8() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::SystemTime;

    let mut h = DefaultHasher::new();
    SystemTime::now().hash(&mut h);
    std::thread::current().id().hash(&mut h);
    // Mix in a global counter for uniqueness within the same tick
    static CTR: AtomicU32 = AtomicU32::new(0);
    CTR.fetch_add(1, Ordering::Relaxed).hash(&mut h);
    format!("{:08x}", h.finish() as u32)
}

/// Generate a cache key that does not already exist as a subdirectory of `cache_dir`.
fn new_unique_cache_key(cache_dir: &std::path::Path) -> String {
    loop {
        let key = random_hex8();
        if !cache_dir.join(&key).exists() {
            return key;
        }
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
    PadCleared { pad_index: usize },
}

// ---------------------------------------------------------------------------
// Params
// ---------------------------------------------------------------------------

/// Per-pad automatable parameters.
#[derive(Params)]
struct PadChannelParams {
    #[id = "vol"]
    pub volume: FloatParam,
    /// When true, L and R are averaged before output (stereo → mono).
    #[id = "mono"]
    pub mono: BoolParam,
}

#[derive(Params)]
struct DispatchParams {
    /// Optional per-instance cache directory override. When None, the global
    /// default (platform AppData dir) is used.
    #[persist = "cache_dir"]
    pub cache_dir: Arc<Mutex<Option<String>>>,

    /// Per-pad UUID cache keys. Each slot holds the 8-char hex key that
    /// identifies the pad's data folder inside the cache directory.
    /// None means no sample has been loaded to that pad in this instance.
    #[persist = "pad_uuids"]
    pub pad_uuids: Arc<Mutex<[Option<String>; PAD_COUNT]>>,

    /// Per-pad pre-amp gain in dB (−30 to +18). Applied before the automatable
    /// volume. Persisted but not automatable so it doesn't clutter the DAW lane list.
    #[persist = "pad_preamps"]
    pub pad_preamps: Arc<Mutex<[f32; PAD_COUNT]>>,

    /// Per-pad soft-saturation toggle. When on, per-voice output is passed
    /// through a tanh soft-clipper after all gain.
    #[persist = "pad_limiters"]
    pub pad_limiters: Arc<Mutex<[bool; PAD_COUNT]>>,

    /// Per-pad custom display names. None = show the original file name.
    #[persist = "pad_custom_names"]
    pub pad_custom_names: Arc<Mutex<[Option<String>; PAD_COUNT]>>,

    /// When true the original file name is shown as a subtitle under a custom
    /// pad name in the GUI.
    #[persist = "show_original_name"]
    pub show_original_name: Arc<Mutex<bool>>,

    /// Overall output gain in dB, applied after all voices are summed.
    #[id = "master_gain"]
    pub master_gain: FloatParam,

    /// Velocity sensitivity. At 0 dB the velocity has no effect (always full
    /// volume). At −60 dB, a velocity of 0 results in roughly −60 dB of gain.
    #[id = "vel_sens"]
    pub vel_sens_db: FloatParam,

    /// One channel-strip per pad; IDs become e.g. "vol_1" … "vol_16" in the DAW.
    #[nested(array, group = "Pad")]
    pub pads: [PadChannelParams; PAD_COUNT],
}

impl Default for DispatchParams {
    fn default() -> Self {
        Self {
            cache_dir: Arc::new(Mutex::new(None)),
            pad_uuids: Arc::new(Mutex::new(std::array::from_fn(|_| None))),
            pad_preamps: Arc::new(Mutex::new([0.0f32; PAD_COUNT])),
            pad_limiters: Arc::new(Mutex::new([false; PAD_COUNT])),
            pad_custom_names: Arc::new(Mutex::new(std::array::from_fn(|_| None))),
            show_original_name: Arc::new(Mutex::new(true)),
            master_gain: FloatParam::new(
                "Master Gain",
                0.0,
                FloatRange::Linear { min: -15.0, max: 9.0 },
            )
            .with_unit(" dB"),
            vel_sens_db: FloatParam::new(
                "Velocity Sensitivity",
                -60.0,
                FloatRange::Linear { min: -60.0, max: 0.0 },
            )
            .with_unit(" dB"),
            pads: std::array::from_fn(|i| PadChannelParams {
                volume: FloatParam::new(
                    format!("Pad {} Volume", i + 1),
                    0.0,
                    FloatRange::Linear { min: -48.0, max: 6.0 },
                )
                .with_unit(" dB"),
                mono: BoolParam::new(format!("Pad {} Mono", i + 1), false),
            }),
        }
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
    SetBaseVoices {
        voices: u32,
    },
    SetAllowInfiniteVoices {
        enabled: bool,
    },
    SetRetrigger {
        enabled: bool,
    },
    SetRespectNoteOffs {
        enabled: bool,
    },
    SetPadVolume {
        #[serde(rename = "padIndex")]
        pad_index: usize,
        volume: f32,
    },
    SetPadMono {
        #[serde(rename = "padIndex")]
        pad_index: usize,
        mono: bool,
    },
    SetPadPreamp {
        #[serde(rename = "padIndex")]
        pad_index: usize,
        #[serde(rename = "preampDb")]
        preamp_db: f32,
    },
    SetPadLimiter {
        #[serde(rename = "padIndex")]
        pad_index: usize,
        limit: bool,
    },
    SetPadCustomName {
        #[serde(rename = "padIndex")]
        pad_index: usize,
        /// None / JSON null clears the custom name and reverts to the file name.
        name: Option<String>,
    },
    SetShowOriginalName {
        enabled: bool,
    },
    SetMasterGain {
        #[serde(rename = "gainDb")]
        gain_db: f32,
    },
    SetVelSens {
        #[serde(rename = "sensDb")]
        sens_db: f32,
    },
    DeletePad {
        #[serde(rename = "padIndex")]
        pad_index: usize,
    },
    /// Sent by the webview when the user presses spacebar, so we can forward
    /// it to the DAW as a real WM_KEYDOWN event (play/pause transport).
    Spacebar,
}

// ---------------------------------------------------------------------------
// Windows helper: forward spacebar to the DAW window
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn forward_spacebar_to_daw() {
    use winapi::um::winuser::{GetForegroundWindow, PostMessageW, WM_KEYDOWN, WM_KEYUP};

    // VK_SPACE = 0x20; lParam encodes: scan code 0x39 (space), repeat count 1.
    // WM_KEYUP also sets bits 30 (prev key-state) and 31 (transition state).
    const VK_SPACE: usize = 0x20;
    const LPARAM_DOWN: isize = 0x0039_0001; // scan 0x39, count 1
    const LPARAM_UP: isize = 0xC039_0001_u32 as isize; // + bits 30+31

    unsafe {
        let hwnd = GetForegroundWindow();
        if !hwnd.is_null() {
            PostMessageW(hwnd, WM_KEYDOWN, VK_SPACE, LPARAM_DOWN);
            PostMessageW(hwnd, WM_KEYUP, VK_SPACE, LPARAM_UP);
        }
    }
}

// ---------------------------------------------------------------------------
// Disk helpers
// ---------------------------------------------------------------------------

/// Resolve the effective cache directory: per-instance override if set,
/// otherwise the platform-appropriate AppData default.
fn effective_cache_dir(override_dir: &Option<String>) -> PathBuf {
    match override_dir {
        Some(s) => PathBuf::from(s),
        None => default_cache_dir(),
    }
}

fn pad_dir(cache_dir: &std::path::Path, uuid: &str) -> PathBuf {
    cache_dir.join(uuid)
}

fn save_pad_raw(
    cache_dir: &std::path::Path,
    uuid: &str,
    name: &str,
    sample_rate: u32,
    channels: u16,
    frames: u32,
    data: &[f32],
) -> Result<(), String> {
    let dir = pad_dir(cache_dir, uuid);
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

fn load_raw_meta(cache_dir: &std::path::Path, uuid: &str) -> Option<RawMeta> {
    let bytes = std::fs::read(pad_dir(cache_dir, uuid).join("sample.json")).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn load_resampled(cache_dir: &std::path::Path, uuid: &str, project_rate: u32) -> Option<PadData> {
    let dir = pad_dir(cache_dir, uuid);
    let meta_bytes = std::fs::read(dir.join("resampled.json")).ok()?;
    let meta: ResampledMeta = serde_json::from_slice(&meta_bytes).ok()?;
    let raw_meta = load_raw_meta(cache_dir, uuid)?;
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

/// Resample the raw sample for `pad_dir` to `project_rate`, write the result
/// files, and return the ready-to-play `PadData`. Runs synchronously — call
/// from `initialize()` to guarantee the pad is ready before the first buffer.
fn resample_pad_to_file(dir: &std::path::Path, project_rate: u32) -> Result<PadData, String> {
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
}

fn spawn_pad_resample(
    cache_dir: PathBuf,
    uuid: String,
    project_rate: u32,
    pad_data: Arc<Vec<Mutex<Option<PadData>>>>,
    pad_index: usize,
    resample_in_progress: Arc<Vec<AtomicBool>>,
    ui_events: Arc<Mutex<Vec<UiEvent>>>,
    ui_dirty: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        let dir = pad_dir(&cache_dir, &uuid);
        let result = resample_pad_to_file(&dir, project_rate);

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

    voices: Vec<Voice>,
    voice_seq: u64,

    pad_data: Arc<Vec<Mutex<Option<PadData>>>>,
    project_sample_rate: Arc<AtomicU32>,
    sample_rate_changed: Arc<AtomicBool>,

    /// Number of voices (16, 32, or 64) used as the cap when infinite is off.
    base_voices: Arc<AtomicU32>,
    /// When true, voices grow unboundedly. When false, base_voices is the cap.
    allow_infinite_voices: Arc<AtomicBool>,
    retrigger: Arc<AtomicBool>,
    respect_note_offs: Arc<AtomicBool>,

    ui_events: Arc<Mutex<Vec<UiEvent>>>,
    ui_events_dirty: Arc<AtomicBool>,

    resample_in_progress: Arc<Vec<AtomicBool>>,
}

impl Default for Dispatch {
    fn default() -> Self {
        Self {
            params: Arc::new(DispatchParams::default()),
            voices: Vec::new(),
            voice_seq: 0,
            pad_data: Arc::new((0..PAD_COUNT).map(|_| Mutex::new(None)).collect()),
            project_sample_rate: Arc::new(AtomicU32::new(0)),
            sample_rate_changed: Arc::new(AtomicBool::new(false)),
            base_voices: Arc::new(AtomicU32::new(16)),
            allow_infinite_voices: Arc::new(AtomicBool::new(true)),
            retrigger: Arc::new(AtomicBool::new(true)),
            respect_note_offs: Arc::new(AtomicBool::new(true)),
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
        let infinite = self.allow_infinite_voices.load(Ordering::Relaxed);
        let retrigger = self.retrigger.load(Ordering::Relaxed);

        // ── Infinite polyphony ────────────────────────────────────────────────
        if infinite {
            if retrigger {
                if let Some(v) =
                    self.voices.iter_mut().find(|v| v.active && v.pad_index == pad_index)
                {
                    v.sample_pos = 0;
                    v.velocity_gain = velocity;
                    v.seq = self.voice_seq;
                    self.voice_seq += 1;
                    return;
                }
            }
            self.voices.push(Voice {
                active: true,
                pad_index,
                sample_pos: 0,
                velocity_gain: velocity,
                seq: self.voice_seq,
            });
            self.voice_seq += 1;
            return;
        }

        // ── Finite polyphony ─────────────────────────────────────────────────
        let max_v = (self.base_voices.load(Ordering::Relaxed) as usize).min(MAX_FINITE_VOICES);

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

        // Free slot → use it; otherwise steal the oldest voice (lowest seq).
        let slot = self.voices[..max_v]
            .iter()
            .position(|v| !v.active)
            .unwrap_or_else(|| {
                self.voices[..max_v]
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, v)| v.seq)
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            });

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
        self.voices.clear();
        self.voices
            .reserve(INFINITE_POLY_PREALLOCATE.max(MAX_FINITE_VOICES));
        self.voices.resize_with(MAX_FINITE_VOICES, Voice::idle);

        // Populate pad_data immediately so audio works even if the editor is
        // never opened. Without this, pad_data stays None until the webview
        // sends Action::Init (only happens when the FX panel is opened),
        // causing silence on first project load.
        let cache_dir_override = self.params.cache_dir.lock().unwrap().clone();
        let effective_dir = effective_cache_dir(&cache_dir_override);
        let pad_uuids = self.params.pad_uuids.lock().unwrap().clone();
        for pad_idx in 0..PAD_COUNT {
            let Some(ref uuid) = pad_uuids[pad_idx] else { continue };
            if self.resample_in_progress[pad_idx].load(Ordering::Relaxed) {
                continue;
            }
            if let Some(data) = load_resampled(&effective_dir, uuid, sr) {
                *self.pad_data[pad_idx].lock().unwrap() = Some(data);
            } else if load_raw_meta(&effective_dir, uuid).is_some() {
                // No cached resample at this rate. Block here so the pad is
                // ready before the first buffer — prevents a lost note at
                // tick 0 during render (same fix as tunable_sampler).
                let dir = pad_dir(&effective_dir, uuid);
                if let Ok(data) = resample_pad_to_file(&dir, sr) {
                    *self.pad_data[pad_idx].lock().unwrap() = Some(data);
                }
            }
        }

        true
    }

    fn reset(&mut self) {
        self.voices.truncate(MAX_FINITE_VOICES);
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
        events.sort_by(|a, b| {
            a.timing().cmp(&b.timing()).then_with(|| {
                let off_first = |e: &NoteEvent<()>| match e {
                    NoteEvent::NoteOff { .. } => 0u8,
                    NoteEvent::NoteOn { velocity, .. } if *velocity == 0.0 => 0,
                    _ => 1,
                };
                off_first(a).cmp(&off_first(b))
            })
        });

        // Lock pad_preamps and pad_limiters once per buffer, not per sample.
        let pad_preamps = self.params.pad_preamps.lock().unwrap().clone();
        let pad_limiters = self.params.pad_limiters.lock().unwrap().clone();
        // Precompute velocity floor once per buffer.
        let vel_floor = 10f32.powf(self.params.vel_sens_db.value() / 20.0);
        let respect_note_offs = self.respect_note_offs.load(Ordering::Relaxed);

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
                    NoteEvent::NoteOff { note, .. } => {
                        // NoteOn with velocity==0 is sorted before NoteOn>0 by off_first, so it
                        // won't re-trigger a voice before this silences it.
                        if respect_note_offs {
                            let n = *note;
                            if n >= PAD_MIDI_BASE {
                                let pad_idx = (n - PAD_MIDI_BASE) as usize;
                                if pad_idx < PAD_COUNT {
                                    for v in self.voices.iter_mut() {
                                        if v.active && v.pad_index == pad_idx {
                                            v.active = false;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // NoteOn velocity==0 is the MIDI running-status form of note-off.
                    NoteEvent::NoteOn { note, velocity, .. } if *velocity == 0.0 => {
                        if respect_note_offs {
                            let n = *note;
                            if n >= PAD_MIDI_BASE {
                                let pad_idx = (n - PAD_MIDI_BASE) as usize;
                                if pad_idx < PAD_COUNT {
                                    for v in self.voices.iter_mut() {
                                        if v.active && v.pad_index == pad_idx {
                                            v.active = false;
                                        }
                                    }
                                }
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
                    match *guard {
                        None => {
                            v.active = false;
                        }
                        Some(ref data) => {
                            if v.sample_pos < data.frames {
                                let fi = v.sample_pos * data.channels;
                                let vol_db = self.params.pads[v.pad_index].volume.value();
                                let preamp_db = pad_preamps[v.pad_index];
                                let vol = 10f32.powf((vol_db + preamp_db) / 20.0);
                                let vel_gain = vel_floor + (1.0 - vel_floor) * v.velocity_gain;
                                let gain = vel_gain * vol;
                                let l_raw = data.data[fi] * gain;
                                let r_raw = if data.channels > 1 {
                                    data.data[fi + 1] * gain
                                } else {
                                    l_raw
                                };
                                let (l, r) = if self.params.pads[v.pad_index].mono.value() {
                                    let m = (l_raw + r_raw) * 0.5;
                                    (m, m)
                                } else {
                                    (l_raw, r_raw)
                                };
                                // Tanh soft saturation: smooth S-curve, transparent at low
                                // levels, gradually limits toward ±1 for hot signals.
                                let (l, r) = if pad_limiters[v.pad_index] {
                                    (l.tanh(), r.tanh())
                                } else {
                                    (l, r)
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
            }

            let mut ch = channels.into_iter();
            if let Some(s) = ch.next() {
                *s = out[0];
            }
            if let Some(s) = ch.next() {
                *s = out[1];
            }
        }

        let master_db = self.params.master_gain.value();
        if master_db != 0.0 {
            let master_lin = 10.0f32.powf(master_db / 20.0);
            for channel_samples in buffer.iter_samples() {
                for sample in channel_samples {
                    *sample *= master_lin;
                }
            }
        }

        // In infinite polyphony mode, reclaim finished voice slots.
        if self.allow_infinite_voices.load(Ordering::Relaxed) {
            self.voices.retain(|v| v.active);
        }

        ProcessStatus::Normal
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let pad_data = self.pad_data.clone();
        let project_sample_rate = self.project_sample_rate.clone();
        let sample_rate_changed = self.sample_rate_changed.clone();
        let base_voices = self.base_voices.clone();
        let allow_infinite_voices = self.allow_infinite_voices.clone();
        let retrigger = self.retrigger.clone();
        let respect_note_offs = self.respect_note_offs.clone();
        let ui_events = self.ui_events.clone();
        let ui_events_dirty = self.ui_events_dirty.clone();
        let resample_in_progress = self.resample_in_progress.clone();
        let params = self.params.clone();

        let source = HTMLSource::URL(Self::resolve_gui_url());

        let editor = WebViewEditor::new(source, (GUI_WIDTH, GUI_HEIGHT))
            .with_developer_mode(true)
            .with_data_directory(default_webview_userdata_dir())
            .with_event_loop(move |ctx, setter, _window| {
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
                            let cache_dir_override =
                                params.cache_dir.lock().unwrap().clone();
                            let effective_dir =
                                effective_cache_dir(&cache_dir_override);
                            let pad_uuids = params.pad_uuids.lock().unwrap().clone();
                            let pad_preamps: Vec<f32> =
                                params.pad_preamps.lock().unwrap().to_vec();
                            let pad_limiters =
                                params.pad_limiters.lock().unwrap().clone();
                            let pad_custom_names: Vec<Option<String>> =
                                params.pad_custom_names.lock().unwrap().iter().cloned().collect();
                            let show_original_name =
                                *params.show_original_name.lock().unwrap();
                            let pad_vols: Vec<f32> = (0..PAD_COUNT)
                                .map(|i| params.pads[i].volume.value())
                                .collect();
                            let pad_monos: Vec<bool> = (0..PAD_COUNT)
                                .map(|i| params.pads[i].mono.value())
                                .collect();

                            ctx.send_json(json!({
                                "type": "State",
                                "cacheDirOverride": cache_dir_override,
                                "effectiveCacheDir": effective_dir.to_string_lossy(),
                                "pluginVersion": env!("CARGO_PKG_VERSION"),
                                "baseVoices": base_voices.load(Ordering::Relaxed),
                                "allowInfiniteVoices": allow_infinite_voices.load(Ordering::Relaxed),
                                "retrigger": retrigger.load(Ordering::Relaxed),
                                "respectNoteOffs": respect_note_offs.load(Ordering::Relaxed),
                                "masterGain": params.master_gain.value(),
                                "velSensDb": params.vel_sens_db.value(),
                                "padVolumes": pad_vols,
                                "padMonos": pad_monos,
                                "padPreamps": pad_preamps,
                                "padLimiters": pad_limiters,
                                "padCustomNames": pad_custom_names,
                                "showOriginalName": show_original_name,
                            }));

                            let pr = project_sample_rate.load(Ordering::Relaxed);
                            for pad_idx in 0..PAD_COUNT {
                                let Some(ref uuid) = pad_uuids[pad_idx] else {
                                    continue;
                                };
                                let Some(meta) = load_raw_meta(&effective_dir, uuid) else {
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
                                if let Some(data) = load_resampled(&effective_dir, uuid, pr) {
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
                                        effective_dir.clone(),
                                        uuid.clone(),
                                        pr,
                                        pad_data.clone(),
                                        pad_idx,
                                        resample_in_progress.clone(),
                                        ui_events.clone(),
                                        ui_events_dirty.clone(),
                                    );
                                }
                            }
                        }

                        Action::SetCacheDir { path } => {
                            *params.cache_dir.lock().unwrap() = Some(path.clone());
                            let effective =
                                effective_cache_dir(&Some(path.clone()));
                            ctx.send_json(json!({
                                "type": "State",
                                "cacheDirOverride": path,
                                "effectiveCacheDir": effective.to_string_lossy(),
                            }));
                        }

                        Action::ClearCacheDir => {
                            *params.cache_dir.lock().unwrap() = None;
                            for slot in pad_data.iter() {
                                *slot.lock().unwrap() = None;
                            }
                            let effective = effective_cache_dir(&None);
                            ctx.send_json(json!({
                                "type": "State",
                                "cacheDirOverride": null,
                                "effectiveCacheDir": effective.to_string_lossy(),
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

                            let decoded = match general_purpose::STANDARD
                                .decode(data_base64.as_bytes())
                            {
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

                            let cache_dir_override =
                                params.cache_dir.lock().unwrap().clone();
                            let effective_dir = effective_cache_dir(&cache_dir_override);

                            // Ensure (or reuse) a UUID for this pad slot.
                            let uuid = {
                                let mut uuids = params.pad_uuids.lock().unwrap();
                                if uuids[pad_index].is_none() {
                                    uuids[pad_index] =
                                        Some(new_unique_cache_key(&effective_dir));
                                }
                                uuids[pad_index].clone().unwrap()
                            };

                            if let Err(e) = save_pad_raw(
                                &effective_dir,
                                &uuid,
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
                                && !resample_in_progress[pad_index]
                                    .load(Ordering::Relaxed)
                            {
                                resample_in_progress[pad_index]
                                    .store(true, Ordering::Relaxed);
                                spawn_pad_resample(
                                    effective_dir,
                                    uuid,
                                    pr,
                                    pad_data.clone(),
                                    pad_index,
                                    resample_in_progress.clone(),
                                    ui_events.clone(),
                                    ui_events_dirty.clone(),
                                );
                            }
                        }

                        Action::SetBaseVoices { voices } => {
                            // Only accept 16, 32, or 64; ignore anything else.
                            let clamped = match voices {
                                16 | 32 | 64 => voices,
                                _ => 16,
                            };
                            base_voices.store(clamped, Ordering::Relaxed);
                            ctx.send_json(json!({
                                "type": "State",
                                "baseVoices": clamped,
                            }));
                        }

                        Action::SetAllowInfiniteVoices { enabled } => {
                            allow_infinite_voices.store(enabled, Ordering::Relaxed);
                            ctx.send_json(json!({
                                "type": "State",
                                "allowInfiniteVoices": enabled,
                            }));
                        }

                        Action::SetRetrigger { enabled } => {
                            retrigger.store(enabled, Ordering::Relaxed);
                            ctx.send_json(json!({ "type": "State", "retrigger": enabled }));
                        }

                        Action::SetRespectNoteOffs { enabled } => {
                            respect_note_offs.store(enabled, Ordering::Relaxed);
                            ctx.send_json(json!({ "type": "State", "respectNoteOffs": enabled }));
                        }

                        Action::SetPadVolume { pad_index, volume } => {
                            if pad_index < PAD_COUNT {
                                let clamped = volume.clamp(-48.0, 6.0);
                                setter.begin_set_parameter(
                                    &params.pads[pad_index].volume,
                                );
                                setter.set_parameter(
                                    &params.pads[pad_index].volume,
                                    clamped,
                                );
                                setter.end_set_parameter(
                                    &params.pads[pad_index].volume,
                                );
                            }
                        }

                        Action::SetPadMono { pad_index, mono } => {
                            if pad_index < PAD_COUNT {
                                setter.begin_set_parameter(
                                    &params.pads[pad_index].mono,
                                );
                                setter.set_parameter(
                                    &params.pads[pad_index].mono,
                                    mono,
                                );
                                setter.end_set_parameter(
                                    &params.pads[pad_index].mono,
                                );
                            }
                        }

                        Action::SetPadPreamp { pad_index, preamp_db } => {
                            if pad_index < PAD_COUNT {
                                params.pad_preamps.lock().unwrap()[pad_index] =
                                    preamp_db.clamp(-30.0, 18.0);
                            }
                        }

                        Action::SetPadLimiter { pad_index, limit } => {
                            if pad_index < PAD_COUNT {
                                params.pad_limiters.lock().unwrap()[pad_index] = limit;
                            }
                        }

                        Action::SetPadCustomName { pad_index, name } => {
                            if pad_index < PAD_COUNT {
                                params.pad_custom_names.lock().unwrap()[pad_index] = name;
                                let names: Vec<Option<String>> =
                                    params.pad_custom_names.lock().unwrap().iter().cloned().collect();
                                ctx.send_json(json!({
                                    "type": "State",
                                    "padCustomNames": names,
                                }));
                            }
                        }

                        Action::SetShowOriginalName { enabled } => {
                            *params.show_original_name.lock().unwrap() = enabled;
                            ctx.send_json(json!({
                                "type": "State",
                                "showOriginalName": enabled,
                            }));
                        }

                        Action::SetMasterGain { gain_db } => {
                            let clamped = gain_db.clamp(-15.0, 9.0);
                            setter.begin_set_parameter(&params.master_gain);
                            setter.set_parameter(&params.master_gain, clamped);
                            setter.end_set_parameter(&params.master_gain);
                        }

                        Action::SetVelSens { sens_db } => {
                            let clamped = sens_db.clamp(-60.0, 0.0);
                            setter.begin_set_parameter(&params.vel_sens_db);
                            setter.set_parameter(&params.vel_sens_db, clamped);
                            setter.end_set_parameter(&params.vel_sens_db);
                        }

                        Action::DeletePad { pad_index } => {
                            if pad_index < PAD_COUNT {
                                *pad_data[pad_index].lock().unwrap() = None;

                                // Reset all per-pad params to defaults.
                                setter.begin_set_parameter(&params.pads[pad_index].volume);
                                setter.set_parameter(&params.pads[pad_index].volume, 0.0);
                                setter.end_set_parameter(&params.pads[pad_index].volume);
                                setter.begin_set_parameter(&params.pads[pad_index].mono);
                                setter.set_parameter(&params.pads[pad_index].mono, false);
                                setter.end_set_parameter(&params.pads[pad_index].mono);
                                params.pad_preamps.lock().unwrap()[pad_index] = 0.0;
                                params.pad_limiters.lock().unwrap()[pad_index] = false;
                                params.pad_custom_names.lock().unwrap()[pad_index] = None;

                                let cache_dir_override =
                                    params.cache_dir.lock().unwrap().clone();
                                let effective_dir =
                                    effective_cache_dir(&cache_dir_override);

                                // Remove the UUID folder from cache and clear the key.
                                let uuid = {
                                    let mut uuids = params.pad_uuids.lock().unwrap();
                                    uuids[pad_index].take()
                                };
                                if let Some(ref key) = uuid {
                                    let dir = pad_dir(&effective_dir, key);
                                    let _ = std::fs::remove_dir_all(&dir);
                                }

                                // Send the full per-pad arrays so the UI reflects
                                // the reset values immediately.
                                let pad_vols: Vec<f32> = (0..PAD_COUNT)
                                    .map(|i| params.pads[i].volume.value())
                                    .collect();
                                let pad_monos: Vec<bool> = (0..PAD_COUNT)
                                    .map(|i| params.pads[i].mono.value())
                                    .collect();
                                let pad_preamps: Vec<f32> =
                                    params.pad_preamps.lock().unwrap().to_vec();
                                let pad_limiters =
                                    params.pad_limiters.lock().unwrap().clone();
                                let pad_custom_names: Vec<Option<String>> =
                                    params.pad_custom_names.lock().unwrap().iter().cloned().collect();

                                ctx.send_json(json!({
                                    "type": "State",
                                    "padVolumes": pad_vols,
                                    "padMonos": pad_monos,
                                    "padPreamps": pad_preamps,
                                    "padLimiters": pad_limiters,
                                    "padCustomNames": pad_custom_names,
                                }));
                                ctx.send_json(json!({
                                    "type": "PadCleared",
                                    "padIndex": pad_index,
                                }));
                            }
                        }

                        Action::Spacebar => {
                            #[cfg(windows)]
                            forward_spacebar_to_daw();
                        }
                    }
                }

                // Sample rate change: re-resample all loaded pads
                if sample_rate_changed.swap(false, Ordering::Relaxed) {
                    let new_rate = project_sample_rate.load(Ordering::Relaxed);
                    if new_rate > 0 {
                        let cache_dir_override =
                            params.cache_dir.lock().unwrap().clone();
                        let effective_dir = effective_cache_dir(&cache_dir_override);
                        let pad_uuids = params.pad_uuids.lock().unwrap().clone();
                        for pad_idx in 0..PAD_COUNT {
                            let Some(ref uuid) = pad_uuids[pad_idx] else {
                                continue;
                            };
                            if load_raw_meta(&effective_dir, uuid).is_none() {
                                continue;
                            }
                            if resample_in_progress[pad_idx].load(Ordering::Relaxed) {
                                continue;
                            }
                            if let Some(data) = load_resampled(&effective_dir, uuid, new_rate) {
                                *pad_data[pad_idx].lock().unwrap() = Some(data);
                            } else {
                                resample_in_progress[pad_idx]
                                    .store(true, Ordering::Relaxed);
                                spawn_pad_resample(
                                    effective_dir.clone(),
                                    uuid.clone(),
                                    new_rate,
                                    pad_data.clone(),
                                    pad_idx,
                                    resample_in_progress.clone(),
                                    ui_events.clone(),
                                    ui_events_dirty.clone(),
                                );
                            }
                        }
                    }
                }

                // Forward background thread events to UI
                if ui_events_dirty.swap(false, Ordering::Relaxed) {
                    let events: Vec<UiEvent> =
                        ui_events.lock().unwrap().drain(..).collect();
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
                            UiEvent::PadCleared { pad_index } => {
                                ctx.send_json(json!({
                                    "type": "PadCleared",
                                    "padIndex": pad_index,
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
