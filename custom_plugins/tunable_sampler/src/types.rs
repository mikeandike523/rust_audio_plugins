use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
#[serde(tag = "type")]
pub enum Action {
    Init,
    RequestState,
    PickCacheDir,
    SetCacheDir { path: String },
    ClearCacheDir,
    SetGain { value: f32 },
    SetSampleStart { value: f32 },
    SetSampleEnd { value: f32 },
    SetResampleQualityInput { quality: u32 },
    SetResampleQualityPitch { quality: u32 },
    ForceResample,
    SaveSample {
        name: String,
        sample_rate: u32,
        channels: u16,
        frames: u32,
        data_base64: String,
    },
    RequestPitchEstimate { sample_start: f32 },
    SetDetune { value: f32 },
}

#[derive(Deserialize)]
pub struct SampleMetadata {
    pub name: Option<String>,
    pub sample_rate: u32,
    pub channels: u16,
    pub frames: u32,
}

pub struct CachedSample {
    pub name: Option<String>,
    pub sample_rate: u32,
    pub channels: u16,
    pub frames: u32,
    pub data_base64: String,
}

#[derive(Deserialize)]
pub struct ResampledMetadata {
    pub name: Option<String>,
    pub sample_rate: u32,
    pub channels: u16,
    pub frames: u32,
    pub source_sample_rate: u32,
    pub source_frames: u32,
    /// Quality preset index (0=Normal, 1=High, 2=UltraHigh).
    /// Defaults to 0 so old cache files (missing this field) always trigger a re-resample.
    #[serde(default)]
    pub quality: u32,
}

pub enum ResampleEvent {
    Started { label: String },
    Progress { progress: f32 },
    Completed { message: String },
    Error { message: String },
}

pub enum PitchEvent {
    Detected { hz: f64 },
    NoResult,
    Error { message: String },
}

pub enum FolderSelectionResult {
    Selected { path: PathBuf },
    Canceled,
    Error { message: String },
}
