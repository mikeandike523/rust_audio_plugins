use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
#[serde(tag = "type")]
pub enum Action {
    Init,
    RequestState,
    PickProjectFolder,
    SetProjectFolder { path: String },
    SetGain { value: f32 },
    SetResamplePointsInput { points: u32 },
    SetResamplePointsPitch { points: u32 },
    SaveSample {
        name: String,
        sample_rate: u32,
        channels: u16,
        frames: u32,
        data_base64: String,
    },
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
    pub points: u32,
}

pub enum ResampleEvent {
    Started { label: String },
    Progress { progress: f32 },
    Completed { message: String },
    Error { message: String },
}

pub enum FolderSelectionResult {
    Selected {
        folder: PathBuf,
        cache_folder: PathBuf,
    },
    Canceled,
    Error {
        message: String,
    },
}
