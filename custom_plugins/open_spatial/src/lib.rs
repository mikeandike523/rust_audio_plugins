mod remote_logging;

use nih_plug::prelude::*;
use nih_plug_webview::*;
use remote_logging::RemoteLogger;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sofar::reader::{Filter, OpenOptions, Sofar};
use sofar::render::Renderer;
use std::f32::consts::PI;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// GUI / infrastructure constants
// ---------------------------------------------------------------------------

const GUI_WIDTH: u32 = 1080;
const GUI_HEIGHT: u32 = 1080;
const GUI_DEV_SERVER_PROBE_URLS: [&str; 2] = [
    "http://localhost:5173/open-spatial",
    "http://localhost:4173/open-spatial",
];
const GUI_PUBLISHED_URL: &str = "https://open-spatial-web-gui.vercel.app";
const METER_UPDATE_SECONDS: f32 = 0.1;
const ANALYTIC_RENDERER_ID: &str = "sofa-runtime-fetch-v1";
const ASSET_MANIFEST_FILE: &str = "asset_manifest.json";
const CACHE_NAMESPACE: &str = "wth_plugins/open_spatial";
const DOWNLOAD_BUFFER_BYTES: usize = 64 * 1024;
const HRTF_PARTITION_LEN: usize = 64;

// ---------------------------------------------------------------------------
// SOFA file definitions — both entries are on the same Zenodo record
// ---------------------------------------------------------------------------

struct SofaDefinition {
    key: &'static str,
    name: &'static str,
    url: &'static str,
    filename: &'static str,
}

const SOFA_DEFINITIONS: &[SofaDefinition] = &[
    SofaDefinition {
        key: "HRIR_FULL2DEG",
        name: "HRIR Full 2° (AES69-2022)",
        url: "https://zenodo.org/records/3928297/files/HRIR_FULL2DEG.sofa?download=1",
        filename: "HRIR_FULL2DEG.sofa",
    },
    SofaDefinition {
        key: "HRIR_L2354",
        name: "HRIR L2354 (IRCAM)",
        url: "https://zenodo.org/records/3928297/files/HRIR_L2354.sofa?download=1",
        filename: "HRIR_L2354.sofa",
    },
];

const DEFAULT_SOFA_KEY: &str = "HRIR_FULL2DEG";

fn sofa_definition_for_key(key: &str) -> &'static SofaDefinition {
    SOFA_DEFINITIONS
        .iter()
        .find(|d| d.key == key)
        .unwrap_or(&SOFA_DEFINITIONS[0])
}

// ---------------------------------------------------------------------------
// Wire-protocol types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AssetManifest {
    schema_version: u32,
    renderer_id: String,
    hrtf_url: String,
    hrtf_filename: String,
    sha256_hex: String,
    bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeInitStatus {
    stage: String,
    message: String,
    progress: Option<f32>,
    downloaded_bytes: Option<u64>,
    total_bytes: Option<u64>,
    cache_path: String,
    hrtf_path: String,
    hrtf_url: String,
    ready: bool,
    file_ready: bool,
}

impl Default for RuntimeInitStatus {
    fn default() -> Self {
        let def = sofa_definition_for_key(DEFAULT_SOFA_KEY);
        Self {
            stage: "idle".to_string(),
            message: "Waiting for initialization".to_string(),
            progress: None,
            downloaded_bytes: None,
            total_bytes: None,
            cache_path: String::new(),
            hrtf_path: String::new(),
            hrtf_url: def.url.to_string(),
            ready: false,
            file_ready: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Parameters
// ---------------------------------------------------------------------------

#[derive(Params)]
struct OpenSpatialParams {
    // --- Spatial position ---
    #[id = "azimth"]
    azimuth: FloatParam,
    #[id = "elevtn"]
    elevation: FloatParam,
    #[id = "distnc"]
    distance: FloatParam,
    #[id = "srcyaw"]
    source_yaw: FloatParam,
    #[id = "towrds"]
    always_towards_head: BoolParam,
    #[id = "direct"]
    directivity: FloatParam,
    #[id = "outgn"]
    output_gain: FloatParam,
    #[id = "radmul"]
    radial_multiply: FloatParam,

    // --- Pinna pre-filter ---
    #[id = "pnaen0"]
    pinna_enabled: BoolParam,
    #[id = "pnafrq"]
    pinna_freq: FloatParam,
    #[id = "pnagdb"]
    pinna_gain_db: FloatParam,
    #[id = "pnaqq0"]
    pinna_q: FloatParam,

    // --- HRTF engine ---
    #[id = "hrtfin"]
    hrtf_interpolate: BoolParam,
    #[id = "itdenb"]
    itd_enabled: BoolParam,

    // --- Distance model ---
    #[id = "distex"]
    distance_exponent: FloatParam,
    #[id = "distmn"]
    distance_min_m: FloatParam,

    // --- Directivity model ---
    #[id = "dirflo"]
    directivity_floor: FloatParam,
    #[id = "dirrng"]
    directivity_range: FloatParam,
    #[id = "direxs"]
    directivity_exp_scale: FloatParam,

    // --- Room / Reverb ---
    #[id = "rvben0"]
    reverb_enabled: BoolParam,
    #[id = "rvbwet"]
    reverb_wet: FloatParam,
    #[id = "rvbrsz"]
    reverb_room_size: FloatParam,
    #[id = "rvbprd"]
    reverb_pre_delay_ms: FloatParam,
    #[id = "rvbdmp"]
    reverb_damping: FloatParam,

    // --- Persisted non-audio state ---
    #[persist = "asset_cache_dir"]
    asset_cache_dir: Arc<Mutex<String>>,
    #[persist = "sofa_selection"]
    sofa_selection: Arc<Mutex<String>>,

    params_dirty: Arc<AtomicBool>,
}

impl Default for OpenSpatialParams {
    fn default() -> Self {
        let params_dirty = Arc::new(AtomicBool::new(false));

        macro_rules! dirty_cb {
            ($arc:expr) => {{
                let d = $arc.clone();
                Arc::new(move |_| { d.store(true, Ordering::Relaxed); })
            }};
        }

        Self {
            // --- Spatial ---
            azimuth: FloatParam::new(
                "Azimuth",
                30.0,
                FloatRange::Linear { min: -180.0, max: 180.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_step_size(0.1)
            .with_unit(" deg")
            .with_callback(dirty_cb!(params_dirty)),

            elevation: FloatParam::new(
                "Elevation",
                0.0,
                FloatRange::Linear { min: -90.0, max: 90.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_step_size(0.1)
            .with_unit(" deg")
            .with_callback(dirty_cb!(params_dirty)),

            distance: FloatParam::new(
                "Distance",
                1.5,
                FloatRange::Skewed { min: 1.0, max: 30.0, factor: FloatRange::skew_factor(3.0) },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_step_size(0.01)
            .with_unit(" m")
            .with_callback(dirty_cb!(params_dirty)),

            source_yaw: FloatParam::new(
                "Source Yaw",
                0.0,
                FloatRange::Linear { min: -180.0, max: 180.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_step_size(0.1)
            .with_unit(" deg")
            .with_callback(dirty_cb!(params_dirty)),

            always_towards_head: BoolParam::new("Always Towards Head", true)
                .with_callback(dirty_cb!(params_dirty)),

            directivity: FloatParam::new(
                "Directivity",
                0.65,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_step_size(0.01)
            .with_callback(dirty_cb!(params_dirty)),

            output_gain: FloatParam::new(
                "Output Gain",
                -3.0,
                FloatRange::Linear { min: -24.0, max: 12.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_step_size(0.1)
            .with_unit(" dB")
            .with_callback(dirty_cb!(params_dirty)),

            radial_multiply: FloatParam::new(
                "Radial Multiply",
                1.0,
                FloatRange::Linear { min: -1.0, max: 1.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_step_size(0.01)
            .with_callback(dirty_cb!(params_dirty)),

            // --- Pinna pre-filter ---
            pinna_enabled: BoolParam::new("Pinna Filter Enable", true)
                .with_callback(dirty_cb!(params_dirty)),

            pinna_freq: FloatParam::new(
                "Pinna Freq",
                8000.0,
                FloatRange::Skewed { min: 2000.0, max: 16000.0, factor: FloatRange::skew_factor(2.0) },
            )
            .with_smoother(SmoothingStyle::Linear(30.0))
            .with_step_size(10.0)
            .with_unit(" Hz")
            .with_callback(dirty_cb!(params_dirty)),

            pinna_gain_db: FloatParam::new(
                "Pinna Gain",
                4.0,
                FloatRange::Linear { min: -12.0, max: 12.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_step_size(0.1)
            .with_unit(" dB")
            .with_callback(dirty_cb!(params_dirty)),

            pinna_q: FloatParam::new(
                "Pinna Q",
                0.88,
                FloatRange::Skewed { min: 0.1, max: 8.0, factor: FloatRange::skew_factor(2.0) },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_step_size(0.01)
            .with_callback(dirty_cb!(params_dirty)),

            // --- HRTF engine ---
            hrtf_interpolate: BoolParam::new("HRTF Interpolation", true)
                .with_callback(dirty_cb!(params_dirty)),

            itd_enabled: BoolParam::new("ITD Delays", true)
                .with_callback(dirty_cb!(params_dirty)),

            // --- Distance model ---
            distance_exponent: FloatParam::new(
                "Distance Exponent",
                1.0,
                FloatRange::Linear { min: 0.0, max: 2.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_step_size(0.01)
            .with_callback(dirty_cb!(params_dirty)),

            distance_min_m: FloatParam::new(
                "Distance Min",
                1.0,
                FloatRange::Skewed { min: 0.1, max: 5.0, factor: FloatRange::skew_factor(2.0) },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_step_size(0.01)
            .with_unit(" m")
            .with_callback(dirty_cb!(params_dirty)),

            // --- Directivity model ---
            directivity_floor: FloatParam::new(
                "Directivity Floor",
                0.15,
                FloatRange::Linear { min: 0.0, max: 0.5 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_step_size(0.01)
            .with_callback(dirty_cb!(params_dirty)),

            directivity_range: FloatParam::new(
                "Directivity Range",
                0.85,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_step_size(0.01)
            .with_callback(dirty_cb!(params_dirty)),

            directivity_exp_scale: FloatParam::new(
                "Directivity Exp Scale",
                3.0,
                FloatRange::Skewed { min: 0.5, max: 10.0, factor: FloatRange::skew_factor(2.0) },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_step_size(0.05)
            .with_callback(dirty_cb!(params_dirty)),

            // --- Reverb ---
            reverb_enabled: BoolParam::new("Reverb Enable", false)
                .with_callback(dirty_cb!(params_dirty)),

            reverb_wet: FloatParam::new(
                "Reverb Wet",
                0.15,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_step_size(0.01)
            .with_callback(dirty_cb!(params_dirty)),

            reverb_room_size: FloatParam::new(
                "Room Size",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_smoother(SmoothingStyle::Linear(50.0))
            .with_step_size(0.01)
            .with_callback(dirty_cb!(params_dirty)),

            reverb_pre_delay_ms: FloatParam::new(
                "Reverb Pre-Delay",
                20.0,
                FloatRange::Linear { min: 0.0, max: 100.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_step_size(0.5)
            .with_unit(" ms")
            .with_callback(dirty_cb!(params_dirty)),

            reverb_damping: FloatParam::new(
                "Reverb Damping",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_step_size(0.01)
            .with_callback(dirty_cb!(params_dirty)),

            // --- Persisted ---
            asset_cache_dir: Arc::new(Mutex::new(String::new())),
            sofa_selection: Arc::new(Mutex::new(DEFAULT_SOFA_KEY.to_string())),

            params_dirty,
        }
    }
}

// ---------------------------------------------------------------------------
// Vec3 / SpatialPose
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct Vec3 {
    x: f32,
    y: f32,
    z: f32,
}

impl Vec3 {
    fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    fn norm(self) -> f32 {
        self.dot(self).sqrt()
    }

    fn normalized(self) -> Self {
        let length = self.norm().max(1.0e-6);
        Self {
            x: self.x / length,
            y: self.y / length,
            z: self.z / length,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct SpatialPose {
    position: Vec3,
}

impl SpatialPose {
    /// Azimuth/elevation in degrees, distance in metres.
    /// Caller is responsible for clamping distance to distance_min_m before calling;
    /// a 1 cm hard floor is enforced here as safety against division-by-zero.
    fn from_params(azimuth_deg: f32, elevation_deg: f32, distance_m: f32) -> Self {
        let azimuth_rad = azimuth_deg.to_radians();
        let elevation_rad = elevation_deg.to_radians();
        let distance = distance_m.max(0.01);
        let horizontal = distance * elevation_rad.cos();
        Self {
            position: Vec3 {
                x: horizontal * azimuth_rad.cos(),
                y: horizontal * azimuth_rad.sin(),
                z: distance * elevation_rad.sin(),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// DelayLine — integer-sample delay, used for ITD
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct DelayLine {
    buf: Vec<f32>,
    delay_samples: usize,
    read_pos: usize,
    write_pos: usize,
}

impl DelayLine {
    fn new() -> Self {
        Self { buf: vec![0.0], delay_samples: 0, read_pos: 0, write_pos: 0 }
    }

    fn set_delay_samples(&mut self, delay_samples: usize) {
        self.delay_samples = delay_samples;
        if self.buf.len() < delay_samples + 1 {
            self.buf.resize(delay_samples + 1, 0.0);
        }
        if delay_samples == 0 {
            self.read_pos = self.write_pos;
        } else if self.write_pos >= delay_samples {
            self.read_pos = self.write_pos - delay_samples;
        } else {
            self.read_pos = self.buf.len() + self.write_pos - delay_samples;
        }
    }

    fn apply(&mut self, samples: &mut [f32]) {
        if self.delay_samples == 0 {
            return;
        }
        for sample in samples {
            self.buf[self.write_pos] = *sample;
            self.write_pos = (self.write_pos + 1) % self.buf.len();
            *sample = self.buf[self.read_pos];
            self.read_pos = (self.read_pos + 1) % self.buf.len();
        }
    }
}

// ---------------------------------------------------------------------------
// BiquadFilter — peaking EQ (Direct Form I), Audio EQ Cookbook
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct BiquadFilter {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl BiquadFilter {
    fn new_peaking_eq(sample_rate: f32, freq: f32, gain_db: f32, q: f32) -> Self {
        let mut f = Self { b0: 0.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0, x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0 };
        f.update_coefficients(sample_rate, freq, gain_db, q);
        f
    }

    /// Update coefficients without resetting state — glitch-free parameter change.
    fn update_coefficients(&mut self, sample_rate: f32, freq: f32, gain_db: f32, q: f32) {
        let a = 10.0_f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * freq / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q.max(1e-4));
        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha / a;
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }

    fn process(&mut self, samples: &mut [f32]) {
        for x in samples.iter_mut() {
            let y = self.b0 * *x + self.b1 * self.x1 + self.b2 * self.x2
                - self.a1 * self.y1
                - self.a2 * self.y2;
            self.x2 = self.x1;
            self.x1 = *x;
            self.y2 = self.y1;
            self.y1 = y;
            *x = y;
        }
    }
}

// ---------------------------------------------------------------------------
// Freeverb-style reverb
//   8 comb filters (L and R, stereo-spread +23 samples) in parallel,
//   2 allpass filters (L and R) in series.
//   Reference: Jezar's Freeverb.
// ---------------------------------------------------------------------------

const COMB_TUNING: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_TUNING: [usize; 2] = [556, 441];
const STEREO_SPREAD: usize = 23;
/// Fixed pre-gain to prevent overload when summing 8 parallel comb filters.
const REVERB_INPUT_GAIN: f32 = 0.015;
const ALLPASS_FEEDBACK: f32 = 0.5;

#[derive(Clone)]
struct CombFilter {
    buf: Vec<f32>,
    pos: usize,
    feedback: f32,
    filterstore: f32,
    /// low-pass coefficient applied to filterstore (Freeverb damp1 = damping * 0.4)
    damp1: f32,
    /// complement: damp2 = 1.0 - damp1
    damp2: f32,
}

impl CombFilter {
    fn new(size: usize) -> Self {
        Self {
            buf: vec![0.0; size.max(1)],
            pos: 0,
            feedback: 0.84,
            filterstore: 0.0,
            damp1: 0.2,
            damp2: 0.8,
        }
    }

    fn set_params(&mut self, feedback: f32, damp1: f32, damp2: f32) {
        self.feedback = feedback;
        self.damp1 = damp1;
        self.damp2 = damp2;
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let output = self.buf[self.pos];
        // Freeverb low-pass: filter output before feeding back
        self.filterstore = output * self.damp2 + self.filterstore * self.damp1;
        self.buf[self.pos] = input + self.filterstore * self.feedback;
        self.pos = (self.pos + 1) % self.buf.len();
        output
    }
}

#[derive(Clone)]
struct AllpassFilter {
    buf: Vec<f32>,
    pos: usize,
}

impl AllpassFilter {
    fn new(size: usize) -> Self {
        Self { buf: vec![0.0; size.max(1)], pos: 0 }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let bufout = self.buf[self.pos];
        let output = bufout - input;
        self.buf[self.pos] = input + bufout * ALLPASS_FEEDBACK;
        self.pos = (self.pos + 1) % self.buf.len();
        output
    }
}

struct Reverb {
    combs_l: [CombFilter; 8],
    combs_r: [CombFilter; 8],
    allpasses_l: [AllpassFilter; 2],
    allpasses_r: [AllpassFilter; 2],
    pre_delay: DelayLine,
}

impl Reverb {
    fn new(sample_rate: f32) -> Self {
        let scale = sample_rate / 44100.0;
        let sz = |base: usize, spread: usize| -> usize {
            ((base + spread) as f32 * scale).round() as usize
        };
        let combs_l = std::array::from_fn(|i| CombFilter::new(sz(COMB_TUNING[i], 0)));
        let combs_r = std::array::from_fn(|i| CombFilter::new(sz(COMB_TUNING[i], STEREO_SPREAD)));
        let allpasses_l = std::array::from_fn(|i| AllpassFilter::new(sz(ALLPASS_TUNING[i], 0)));
        let allpasses_r = std::array::from_fn(|i| AllpassFilter::new(sz(ALLPASS_TUNING[i], STEREO_SPREAD)));
        Self {
            combs_l,
            combs_r,
            allpasses_l,
            allpasses_r,
            pre_delay: DelayLine::new(),
        }
    }

    /// Update time-varying parameters.  Called once per process block.
    fn update_params(&mut self, room_size: f32, damping: f32, pre_delay_ms: f32, sample_rate: f32) {
        // room_size [0,1] → feedback [0.70, 0.98]
        let feedback = (room_size * 0.28 + 0.70_f32).clamp(0.0, 0.98);
        // damping [0,1] → Freeverb damp1 [0, 0.4]
        let damp1 = (damping * 0.4_f32).clamp(0.0, 0.4);
        let damp2 = 1.0 - damp1;
        for c in self.combs_l.iter_mut().chain(self.combs_r.iter_mut()) {
            c.set_params(feedback, damp1, damp2);
        }
        let pre_delay_samples = (pre_delay_ms * 0.001 * sample_rate).round() as usize;
        self.pre_delay.set_delay_samples(pre_delay_samples);
    }

    /// Process one input sample, return (left_out, right_out).
    #[inline]
    fn process_sample(&mut self, input: f32) -> (f32, f32) {
        let mut buf = [input];
        self.pre_delay.apply(&mut buf);
        let scaled = buf[0] * REVERB_INPUT_GAIN;

        let mut out_l = 0.0_f32;
        let mut out_r = 0.0_f32;
        for c in &mut self.combs_l { out_l += c.process(scaled); }
        for c in &mut self.combs_r { out_r += c.process(scaled); }
        for ap in &mut self.allpasses_l { out_l = ap.process(out_l); }
        for ap in &mut self.allpasses_r { out_r = ap.process(out_r); }
        (out_l, out_r)
    }
}

// ---------------------------------------------------------------------------
// RenderParams — snapshot of all DSP parameters for one block
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct RenderParams {
    source_yaw_deg: f32,
    always_towards_head: bool,
    directivity_amount: f32,
    output_gain_db: f32,
    radial_multiply: f32,
    // Pinna pre-filter
    pinna_enabled: bool,
    pinna_freq: f32,
    pinna_gain_db: f32,
    pinna_q: f32,
    // HRTF engine
    hrtf_interpolate: bool,
    itd_enabled: bool,
    // Distance model
    distance_exponent: f32,
    distance_min_m: f32,
    // Directivity model
    directivity_floor: f32,
    directivity_range: f32,
    directivity_exp_scale: f32,
    // Reverb
    reverb_enabled: bool,
    reverb_wet: f32,
    reverb_room_size: f32,
    reverb_pre_delay_ms: f32,
    reverb_damping: f32,
}

// ---------------------------------------------------------------------------
// HrtfEngine
// ---------------------------------------------------------------------------

struct HrtfEngine {
    sofa: Sofar,
    filter: Filter,
    renderer: Renderer,
    sample_rate: f32,
    pinna_filter: BiquadFilter,
    reverb: Reverb,
    mono_input: Vec<f32>,
    pre_pinna_buf: Vec<f32>,
    output_left: Vec<f32>,
    output_right: Vec<f32>,
    chunk_input: Vec<f32>,
    chunk_output_left: Vec<f32>,
    chunk_output_right: Vec<f32>,
    pending_input: Vec<f32>,
    pending_output_left: Vec<f32>,
    pending_output_right: Vec<f32>,
    left_delay: DelayLine,
    right_delay: DelayLine,
}

impl HrtfEngine {
    fn load(path: &Path, sample_rate: f32) -> Result<Self, String> {
        let sofa = OpenOptions::new()
            .sample_rate(sample_rate)
            .open(path)
            .map_err(|err| format!("Failed to open SOFA file: {err}"))?;

        let filter = Filter::new(sofa.filter_len());
        let renderer = Renderer::builder(sofa.filter_len())
            .with_sample_rate(sample_rate)
            .with_partition_len(HRTF_PARTITION_LEN)
            .build()
            .map_err(|err| format!("Failed to build HRTF renderer: {err}"))?;

        // Default pinna filter; coefficients are refreshed every block.
        let pinna_filter = BiquadFilter::new_peaking_eq(sample_rate, 8000.0, 4.0, 0.88);

        Ok(Self {
            sofa,
            filter,
            renderer,
            sample_rate,
            pinna_filter,
            reverb: Reverb::new(sample_rate),
            mono_input: Vec::new(),
            pre_pinna_buf: Vec::new(),
            output_left: Vec::new(),
            output_right: Vec::new(),
            chunk_input: vec![0.0; HRTF_PARTITION_LEN],
            chunk_output_left: vec![0.0; HRTF_PARTITION_LEN],
            chunk_output_right: vec![0.0; HRTF_PARTITION_LEN],
            pending_input: Vec::new(),
            pending_output_left: Vec::new(),
            pending_output_right: Vec::new(),
            left_delay: DelayLine::new(),
            right_delay: DelayLine::new(),
        })
    }

    fn ensure_buffer_len(&mut self, len: usize) {
        if self.mono_input.len() != len {
            self.mono_input.resize(len, 0.0);
            self.pre_pinna_buf.resize(len, 0.0);
            self.output_left.resize(len, 0.0);
            self.output_right.resize(len, 0.0);
        }
    }

    fn render_block(
        &mut self,
        mono_input: &[f32],
        pose: SpatialPose,
        rp: &RenderParams,
        out_left: &mut [f32],
        out_right: &mut [f32],
    ) -> Result<(), String> {
        self.ensure_buffer_len(mono_input.len());
        self.output_left.fill(0.0);
        self.output_right.fill(0.0);

        // --- Build source direction vectors ---
        let direction = pose.position.normalized();
        let to_listener = Vec3 { x: -direction.x, y: -direction.y, z: -direction.z };
        let source_forward = if rp.always_towards_head {
            to_listener
        } else {
            let yaw_rad = rp.source_yaw_deg.to_radians();
            Vec3 { x: yaw_rad.cos(), y: yaw_rad.sin(), z: 0.0 }
        };
        let alignment = source_forward.dot(to_listener).clamp(-1.0, 1.0);
        let cardioid = 0.5 * (1.0 + alignment);

        // Directivity gain with exposed floor / range / exp_scale constants.
        let base = (rp.directivity_floor + rp.directivity_range * cardioid).clamp(0.0, 1.0);
        let directivity_gain = lerp(
            1.0,
            base.powf(1.0 + rp.directivity_amount * rp.directivity_exp_scale),
            rp.directivity_amount.clamp(0.0, 1.0),
        );

        // --- Distance gain: 1 / d^exponent ---
        // Radial multiply scales the horizontal (XY) plane; z (elevation) is unchanged.
        let sofa_x = pose.position.x * rp.radial_multiply;
        let sofa_y = pose.position.y * rp.radial_multiply;
        let sofa_z = pose.position.z;
        let effective_distance = (sofa_x * sofa_x + sofa_y * sofa_y + sofa_z * sofa_z)
            .sqrt()
            .max(rp.distance_min_m.max(0.01));
        let distance_gain = effective_distance.powf(-rp.distance_exponent.max(0.0));

        let input_gain = util::db_to_gain_fast(rp.output_gain_db) * directivity_gain * distance_gain;

        // Scale input; save pre-pinna copy for reverb.
        for (i, src) in mono_input.iter().enumerate() {
            let v = src * input_gain;
            self.mono_input[i] = v;
            self.pre_pinna_buf[i] = v;
        }

        // --- Pinna pre-filter (tunable peaking EQ) ---
        if rp.pinna_enabled {
            self.pinna_filter.update_coefficients(
                self.sample_rate,
                rp.pinna_freq,
                rp.pinna_gain_db,
                rp.pinna_q,
            );
            self.pinna_filter.process(&mut self.mono_input);
        }

        // --- HRTF convolution ---
        // SOFA/libmysofa: +X = front, +Y = listener-left, +Z = up.
        // Plugin: +X = front, +Y = listener-right → negate Y.
        if rp.hrtf_interpolate {
            self.sofa.filter(sofa_x, -sofa_y, sofa_z, &mut self.filter);
        } else {
            self.sofa.filter_nointerp(sofa_x, -sofa_y, sofa_z, &mut self.filter);
        }
        self.renderer
            .set_filter(&self.filter)
            .map_err(|err| format!("Failed to update HRTF renderer filter: {err}"))?;

        self.pending_input.extend_from_slice(&self.mono_input);

        while self.pending_input.len() >= HRTF_PARTITION_LEN {
            self.chunk_input.copy_from_slice(&self.pending_input[..HRTF_PARTITION_LEN]);
            self.pending_input.drain(..HRTF_PARTITION_LEN);

            self.renderer
                .process_block(
                    &self.chunk_input,
                    &mut self.chunk_output_left,
                    &mut self.chunk_output_right,
                )
                .map_err(|err| format!("Failed to process HRTF block: {err}"))?;

            // ITD: apply integer-sample ITD delays from the SOFA filter.
            // FIX: use round() instead of truncation (floor) for accurate ITD.
            if rp.itd_enabled {
                self.left_delay.set_delay_samples(
                    (self.filter.ldelay * self.sample_rate).round() as usize,
                );
                self.right_delay.set_delay_samples(
                    (self.filter.rdelay * self.sample_rate).round() as usize,
                );
                self.left_delay.apply(&mut self.chunk_output_left);
                self.right_delay.apply(&mut self.chunk_output_right);
            }

            self.pending_output_left.extend_from_slice(&self.chunk_output_left);
            self.pending_output_right.extend_from_slice(&self.chunk_output_right);
        }

        let needed = mono_input.len();
        let available = self
            .pending_output_left
            .len()
            .min(self.pending_output_right.len())
            .min(needed);

        if available > 0 {
            self.output_left[..available]
                .copy_from_slice(&self.pending_output_left[..available]);
            self.output_right[..available]
                .copy_from_slice(&self.pending_output_right[..available]);
            self.pending_output_left.drain(..available);
            self.pending_output_right.drain(..available);
        }

        out_left.copy_from_slice(&self.output_left);
        out_right.copy_from_slice(&self.output_right);

        // --- Reverb (Freeverb, diffuse field, added on top of HRTF output) ---
        if rp.reverb_enabled && rp.reverb_wet > 0.0 {
            self.reverb.update_params(
                rp.reverb_room_size,
                rp.reverb_damping,
                rp.reverb_pre_delay_ms,
                self.sample_rate,
            );
            for i in 0..needed {
                let (rev_l, rev_r) = self.reverb.process_sample(self.pre_pinna_buf[i]);
                out_left[i] += rev_l * rp.reverb_wet;
                out_right[i] += rev_r * rp.reverb_wet;
            }
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Background task
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub enum OpenSpatialTask {
    EnsureHrtfCache,
}

// ---------------------------------------------------------------------------
// Plugin struct
// ---------------------------------------------------------------------------

pub struct OpenSpatial {
    params: Arc<OpenSpatialParams>,
    remote_logger: RemoteLogger,
    sample_rate: f32,
    meter_interval_samples: usize,
    meter_samples_remaining: usize,
    meter_input_peak_l: f32,
    meter_input_peak_r: f32,
    meter_output_peak_l: f32,
    meter_output_peak_r: f32,
    meter_input_l: Arc<AtomicF32>,
    meter_input_r: Arc<AtomicF32>,
    meter_output_l: Arc<AtomicF32>,
    meter_output_r: Arc<AtomicF32>,
    meter_dirty: Arc<AtomicBool>,
    runtime_status: Arc<Mutex<RuntimeInitStatus>>,
    runtime_dirty: Arc<AtomicBool>,
    task_running: Arc<AtomicBool>,
    hrtf_engine: Option<HrtfEngine>,
    /// Set by the editor when the SOFA selection changes; checked in process().
    hrtf_reload_needed: Arc<AtomicBool>,
}

impl Default for OpenSpatial {
    fn default() -> Self {
        Self {
            params: Arc::new(OpenSpatialParams::default()),
            remote_logger: RemoteLogger::new(9099),
            sample_rate: 44100.0,
            meter_interval_samples: 4410,
            meter_samples_remaining: 4410,
            meter_input_peak_l: 0.0,
            meter_input_peak_r: 0.0,
            meter_output_peak_l: 0.0,
            meter_output_peak_r: 0.0,
            meter_input_l: Arc::new(AtomicF32::new(0.0)),
            meter_input_r: Arc::new(AtomicF32::new(0.0)),
            meter_output_l: Arc::new(AtomicF32::new(0.0)),
            meter_output_r: Arc::new(AtomicF32::new(0.0)),
            meter_dirty: Arc::new(AtomicBool::new(false)),
            runtime_status: Arc::new(Mutex::new(RuntimeInitStatus::default())),
            runtime_dirty: Arc::new(AtomicBool::new(true)),
            task_running: Arc::new(AtomicBool::new(false)),
            hrtf_engine: None,
            hrtf_reload_needed: Arc::new(AtomicBool::new(false)),
        }
    }
}

// ---------------------------------------------------------------------------
// GUI → plugin actions
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Action {
    Init,
    // Spatial
    SetAzimuth { value: f32 },
    SetElevation { value: f32 },
    SetDistance { value: f32 },
    SetSourceYaw { value: f32 },
    SetAlwaysTowardsHead { value: bool },
    SetDirectivity { value: f32 },
    SetOutputGain { value: f32 },
    SetRadialMultiply { value: f32 },
    // Pinna
    SetPinnaEnabled { value: bool },
    SetPinnaFreq { value: f32 },
    SetPinnaGainDb { value: f32 },
    SetPinnaQ { value: f32 },
    // HRTF engine
    SetHrtfInterpolate { value: bool },
    SetItdEnabled { value: bool },
    // Distance model
    SetDistanceExponent { value: f32 },
    SetDistanceMinM { value: f32 },
    // Directivity model
    SetDirectivityFloor { value: f32 },
    SetDirectivityRange { value: f32 },
    SetDirectivityExpScale { value: f32 },
    // Reverb
    SetReverbEnabled { value: bool },
    SetReverbWet { value: f32 },
    SetReverbRoomSize { value: f32 },
    SetReverbPreDelayMs { value: f32 },
    SetReverbDamping { value: f32 },
    // SOFA selection
    SetSofaSelection { key: String },
    // Cache
    ValidateCache,
}

// ---------------------------------------------------------------------------
// OpenSpatial implementation
// ---------------------------------------------------------------------------

impl OpenSpatial {
    fn update_sample_rate(&mut self, sample_rate: f32) {
        if (self.sample_rate - sample_rate).abs() < f32::EPSILON {
            return;
        }
        self.remote_logger.log_step(
            "initialize.sample_rate_changed",
            format!("sample_rate={sample_rate}"),
        );
        self.sample_rate = sample_rate.max(1.0);
        self.hrtf_engine = None;
        let interval = (self.sample_rate * METER_UPDATE_SECONDS).round() as usize;
        self.meter_interval_samples = interval.max(1);
        self.meter_samples_remaining = self.meter_interval_samples;
        self.reset_meter_peaks();

        if let Ok(mut status) = self.runtime_status.lock() {
            if status.file_ready {
                status.stage = "loading".to_string();
                status.message = "HRTF file is cached, preparing renderer".to_string();
                status.ready = false;
            }
        }
        self.runtime_dirty.store(true, Ordering::Relaxed);
    }

    fn reset_meter_peaks(&mut self) {
        self.meter_input_peak_l = 0.0;
        self.meter_input_peak_r = 0.0;
        self.meter_output_peak_l = 0.0;
        self.meter_output_peak_r = 0.0;
    }

    fn publish_meter_values(&mut self) {
        self.meter_input_l.store(self.meter_input_peak_l, Ordering::Relaxed);
        self.meter_input_r.store(self.meter_input_peak_r, Ordering::Relaxed);
        self.meter_output_l.store(self.meter_output_peak_l, Ordering::Relaxed);
        self.meter_output_r.store(self.meter_output_peak_r, Ordering::Relaxed);
        self.meter_dirty.store(true, Ordering::Relaxed);
        self.reset_meter_peaks();
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
            for url in GUI_DEV_SERVER_PROBE_URLS {
                if let Ok(response) = client.get(url).call() {
                    let content_type = response.header("Content-Type").unwrap_or("");
                    if content_type.starts_with("text/") {
                        return Some(url);
                    }
                }
            }
            None
        })
        .join()
        {
            Ok(Some(url)) => url,
            _ => GUI_PUBLISHED_URL,
        }
    }

    fn maybe_queue_cache_task_from_process(&self, context: &mut impl ProcessContext<Self>) {
        if self
            .task_running
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            context.execute_background(OpenSpatialTask::EnsureHrtfCache);
        }
    }

    fn maybe_queue_cache_task_from_editor(&self, executor: &AsyncExecutor<Self>) {
        if self
            .task_running
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            self.remote_logger.log_step("editor.queue_cache_task", "queued from editor");
            executor.execute_background(OpenSpatialTask::EnsureHrtfCache);
        } else {
            self.remote_logger.log_step(
                "editor.queue_cache_task_skipped",
                "task already running when editor tried to queue cache validation",
            );
        }
    }

    fn try_load_hrtf_from_cache(&mut self) {
        if self.hrtf_engine.is_some() {
            return;
        }
        let snapshot = self
            .runtime_status
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default();
        if !snapshot.file_ready {
            return;
        }
        self.remote_logger.log_step(
            "renderer.load_cache_begin",
            format!("path={}", snapshot.hrtf_path),
        );
        if let Ok(mut status) = self.runtime_status.lock() {
            status.stage = "loading".to_string();
            status.message = "Loading cached HRTF".to_string();
            status.ready = false;
        }
        self.runtime_dirty.store(true, Ordering::Relaxed);

        match HrtfEngine::load(Path::new(&snapshot.hrtf_path), self.sample_rate) {
            Ok(engine) => {
                self.hrtf_engine = Some(engine);
                self.remote_logger.log_step(
                    "renderer.load_cache_success",
                    format!("path={} sample_rate={}", snapshot.hrtf_path, self.sample_rate),
                );
                if let Ok(mut status) = self.runtime_status.lock() {
                    status.stage = "ready".to_string();
                    status.message = "Measured HRTF ready".to_string();
                    status.progress = Some(1.0);
                    status.ready = true;
                    status.file_ready = true;
                }
            }
            Err(err) => {
                self.remote_logger.log_step(
                    "renderer.load_cache_error",
                    format!("path={} error={err}", snapshot.hrtf_path),
                );
                if let Ok(mut status) = self.runtime_status.lock() {
                    status.stage = "error".to_string();
                    status.message = err;
                    status.ready = false;
                }
            }
        }
        self.runtime_dirty.store(true, Ordering::Relaxed);
    }

    fn handle_sofa_selection_change(
        sofa_selection: &Arc<Mutex<String>>,
        new_key: String,
        runtime_status: &Arc<Mutex<RuntimeInitStatus>>,
        runtime_dirty: &Arc<AtomicBool>,
        hrtf_reload_needed: &Arc<AtomicBool>,
        task_running: &Arc<AtomicBool>,
        remote_logger: &RemoteLogger,
        executor: &AsyncExecutor<Self>,
    ) {
        remote_logger.log_step("editor.set_sofa_selection", format!("key={new_key}"));
        if let Ok(mut g) = sofa_selection.lock() {
            *g = new_key;
        }
        // Mark engine for reload
        if let Ok(mut status) = runtime_status.lock() {
            status.ready = false;
            status.file_ready = false;
            status.stage = "idle".to_string();
            status.message = "SOFA selection changed, reloading…".to_string();
        }
        hrtf_reload_needed.store(true, Ordering::Relaxed);
        runtime_dirty.store(true, Ordering::Relaxed);
        // Queue the cache / download task
        if task_running
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            executor.execute_background(OpenSpatialTask::EnsureHrtfCache);
        }
    }

    fn collect_render_params(&self) -> RenderParams {
        RenderParams {
            source_yaw_deg: self.params.source_yaw.value(),
            always_towards_head: self.params.always_towards_head.value(),
            directivity_amount: self.params.directivity.value(),
            output_gain_db: self.params.output_gain.value(),
            radial_multiply: self.params.radial_multiply.value(),
            pinna_enabled: self.params.pinna_enabled.value(),
            pinna_freq: self.params.pinna_freq.value(),
            pinna_gain_db: self.params.pinna_gain_db.value(),
            pinna_q: self.params.pinna_q.value(),
            hrtf_interpolate: self.params.hrtf_interpolate.value(),
            itd_enabled: self.params.itd_enabled.value(),
            distance_exponent: self.params.distance_exponent.value(),
            distance_min_m: self.params.distance_min_m.value(),
            directivity_floor: self.params.directivity_floor.value(),
            directivity_range: self.params.directivity_range.value(),
            directivity_exp_scale: self.params.directivity_exp_scale.value(),
            reverb_enabled: self.params.reverb_enabled.value(),
            reverb_wet: self.params.reverb_wet.value(),
            reverb_room_size: self.params.reverb_room_size.value(),
            reverb_pre_delay_ms: self.params.reverb_pre_delay_ms.value(),
            reverb_damping: self.params.reverb_damping.value(),
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin trait impl
// ---------------------------------------------------------------------------

impl Plugin for OpenSpatial {
    const NAME: &'static str = "Open Spatial";
    const VENDOR: &'static str = "WTH Plugins";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = OpenSpatialTask;

    fn task_executor(&mut self) -> TaskExecutor<Self> {
        let sofa_selection = self.params.sofa_selection.clone();
        let asset_cache_dir = self.params.asset_cache_dir.clone();
        let runtime_status = self.runtime_status.clone();
        let runtime_dirty = self.runtime_dirty.clone();
        let task_running = self.task_running.clone();
        let remote_logger = self.remote_logger.clone();

        Box::new(move |task| {
            remote_logger.log_step("task_executor.received", format!("task={task:?}"));
            match task {
                OpenSpatialTask::EnsureHrtfCache => ensure_hrtf_cache_task(
                    &sofa_selection,
                    &asset_cache_dir,
                    &runtime_status,
                    &runtime_dirty,
                    &remote_logger,
                ),
            }
            task_running.store(false, Ordering::Relaxed);
            remote_logger.log_step("task_executor.completed", format!("task={task:?}"));
        })
    }

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.remote_logger.log_step(
            "initialize.begin",
            format!("sample_rate={}", buffer_config.sample_rate),
        );
        self.update_sample_rate(buffer_config.sample_rate);
        self.remote_logger.log_step("initialize.end", "plugin initialize completed");
        true
    }

    fn reset(&mut self) {
        self.reset_meter_peaks();
        self.meter_samples_remaining = self.meter_interval_samples;
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Drop and reload engine if SOFA selection changed from the editor.
        if self.hrtf_reload_needed.swap(false, Ordering::Relaxed) {
            self.hrtf_engine = None;
            self.remote_logger.log_step("process.sofa_reload", "engine dropped due to SOFA change");
        }

        self.try_load_hrtf_from_cache();

        let mut samples_remaining = self.meter_samples_remaining;
        let runtime_snapshot = self
            .runtime_status
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default();

        if !runtime_snapshot.file_ready && self.hrtf_engine.is_none() {
            self.maybe_queue_cache_task_from_process(context);
        }

        let sample_count = buffer.samples();
        let channels = buffer.as_slice();

        // Mix stereo input to mono, track input meters.
        let mut mono_input = vec![0.0; sample_count];
        for i in 0..sample_count {
            let left = channels.get(0).and_then(|c| c.get(i)).copied().unwrap_or(0.0);
            let right = channels.get(1).and_then(|c| c.get(i)).copied().unwrap_or(left);
            mono_input[i] = 0.5 * (left + right);
            self.meter_input_peak_l = self.meter_input_peak_l.max(left.abs());
            self.meter_input_peak_r = self.meter_input_peak_r.max(right.abs());
        }

        let distance_min = self.params.distance_min_m.value().max(0.01);
        let pose = SpatialPose::from_params(
            self.params.azimuth.value(),
            self.params.elevation.value(),
            self.params.distance.value().max(distance_min),
        );

        let rp = self.collect_render_params();
        if let Some(engine) = self.hrtf_engine.as_mut() {
            let mut output_left = vec![0.0; sample_count];
            let mut output_right = vec![0.0; sample_count];

            if let Err(err) =
                engine.render_block(&mono_input, pose, &rp, &mut output_left, &mut output_right)
            {
                if let Ok(mut status) = self.runtime_status.lock() {
                    status.stage = "error".to_string();
                    status.message = err;
                    status.ready = false;
                }
                self.runtime_dirty.store(true, Ordering::Relaxed);
                self.hrtf_engine = None;
            }

            for i in 0..sample_count {
                if let Some(l) = channels.get_mut(0).and_then(|c| c.get_mut(i)) {
                    *l = output_left[i];
                }
                if let Some(r) = channels.get_mut(1).and_then(|c| c.get_mut(i)) {
                    *r = output_right[i];
                }
                self.meter_output_peak_l = self.meter_output_peak_l.max(output_left[i].abs());
                self.meter_output_peak_r = self.meter_output_peak_r.max(output_right[i].abs());
                samples_remaining = samples_remaining.saturating_sub(1);
                if samples_remaining == 0 {
                    self.publish_meter_values();
                    samples_remaining = self.meter_interval_samples;
                }
            }
        } else {
            for i in 0..sample_count {
                if let Some(l) = channels.get_mut(0).and_then(|c| c.get_mut(i)) {
                    *l = 0.0;
                }
                if let Some(r) = channels.get_mut(1).and_then(|c| c.get_mut(i)) {
                    *r = 0.0;
                }
                samples_remaining = samples_remaining.saturating_sub(1);
                if samples_remaining == 0 {
                    self.publish_meter_values();
                    samples_remaining = self.meter_interval_samples;
                }
            }
        }

        self.meter_samples_remaining = samples_remaining;
        ProcessStatus::Normal
    }

    fn editor(&mut self, async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        self.remote_logger.log_step("editor.begin", "building webview editor");

        // Drop engine if SOFA changed before editor opened.
        if self.hrtf_reload_needed.swap(false, Ordering::Relaxed) {
            self.hrtf_engine = None;
        }

        self.try_load_hrtf_from_cache();

        let params = self.params.clone();
        let params_dirty = self.params.params_dirty.clone();
        let meter_input_l = self.meter_input_l.clone();
        let meter_input_r = self.meter_input_r.clone();
        let meter_output_l = self.meter_output_l.clone();
        let meter_output_r = self.meter_output_r.clone();
        let meter_dirty = self.meter_dirty.clone();
        let runtime_status = self.runtime_status.clone();
        let runtime_dirty = self.runtime_dirty.clone();
        let task_running = self.task_running.clone();
        let remote_logger = self.remote_logger.clone();
        let hrtf_reload_needed = self.hrtf_reload_needed.clone();

        if !runtime_status.lock().map(|s| s.file_ready).unwrap_or(false)
            && !task_running.load(Ordering::Relaxed)
        {
            self.remote_logger.log_step(
                "editor.cache_status_missing",
                "cache not ready when editor opened; queueing background task",
            );
            self.maybe_queue_cache_task_from_editor(&async_executor);
        }

        let source = HTMLSource::URL(Self::resolve_gui_url());
        self.remote_logger.log_step("editor.webview_source", "resolved GUI source URL");

        let editor = WebViewEditor::new(source, (GUI_WIDTH, GUI_HEIGHT))
            .with_developer_mode(true)
            .with_event_loop(move |ctx, setter, _window| {
                while let Ok(value) = ctx.next_event() {
                    remote_logger.log_step("editor.event_raw", value.to_string());
                    if let Ok(action) = serde_json::from_value::<Action>(value) {
                        match action {
                            Action::Init => {
                                remote_logger.log_step("editor.action_init", "");
                                send_state_message(ctx, &params, &runtime_status);
                            }

                            // --- Spatial ---
                            Action::SetAzimuth { value } => set_float_parameter(&setter, &params.azimuth, value),
                            Action::SetElevation { value } => set_float_parameter(&setter, &params.elevation, value),
                            Action::SetDistance { value } => set_float_parameter(&setter, &params.distance, value),
                            Action::SetSourceYaw { value } => set_float_parameter(&setter, &params.source_yaw, value),
                            Action::SetAlwaysTowardsHead { value } => set_bool_parameter(&setter, &params.always_towards_head, value),
                            Action::SetDirectivity { value } => set_float_parameter(&setter, &params.directivity, value),
                            Action::SetOutputGain { value } => set_float_parameter(&setter, &params.output_gain, value),
                            Action::SetRadialMultiply { value } => set_float_parameter(&setter, &params.radial_multiply, value),

                            // --- Pinna ---
                            Action::SetPinnaEnabled { value } => set_bool_parameter(&setter, &params.pinna_enabled, value),
                            Action::SetPinnaFreq { value } => set_float_parameter(&setter, &params.pinna_freq, value),
                            Action::SetPinnaGainDb { value } => set_float_parameter(&setter, &params.pinna_gain_db, value),
                            Action::SetPinnaQ { value } => set_float_parameter(&setter, &params.pinna_q, value),

                            // --- HRTF engine ---
                            Action::SetHrtfInterpolate { value } => set_bool_parameter(&setter, &params.hrtf_interpolate, value),
                            Action::SetItdEnabled { value } => set_bool_parameter(&setter, &params.itd_enabled, value),

                            // --- Distance ---
                            Action::SetDistanceExponent { value } => set_float_parameter(&setter, &params.distance_exponent, value),
                            Action::SetDistanceMinM { value } => set_float_parameter(&setter, &params.distance_min_m, value),

                            // --- Directivity ---
                            Action::SetDirectivityFloor { value } => set_float_parameter(&setter, &params.directivity_floor, value),
                            Action::SetDirectivityRange { value } => set_float_parameter(&setter, &params.directivity_range, value),
                            Action::SetDirectivityExpScale { value } => set_float_parameter(&setter, &params.directivity_exp_scale, value),

                            // --- Reverb ---
                            Action::SetReverbEnabled { value } => set_bool_parameter(&setter, &params.reverb_enabled, value),
                            Action::SetReverbWet { value } => set_float_parameter(&setter, &params.reverb_wet, value),
                            Action::SetReverbRoomSize { value } => set_float_parameter(&setter, &params.reverb_room_size, value),
                            Action::SetReverbPreDelayMs { value } => set_float_parameter(&setter, &params.reverb_pre_delay_ms, value),
                            Action::SetReverbDamping { value } => set_float_parameter(&setter, &params.reverb_damping, value),

                            // --- SOFA selection ---
                            Action::SetSofaSelection { key } => {
                                OpenSpatial::handle_sofa_selection_change(
                                    &params.sofa_selection,
                                    key,
                                    &runtime_status,
                                    &runtime_dirty,
                                    &hrtf_reload_needed,
                                    &task_running,
                                    &remote_logger,
                                    &async_executor,
                                );
                                params.params_dirty.store(true, Ordering::Relaxed);
                            }

                            // --- Cache ---
                            Action::ValidateCache => {
                                remote_logger.log_step("editor.action_validate_cache", "");
                                if task_running
                                    .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
                                    .is_ok()
                                {
                                    async_executor.execute_background(OpenSpatialTask::EnsureHrtfCache);
                                }
                            }
                        }
                    } else {
                        remote_logger.log_step("editor.event_parse_failed", "");
                    }
                }

                if params_dirty.swap(false, Ordering::Relaxed) {
                    send_state_message(ctx, &params, &runtime_status);
                }

                if runtime_dirty.swap(false, Ordering::Relaxed) {
                    send_state_message(ctx, &params, &runtime_status);
                }

                if meter_dirty.swap(false, Ordering::Relaxed) {
                    ctx.send_json(json!({
                        "type": "Meter",
                        "input": {
                            "l": meter_input_l.load(Ordering::Relaxed),
                            "r": meter_input_r.load(Ordering::Relaxed),
                        },
                        "output": {
                            "l": meter_output_l.load(Ordering::Relaxed),
                            "r": meter_output_r.load(Ordering::Relaxed),
                        },
                    }));
                }
            });

        Some(Box::new(editor))
    }
}

// ---------------------------------------------------------------------------
// VST3 / CLAP exports
// ---------------------------------------------------------------------------

impl Vst3Plugin for OpenSpatial {
    const VST3_CLASS_ID: [u8; 16] = *b"WTH_OpenSpatial_";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Tools];
}

nih_export_vst3!(OpenSpatial);

impl ClapPlugin for OpenSpatial {
    const CLAP_ID: &'static str = "wthplugins.open_spatial";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Runtime-downloaded SOFA HRTF spatializer with source directivity and room reverb");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] =
        &[ClapFeature::AudioEffect, ClapFeature::Stereo, ClapFeature::Utility];
}

nih_export_clap!(OpenSpatial);

// ---------------------------------------------------------------------------
// Background task: ensure HRTF cache (download if needed)
// ---------------------------------------------------------------------------

fn ensure_hrtf_cache_task(
    sofa_selection: &Arc<Mutex<String>>,
    asset_cache_dir: &Arc<Mutex<String>>,
    runtime_status: &Arc<Mutex<RuntimeInitStatus>>,
    runtime_dirty: &Arc<AtomicBool>,
    remote_logger: &RemoteLogger,
) {
    // Determine which SOFA to use based on current selection.
    let key = sofa_selection
        .lock()
        .map(|g| g.clone())
        .unwrap_or_else(|_| DEFAULT_SOFA_KEY.to_string());
    let sofa_def = sofa_definition_for_key(&key);
    let hrtf_url = sofa_def.url;
    let hrtf_filename = sofa_def.filename;

    let cache_root = asset_cache_dir
        .lock()
        .map(|guard| {
            if guard.trim().is_empty() {
                default_cache_root()
            } else {
                PathBuf::from(guard.clone())
            }
        })
        .unwrap_or_else(|_| default_cache_root());

    if let Ok(mut guard) = asset_cache_dir.lock() {
        *guard = cache_root.to_string_lossy().to_string();
    }

    let hrtf_path = cache_root.join(hrtf_filename);
    let manifest_path = cache_root.join(ASSET_MANIFEST_FILE);

    remote_logger.log_step(
        "cache_task.begin",
        format!(
            "sofa_key={} cache_root={} hrtf_path={} manifest_path={}",
            key,
            cache_root.display(),
            hrtf_path.display(),
            manifest_path.display()
        ),
    );
    set_runtime_status(
        runtime_status,
        runtime_dirty,
        remote_logger,
        RuntimeInitStatus {
            stage: "validating".to_string(),
            message: "Validating runtime HRTF cache".to_string(),
            progress: None,
            downloaded_bytes: None,
            total_bytes: None,
            cache_path: cache_root.to_string_lossy().to_string(),
            hrtf_path: hrtf_path.to_string_lossy().to_string(),
            hrtf_url: hrtf_url.to_string(),
            ready: false,
            file_ready: false,
        },
    );

    let task_result = (|| -> Result<RuntimeInitStatus, String> {
        fs::create_dir_all(&cache_root)
            .map_err(|err| format!("Failed to create cache directory: {err}"))?;

        if hrtf_path.is_file() && manifest_path.is_file() {
            let manifest = read_manifest(&manifest_path)?;
            if manifest.renderer_id == ANALYTIC_RENDERER_ID
                && manifest.hrtf_url == hrtf_url
                && manifest.hrtf_filename == hrtf_filename
            {
                let file_hash = sha256_of_file(&hrtf_path)?;
                let file_bytes = fs::metadata(&hrtf_path)
                    .map_err(|err| format!("Failed to stat cached HRTF: {err}"))?
                    .len();
                if file_hash == manifest.sha256_hex && file_bytes == manifest.bytes {
                    remote_logger.log_step(
                        "cache_task.cache_hit",
                        format!("bytes={file_bytes} sha256={file_hash}"),
                    );
                    return Ok(RuntimeInitStatus {
                        stage: "cached".to_string(),
                        message: "Cached HRTF is valid, waiting for renderer load".to_string(),
                        progress: Some(1.0),
                        downloaded_bytes: Some(file_bytes),
                        total_bytes: Some(file_bytes),
                        cache_path: cache_root.to_string_lossy().to_string(),
                        hrtf_path: hrtf_path.to_string_lossy().to_string(),
                        hrtf_url: hrtf_url.to_string(),
                        ready: false,
                        file_ready: true,
                    });
                }
            }
        }

        let temp_path = cache_root.join(format!("{hrtf_filename}.download"));
        remote_logger.log_step("cache_task.download_begin", format!("url={hrtf_url}"));
        let response = ureq::get(hrtf_url)
            .call()
            .map_err(|err| format!("Failed to download HRTF: {err}"))?;
        let total_bytes = response
            .header("Content-Length")
            .and_then(|v| v.parse::<u64>().ok());
        let mut reader = response.into_reader();
        let mut file = File::create(&temp_path)
            .map_err(|err| format!("Failed to create temp file: {err}"))?;
        let mut hasher = Sha256::new();
        let mut downloaded = 0_u64;
        let mut buffer = vec![0_u8; DOWNLOAD_BUFFER_BYTES];

        loop {
            let bytes_read = reader
                .read(&mut buffer)
                .map_err(|err| format!("Failed while reading download stream: {err}"))?;
            if bytes_read == 0 {
                break;
            }
            file.write_all(&buffer[..bytes_read])
                .map_err(|err| format!("Failed while writing cached HRTF: {err}"))?;
            hasher.update(&buffer[..bytes_read]);
            downloaded += bytes_read as u64;

            set_runtime_status(
                runtime_status,
                runtime_dirty,
                remote_logger,
                RuntimeInitStatus {
                    stage: "downloading".to_string(),
                    message: "Downloading HRTF asset".to_string(),
                    progress: total_bytes.map(|t| downloaded as f32 / t as f32),
                    downloaded_bytes: Some(downloaded),
                    total_bytes,
                    cache_path: cache_root.to_string_lossy().to_string(),
                    hrtf_path: hrtf_path.to_string_lossy().to_string(),
                    hrtf_url: hrtf_url.to_string(),
                    ready: false,
                    file_ready: false,
                },
            );
        }

        file.flush()
            .map_err(|err| format!("Failed to flush cached HRTF file: {err}"))?;
        fs::rename(&temp_path, &hrtf_path)
            .map_err(|err| format!("Failed to move cached HRTF into place: {err}"))?;

        let sha256_hex = hex_encode(&hasher.finalize());
        remote_logger.log_step(
            "cache_task.download_complete",
            format!("bytes={downloaded} sha256={sha256_hex}"),
        );
        let manifest = AssetManifest {
            schema_version: 1,
            renderer_id: ANALYTIC_RENDERER_ID.to_string(),
            hrtf_url: hrtf_url.to_string(),
            hrtf_filename: hrtf_filename.to_string(),
            sha256_hex,
            bytes: downloaded,
        };
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|err| format!("Failed to serialize asset manifest: {err}"))?;
        fs::write(&manifest_path, manifest_bytes)
            .map_err(|err| format!("Failed to write asset manifest: {err}"))?;

        Ok(RuntimeInitStatus {
            stage: "cached".to_string(),
            message: "HRTF downloaded and cached, waiting for renderer load".to_string(),
            progress: Some(1.0),
            downloaded_bytes: Some(downloaded),
            total_bytes: Some(downloaded),
            cache_path: cache_root.to_string_lossy().to_string(),
            hrtf_path: hrtf_path.to_string_lossy().to_string(),
            hrtf_url: hrtf_url.to_string(),
            ready: false,
            file_ready: true,
        })
    })();

    match task_result {
        Ok(status) => {
            remote_logger.log_step(
                "cache_task.success",
                format!("stage={} file_ready={}", status.stage, status.file_ready),
            );
            set_runtime_status(runtime_status, runtime_dirty, remote_logger, status);
        }
        Err(err) => {
            remote_logger.log_step("cache_task.error", err.clone());
            set_runtime_status(
                runtime_status,
                runtime_dirty,
                remote_logger,
                RuntimeInitStatus {
                    stage: "error".to_string(),
                    message: err,
                    progress: None,
                    downloaded_bytes: None,
                    total_bytes: None,
                    cache_path: cache_root.to_string_lossy().to_string(),
                    hrtf_path: hrtf_path.to_string_lossy().to_string(),
                    hrtf_url: hrtf_url.to_string(),
                    ready: false,
                    file_ready: false,
                },
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

fn set_runtime_status(
    runtime_status: &Arc<Mutex<RuntimeInitStatus>>,
    runtime_dirty: &Arc<AtomicBool>,
    remote_logger: &RemoteLogger,
    status: RuntimeInitStatus,
) {
    remote_logger.log_step(
        "runtime_status.set",
        format!(
            "stage={} ready={} file_ready={} progress={:?}",
            status.stage, status.ready, status.file_ready, status.progress
        ),
    );
    if let Ok(mut guard) = runtime_status.lock() {
        *guard = status;
    }
    runtime_dirty.store(true, Ordering::Relaxed);
}

fn read_manifest(path: &Path) -> Result<AssetManifest, String> {
    let bytes = fs::read(path).map_err(|err| format!("Failed to read asset manifest: {err}"))?;
    serde_json::from_slice(&bytes)
        .map_err(|err| format!("Failed to parse asset manifest: {err}"))
}

fn sha256_of_file(path: &Path) -> Result<String, String> {
    let mut file =
        File::open(path).map_err(|err| format!("Failed to open cached HRTF: {err}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; DOWNLOAD_BUFFER_BYTES];
    loop {
        let bytes_read = file
            .read(&mut buffer)
            .map_err(|err| format!("Failed to read cached HRTF: {err}"))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    Ok(hex_encode(&hasher.finalize()))
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn default_cache_root() -> PathBuf {
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        return PathBuf::from(local_app_data).join(CACHE_NAMESPACE);
    }
    if let Ok(xdg_cache_home) = std::env::var("XDG_CACHE_HOME") {
        return PathBuf::from(xdg_cache_home).join(CACHE_NAMESPACE);
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".cache").join(CACHE_NAMESPACE);
    }
    std::env::temp_dir().join("open_spatial_cache")
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

fn set_float_parameter(setter: &ParamSetter<'_>, param: &FloatParam, value: f32) {
    setter.begin_set_parameter(param);
    setter.set_parameter(param, value);
    setter.end_set_parameter(param);
}

fn set_bool_parameter(setter: &ParamSetter<'_>, param: &BoolParam, value: bool) {
    setter.begin_set_parameter(param);
    setter.set_parameter(param, value);
    setter.end_set_parameter(param);
}

fn send_state_message(
    ctx: &WindowHandler,
    params: &Arc<OpenSpatialParams>,
    runtime_status: &Arc<Mutex<RuntimeInitStatus>>,
) {
    let runtime = runtime_status
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default();

    let sofa_key = params
        .sofa_selection
        .lock()
        .map(|g| g.clone())
        .unwrap_or_else(|_| DEFAULT_SOFA_KEY.to_string());

    let sofa_options: Vec<serde_json::Value> = SOFA_DEFINITIONS
        .iter()
        .map(|d| json!({ "key": d.key, "name": d.name }))
        .collect();

    ctx.send_json(json!({
        "type": "State",
        // Spatial
        "azimuth": params.azimuth.value(),
        "elevation": params.elevation.value(),
        "distance": params.distance.value(),
        "sourceYaw": params.source_yaw.value(),
        "alwaysTowardsHead": params.always_towards_head.value(),
        "directivity": params.directivity.value(),
        "outputGain": params.output_gain.value(),
        "radialMultiply": params.radial_multiply.value(),
        // Pinna
        "pinnaEnabled": params.pinna_enabled.value(),
        "pinnaFreq": params.pinna_freq.value(),
        "pinnaGainDb": params.pinna_gain_db.value(),
        "pinnaQ": params.pinna_q.value(),
        // HRTF engine
        "hrtfInterpolate": params.hrtf_interpolate.value(),
        "itdEnabled": params.itd_enabled.value(),
        // Distance model
        "distanceExponent": params.distance_exponent.value(),
        "distanceMinM": params.distance_min_m.value(),
        // Directivity model
        "directivityFloor": params.directivity_floor.value(),
        "directivityRange": params.directivity_range.value(),
        "directivityExpScale": params.directivity_exp_scale.value(),
        // Reverb
        "reverbEnabled": params.reverb_enabled.value(),
        "reverbWet": params.reverb_wet.value(),
        "reverbRoomSize": params.reverb_room_size.value(),
        "reverbPreDelayMs": params.reverb_pre_delay_ms.value(),
        "reverbDamping": params.reverb_damping.value(),
        // SOFA selection
        "sofaKey": sofa_key,
        "sofaOptions": sofa_options,
        // Plugin info
        "pluginVersion": env!("CARGO_PKG_VERSION"),
        "rendererId": ANALYTIC_RENDERER_ID,
        // Runtime init
        "cachePath": runtime.cache_path,
        "hrtfPath": runtime.hrtf_path,
        "hrtfUrl": runtime.hrtf_url,
        "initStage": runtime.stage,
        "initMessage": runtime.message,
        "initProgress": runtime.progress,
        "downloadedBytes": runtime.downloaded_bytes,
        "totalBytes": runtime.total_bytes,
        "hrtfReady": runtime.ready,
    }));
}
