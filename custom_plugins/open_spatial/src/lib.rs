use nih_plug::params::enums::Enum;
use nih_plug::prelude::*;
use nih_plug_webview::*;
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

const GUI_WIDTH: u32 = 1080;
const GUI_HEIGHT: u32 = 760;
const GUI_DEV_SERVER_URL: &str = "http://localhost:5173";
const GUI_DEV_SERVER_ROUTE: &str = "/open-spatial";
const GUI_DEV_SERVER_PROBE_URL: &str = "http://localhost:5173/open-spatial";
const GUI_PUBLISHED_URL: &str = "https://open-spatial-web-gui.vercel.app";
const METER_UPDATE_SECONDS: f32 = 0.1;
const ANALYTIC_RENDERER_ID: &str = "sofa-runtime-fetch-v1";
const ASSET_MANIFEST_FILE: &str = "asset_manifest.json";
const CACHE_NAMESPACE: &str = "wth_plugins/open_spatial";
const HRTF_URL: &str = "https://zenodo.org/records/3928297/files/HRIR_L2354.sofa?download=1";
const HRTF_FILENAME: &str = "HRIR_L2354.sofa";
const DOWNLOAD_BUFFER_BYTES: usize = 64 * 1024;
const HRTF_PARTITION_LEN: usize = 64;

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
enum CoordinateMode {
    #[id = "spherical"]
    #[name = "Spherical"]
    Spherical,
    #[id = "cylindrical"]
    #[name = "Cylindrical"]
    Cylindrical,
}

impl CoordinateMode {
    fn from_id(value: &str) -> Self {
        match value {
            "cylindrical" => Self::Cylindrical,
            _ => Self::Spherical,
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::Spherical => "spherical",
            Self::Cylindrical => "cylindrical",
        }
    }
}

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
        Self {
            stage: "idle".to_string(),
            message: "Waiting for initialization".to_string(),
            progress: None,
            downloaded_bytes: None,
            total_bytes: None,
            cache_path: String::new(),
            hrtf_path: String::new(),
            hrtf_url: HRTF_URL.to_string(),
            ready: false,
            file_ready: false,
        }
    }
}

#[derive(Params)]
struct OpenSpatialParams {
    #[id = "coord"]
    coordinate_mode: EnumParam<CoordinateMode>,
    #[id = "azimth"]
    azimuth: FloatParam,
    #[id = "elevtn"]
    elevation: FloatParam,
    #[id = "distnc"]
    distance: FloatParam,
    #[id = "radius"]
    radius: FloatParam,
    #[id = "height"]
    height: FloatParam,
    #[id = "srcyaw"]
    source_yaw: FloatParam,
    #[id = "direct"]
    directivity: FloatParam,
    #[id = "outgn"]
    output_gain: FloatParam,

    #[persist = "asset_cache_dir"]
    asset_cache_dir: Arc<Mutex<String>>,

    params_dirty: Arc<AtomicBool>,
}

impl Default for OpenSpatialParams {
    fn default() -> Self {
        let params_dirty = Arc::new(AtomicBool::new(false));

        let coordinate_dirty = params_dirty.clone();
        let azimuth_dirty = params_dirty.clone();
        let elevation_dirty = params_dirty.clone();
        let distance_dirty = params_dirty.clone();
        let radius_dirty = params_dirty.clone();
        let height_dirty = params_dirty.clone();
        let source_yaw_dirty = params_dirty.clone();
        let directivity_dirty = params_dirty.clone();
        let output_gain_dirty = params_dirty.clone();

        Self {
            coordinate_mode: EnumParam::new("Coordinates", CoordinateMode::Spherical)
                .with_callback(Arc::new(move |_| {
                    coordinate_dirty.store(true, Ordering::Relaxed);
                })),
            azimuth: FloatParam::new(
                "Azimuth",
                30.0,
                FloatRange::Linear {
                    min: -180.0,
                    max: 180.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_step_size(0.1)
            .with_unit(" deg")
            .with_callback(Arc::new(move |_| {
                azimuth_dirty.store(true, Ordering::Relaxed);
            })),
            elevation: FloatParam::new(
                "Elevation",
                0.0,
                FloatRange::Linear {
                    min: -90.0,
                    max: 90.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_step_size(0.1)
            .with_unit(" deg")
            .with_callback(Arc::new(move |_| {
                elevation_dirty.store(true, Ordering::Relaxed);
            })),
            distance: FloatParam::new(
                "Distance",
                1.5,
                FloatRange::Skewed {
                    min: 1.0,
                    max: 30.0,
                    factor: FloatRange::skew_factor(3.0),
                },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_step_size(0.01)
            .with_unit(" m")
            .with_callback(Arc::new(move |_| {
                distance_dirty.store(true, Ordering::Relaxed);
            })),
            radius: FloatParam::new(
                "Radius",
                1.5,
                FloatRange::Skewed {
                    min: 1.0,
                    max: 30.0,
                    factor: FloatRange::skew_factor(3.0),
                },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_step_size(0.01)
            .with_unit(" m")
            .with_callback(Arc::new(move |_| {
                radius_dirty.store(true, Ordering::Relaxed);
            })),
            height: FloatParam::new(
                "Height",
                0.0,
                FloatRange::Linear {
                    min: -10.0,
                    max: 10.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_step_size(0.01)
            .with_unit(" m")
            .with_callback(Arc::new(move |_| {
                height_dirty.store(true, Ordering::Relaxed);
            })),
            source_yaw: FloatParam::new(
                "Source Yaw",
                180.0,
                FloatRange::Linear {
                    min: -180.0,
                    max: 180.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_step_size(0.1)
            .with_unit(" deg")
            .with_callback(Arc::new(move |_| {
                source_yaw_dirty.store(true, Ordering::Relaxed);
            })),
            directivity: FloatParam::new(
                "Directivity",
                0.65,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_step_size(0.01)
            .with_callback(Arc::new(move |_| {
                directivity_dirty.store(true, Ordering::Relaxed);
            })),
            output_gain: FloatParam::new(
                "Output Gain",
                -3.0,
                FloatRange::Linear {
                    min: -24.0,
                    max: 12.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_step_size(0.1)
            .with_unit(" dB")
            .with_callback(Arc::new(move |_| {
                output_gain_dirty.store(true, Ordering::Relaxed);
            })),
            asset_cache_dir: Arc::new(Mutex::new(String::new())),
            params_dirty,
        }
    }
}

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
    distance_m: f32,
}

impl SpatialPose {
    fn from_params(
        coordinate_mode: CoordinateMode,
        azimuth_deg: f32,
        elevation_deg: f32,
        distance_m: f32,
        radius_m: f32,
        height_m: f32,
    ) -> Self {
        let azimuth_rad = azimuth_deg.to_radians();

        match coordinate_mode {
            CoordinateMode::Spherical => {
                let elevation_rad = elevation_deg.to_radians();
                let distance = distance_m.max(1.0);
                let horizontal = distance * elevation_rad.cos();
                Self {
                    position: Vec3 {
                        x: horizontal * azimuth_rad.cos(),
                        y: horizontal * azimuth_rad.sin(),
                        z: distance * elevation_rad.sin(),
                    },
                    distance_m: distance,
                }
            }
            CoordinateMode::Cylindrical => {
                let radius = radius_m.max(1.0);
                let position = Vec3 {
                    x: radius * azimuth_rad.cos(),
                    y: radius * azimuth_rad.sin(),
                    z: height_m,
                };
                Self {
                    position,
                    distance_m: position.norm().max(1.0),
                }
            }
        }
    }
}

struct HrtfEngine {
    sofa: Sofar,
    filter: Filter,
    renderer: Renderer,
    mono_input: Vec<f32>,
    output_left: Vec<f32>,
    output_right: Vec<f32>,
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

        Ok(Self {
            sofa,
            filter,
            renderer,
            mono_input: Vec::new(),
            output_left: Vec::new(),
            output_right: Vec::new(),
        })
    }

    fn ensure_buffer_len(&mut self, len: usize) {
        if self.mono_input.len() != len {
            self.mono_input.resize(len, 0.0);
            self.output_left.resize(len, 0.0);
            self.output_right.resize(len, 0.0);
        }
    }

    fn render_block(
        &mut self,
        mono_input: &[f32],
        pose: SpatialPose,
        source_yaw_deg: f32,
        directivity_amount: f32,
        output_gain_db: f32,
        out_left: &mut [f32],
        out_right: &mut [f32],
    ) -> Result<(), String> {
        self.ensure_buffer_len(mono_input.len());

        let direction = pose.position.normalized();
        let to_listener = Vec3 {
            x: -direction.x,
            y: -direction.y,
            z: -direction.z,
        };
        let source_yaw_rad = source_yaw_deg.to_radians();
        let source_forward = Vec3 {
            x: source_yaw_rad.cos(),
            y: source_yaw_rad.sin(),
            z: 0.0,
        };
        let alignment = source_forward.dot(to_listener).clamp(-1.0, 1.0);
        let cardioid = 0.5 * (1.0 + alignment);
        let directivity_gain = lerp(
            1.0,
            (0.15 + 0.85 * cardioid).powf(1.0 + directivity_amount * 3.0),
            directivity_amount.clamp(0.0, 1.0),
        );
        let distance_gain = pose.distance_m.max(1.0).sqrt().recip();
        let input_gain = util::db_to_gain_fast(output_gain_db) * directivity_gain * distance_gain;

        for (dst, src) in self.mono_input.iter_mut().zip(mono_input.iter()) {
            *dst = *src * input_gain;
        }

        // libmysofa uses x = right, y = front, z = up. Our internal pose uses x = front, y = right.
        self.sofa
            .filter(
                pose.position.y,
                pose.position.x,
                pose.position.z,
                &mut self.filter,
            );
        self.renderer.set_filter(&self.filter);
        self.renderer
            .process_block(
                &self.mono_input,
                &mut self.output_left,
                &mut self.output_right,
            )
            .map_err(|err| format!("Failed to process HRTF block: {err}"))?;

        out_left.copy_from_slice(&self.output_left);
        out_right.copy_from_slice(&self.output_right);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
enum OpenSpatialTask {
    EnsureHrtfCache,
}

pub struct OpenSpatial {
    params: Arc<OpenSpatialParams>,
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
}

impl Default for OpenSpatial {
    fn default() -> Self {
        Self {
            params: Arc::new(OpenSpatialParams::default()),
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
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Action {
    Init,
    SetCoordinateMode { value: String },
    SetAzimuth { value: f32 },
    SetElevation { value: f32 },
    SetDistance { value: f32 },
    SetRadius { value: f32 },
    SetHeight { value: f32 },
    SetSourceYaw { value: f32 },
    SetDirectivity { value: f32 },
    SetOutputGain { value: f32 },
    ValidateCache,
}

impl OpenSpatial {
    fn update_sample_rate(&mut self, sample_rate: f32) {
        if (self.sample_rate - sample_rate).abs() < f32::EPSILON {
            return;
        }

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
        self.meter_input_l
            .store(self.meter_input_peak_l, Ordering::Relaxed);
        self.meter_input_r
            .store(self.meter_input_peak_r, Ordering::Relaxed);
        self.meter_output_l
            .store(self.meter_output_peak_l, Ordering::Relaxed);
        self.meter_output_r
            .store(self.meter_output_peak_r, Ordering::Relaxed);
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

            client.get(GUI_DEV_SERVER_PROBE_URL).call()
        })
        .join()
        {
            Ok(Ok(response)) => {
                let content_type = response.header("Content-Type").unwrap_or("");
                if content_type.starts_with("text/") {
                    GUI_DEV_SERVER_PROBE_URL
                } else {
                    GUI_PUBLISHED_URL
                }
            }
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
            executor.execute_background(OpenSpatialTask::EnsureHrtfCache);
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

        if let Ok(mut status) = self.runtime_status.lock() {
            status.stage = "loading".to_string();
            status.message = "Loading cached HRTF".to_string();
            status.ready = false;
        }
        self.runtime_dirty.store(true, Ordering::Relaxed);

        match HrtfEngine::load(Path::new(&snapshot.hrtf_path), self.sample_rate) {
            Ok(engine) => {
                self.hrtf_engine = Some(engine);
                if let Ok(mut status) = self.runtime_status.lock() {
                    status.stage = "ready".to_string();
                    status.message = "Measured HRTF ready".to_string();
                    status.progress = Some(1.0);
                    status.ready = true;
                    status.file_ready = true;
                }
            }
            Err(err) => {
                if let Ok(mut status) = self.runtime_status.lock() {
                    status.stage = "error".to_string();
                    status.message = err;
                    status.ready = false;
                }
            }
        }
        self.runtime_dirty.store(true, Ordering::Relaxed);
    }
}

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
        let asset_cache_dir = self.params.asset_cache_dir.clone();
        let runtime_status = self.runtime_status.clone();
        let runtime_dirty = self.runtime_dirty.clone();
        let task_running = self.task_running.clone();

        Box::new(move |task| {
            match task {
                OpenSpatialTask::EnsureHrtfCache => {
                    ensure_hrtf_cache_task(&asset_cache_dir, &runtime_status, &runtime_dirty)
                }
            }
            task_running.store(false, Ordering::Relaxed);
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
        self.update_sample_rate(buffer_config.sample_rate);
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
        self.update_sample_rate(context.transport().sample_rate);
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
        let mut mono_input = vec![0.0; sample_count];
        for sample_idx in 0..sample_count {
            let left = channels
                .get(0)
                .and_then(|channel| channel.get(sample_idx))
                .copied()
                .unwrap_or(0.0);
            let right = channels
                .get(1)
                .and_then(|channel| channel.get(sample_idx))
                .copied()
                .unwrap_or(left);
            mono_input[sample_idx] = 0.5 * (left + right);
            self.meter_input_peak_l = self.meter_input_peak_l.max(left.abs());
            self.meter_input_peak_r = self.meter_input_peak_r.max(right.abs());
        }

        let coordinate_mode = self.params.coordinate_mode.value();
        let pose = SpatialPose::from_params(
            coordinate_mode,
            self.params.azimuth.value(),
            self.params.elevation.value(),
            self.params.distance.value(),
            self.params.radius.value(),
            self.params.height.value(),
        );

        if let Some(engine) = self.hrtf_engine.as_mut() {
            let mut output_left = vec![0.0; sample_count];
            let mut output_right = vec![0.0; sample_count];
            let source_yaw = self.params.source_yaw.value();
            let directivity = self.params.directivity.value();
            let output_gain = self.params.output_gain.value();

            if let Err(err) = engine.render_block(
                &mono_input,
                pose,
                source_yaw,
                directivity,
                output_gain,
                &mut output_left,
                &mut output_right,
            ) {
                if let Ok(mut status) = self.runtime_status.lock() {
                    status.stage = "error".to_string();
                    status.message = err;
                    status.ready = false;
                }
                self.runtime_dirty.store(true, Ordering::Relaxed);
                self.hrtf_engine = None;
            }

            for sample_idx in 0..sample_count {
                if let Some(left) = channels
                    .get_mut(0)
                    .and_then(|channel| channel.get_mut(sample_idx))
                {
                    *left = output_left[sample_idx];
                }
                if let Some(right) = channels
                    .get_mut(1)
                    .and_then(|channel| channel.get_mut(sample_idx))
                {
                    *right = output_right[sample_idx];
                }

                self.meter_output_peak_l =
                    self.meter_output_peak_l.max(output_left[sample_idx].abs());
                self.meter_output_peak_r =
                    self.meter_output_peak_r.max(output_right[sample_idx].abs());

                samples_remaining = samples_remaining.saturating_sub(1);
                if samples_remaining == 0 {
                    self.publish_meter_values();
                    samples_remaining = self.meter_interval_samples;
                }
            }
        } else {
            for sample_idx in 0..sample_count {
                if let Some(left) = channels
                    .get_mut(0)
                    .and_then(|channel| channel.get_mut(sample_idx))
                {
                    *left = 0.0;
                }
                if let Some(right) = channels
                    .get_mut(1)
                    .and_then(|channel| channel.get_mut(sample_idx))
                {
                    *right = 0.0;
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

        if !runtime_status
            .lock()
            .map(|status| status.file_ready)
            .unwrap_or(false)
            && !task_running.load(Ordering::Relaxed)
        {
            self.maybe_queue_cache_task_from_editor(&async_executor);
        }

        let source = HTMLSource::URL(Self::resolve_gui_url());

        let editor = WebViewEditor::new(source, (GUI_WIDTH, GUI_HEIGHT))
            .with_developer_mode(true)
            .with_event_loop(move |ctx, setter, _window| {
                while let Ok(value) = ctx.next_event() {
                    if let Ok(action) = serde_json::from_value::<Action>(value) {
                        match action {
                            Action::Init => {
                                send_state_message(ctx, &params, &runtime_status);
                            }
                            Action::SetCoordinateMode { value } => {
                                setter.begin_set_parameter(&params.coordinate_mode);
                                setter.set_parameter(
                                    &params.coordinate_mode,
                                    CoordinateMode::from_id(&value),
                                );
                                setter.end_set_parameter(&params.coordinate_mode);
                            }
                            Action::SetAzimuth { value } => {
                                set_float_parameter(&setter, &params.azimuth, value);
                            }
                            Action::SetElevation { value } => {
                                set_float_parameter(&setter, &params.elevation, value);
                            }
                            Action::SetDistance { value } => {
                                set_float_parameter(&setter, &params.distance, value);
                            }
                            Action::SetRadius { value } => {
                                set_float_parameter(&setter, &params.radius, value);
                            }
                            Action::SetHeight { value } => {
                                set_float_parameter(&setter, &params.height, value);
                            }
                            Action::SetSourceYaw { value } => {
                                set_float_parameter(&setter, &params.source_yaw, value);
                            }
                            Action::SetDirectivity { value } => {
                                set_float_parameter(&setter, &params.directivity, value);
                            }
                            Action::SetOutputGain { value } => {
                                set_float_parameter(&setter, &params.output_gain, value);
                            }
                            Action::ValidateCache => {
                                if task_running
                                    .compare_exchange(
                                        false,
                                        true,
                                        Ordering::Relaxed,
                                        Ordering::Relaxed,
                                    )
                                    .is_ok()
                                {
                                    async_executor
                                        .execute_background(OpenSpatialTask::EnsureHrtfCache);
                                }
                            }
                        }
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

impl Vst3Plugin for OpenSpatial {
    const VST3_CLASS_ID: [u8; 16] = *b"WTH_OpenSpatial_";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Tools];
}

nih_export_vst3!(OpenSpatial);

impl ClapPlugin for OpenSpatial {
    const CLAP_ID: &'static str = "wthplugins.open_spatial";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Runtime-downloaded SOFA HRTF spatializer with source directivity");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;

    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Utility,
    ];
}

nih_export_clap!(OpenSpatial);

fn ensure_hrtf_cache_task(
    asset_cache_dir: &Arc<Mutex<String>>,
    runtime_status: &Arc<Mutex<RuntimeInitStatus>>,
    runtime_dirty: &Arc<AtomicBool>,
) {
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

    let hrtf_path = cache_root.join(HRTF_FILENAME);
    let manifest_path = cache_root.join(ASSET_MANIFEST_FILE);

    set_runtime_status(
        runtime_status,
        runtime_dirty,
        RuntimeInitStatus {
            stage: "validating".to_string(),
            message: "Validating runtime HRTF cache".to_string(),
            progress: None,
            downloaded_bytes: None,
            total_bytes: None,
            cache_path: cache_root.to_string_lossy().to_string(),
            hrtf_path: hrtf_path.to_string_lossy().to_string(),
            hrtf_url: HRTF_URL.to_string(),
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
                && manifest.hrtf_url == HRTF_URL
                && manifest.hrtf_filename == HRTF_FILENAME
            {
                let file_hash = sha256_of_file(&hrtf_path)?;
                let file_bytes = fs::metadata(&hrtf_path)
                    .map_err(|err| format!("Failed to stat cached HRTF: {err}"))?
                    .len();
                if file_hash == manifest.sha256_hex && file_bytes == manifest.bytes {
                    return Ok(RuntimeInitStatus {
                        stage: "cached".to_string(),
                        message: "Cached HRTF is valid, waiting for renderer load".to_string(),
                        progress: Some(1.0),
                        downloaded_bytes: Some(file_bytes),
                        total_bytes: Some(file_bytes),
                        cache_path: cache_root.to_string_lossy().to_string(),
                        hrtf_path: hrtf_path.to_string_lossy().to_string(),
                        hrtf_url: HRTF_URL.to_string(),
                        ready: false,
                        file_ready: true,
                    });
                }
            }
        }

        let temp_path = cache_root.join(format!("{HRTF_FILENAME}.download"));
        let response = ureq::get(HRTF_URL)
            .call()
            .map_err(|err| format!("Failed to download HRTF: {err}"))?;
        let total_bytes = response
            .header("Content-Length")
            .and_then(|value| value.parse::<u64>().ok());
        let mut reader = response.into_reader();
        let mut file =
            File::create(&temp_path).map_err(|err| format!("Failed to create temp file: {err}"))?;
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
                RuntimeInitStatus {
                    stage: "downloading".to_string(),
                    message: "Downloading HRTF asset".to_string(),
                    progress: total_bytes.map(|total| downloaded as f32 / total as f32),
                    downloaded_bytes: Some(downloaded),
                    total_bytes,
                    cache_path: cache_root.to_string_lossy().to_string(),
                    hrtf_path: hrtf_path.to_string_lossy().to_string(),
                    hrtf_url: HRTF_URL.to_string(),
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
        let manifest = AssetManifest {
            schema_version: 1,
            renderer_id: ANALYTIC_RENDERER_ID.to_string(),
            hrtf_url: HRTF_URL.to_string(),
            hrtf_filename: HRTF_FILENAME.to_string(),
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
            hrtf_url: HRTF_URL.to_string(),
            ready: false,
            file_ready: true,
        })
    })();

    match task_result {
        Ok(status) => set_runtime_status(runtime_status, runtime_dirty, status),
        Err(err) => set_runtime_status(
            runtime_status,
            runtime_dirty,
            RuntimeInitStatus {
                stage: "error".to_string(),
                message: err,
                progress: None,
                downloaded_bytes: None,
                total_bytes: None,
                cache_path: cache_root.to_string_lossy().to_string(),
                hrtf_path: hrtf_path.to_string_lossy().to_string(),
                hrtf_url: HRTF_URL.to_string(),
                ready: false,
                file_ready: false,
            },
        ),
    }
}

fn set_runtime_status(
    runtime_status: &Arc<Mutex<RuntimeInitStatus>>,
    runtime_dirty: &Arc<AtomicBool>,
    status: RuntimeInitStatus,
) {
    if let Ok(mut guard) = runtime_status.lock() {
        *guard = status;
    }
    runtime_dirty.store(true, Ordering::Relaxed);
}

fn read_manifest(path: &Path) -> Result<AssetManifest, String> {
    let bytes = fs::read(path).map_err(|err| format!("Failed to read asset manifest: {err}"))?;
    serde_json::from_slice(&bytes).map_err(|err| format!("Failed to parse asset manifest: {err}"))
}

fn sha256_of_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|err| format!("Failed to open cached HRTF: {err}"))?;
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

fn send_state_message(
    ctx: &WindowHandler,
    params: &Arc<OpenSpatialParams>,
    runtime_status: &Arc<Mutex<RuntimeInitStatus>>,
) {
    let mode = params.coordinate_mode.value();
    let runtime = runtime_status
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default();

    ctx.send_json(json!({
        "type": "State",
        "coordinateMode": mode.id(),
        "azimuth": params.azimuth.value(),
        "elevation": params.elevation.value(),
        "distance": params.distance.value(),
        "radius": params.radius.value(),
        "height": params.height.value(),
        "sourceYaw": params.source_yaw.value(),
        "directivity": params.directivity.value(),
        "outputGain": params.output_gain.value(),
        "pluginVersion": env!("CARGO_PKG_VERSION"),
        "rendererId": ANALYTIC_RENDERER_ID,
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
