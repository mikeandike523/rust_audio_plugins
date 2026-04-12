use crate::types::{CachedSample, FolderSelectionResult, ResampledMetadata, SampleMetadata};
use base64::{engine::general_purpose, Engine as _};
use directories::ProjectDirs;
use nih_plug_webview::WindowHandler;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// Cache directory resolution
// ---------------------------------------------------------------------------

pub fn default_cache_dir() -> PathBuf {
    if let Some(proj) = ProjectDirs::from("com", "WTH Plugins", "TunableSampler") {
        proj.data_local_dir().join("cache")
    } else {
        PathBuf::from("tunable_sampler_cache")
    }
}

pub fn default_webview_userdata_dir() -> PathBuf {
    if let Some(proj) = ProjectDirs::from("com", "WTH Plugins", "TunableSampler") {
        proj.data_local_dir().join("webview_userdata")
    } else {
        std::env::temp_dir().join("tunable_sampler_webview_userdata")
    }
}

pub fn effective_cache_dir(override_dir: &Option<String>) -> PathBuf {
    match override_dir {
        Some(s) => PathBuf::from(s),
        None => default_cache_dir(),
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
    static CTR: AtomicU32 = AtomicU32::new(0);
    CTR.fetch_add(1, Ordering::Relaxed).hash(&mut h);
    format!("{:08x}", h.finish() as u32)
}

/// Generate a cache key that does not already exist as a subdirectory of `cache_dir`.
pub fn new_unique_cache_key(cache_dir: &Path) -> String {
    loop {
        let key = random_hex8();
        if !cache_dir.join(&key).exists() {
            return key;
        }
    }
}

// ---------------------------------------------------------------------------
// Sample dir helpers
// ---------------------------------------------------------------------------

pub fn sample_dir(cache_dir: &Path, uuid: &str) -> PathBuf {
    cache_dir.join(uuid)
}

pub fn get_sample_dir(
    cache_dir: &Arc<Mutex<Option<String>>>,
    sample_uuid: &Arc<Mutex<Option<String>>>,
) -> Option<PathBuf> {
    let override_dir = cache_dir.lock().ok().and_then(|g| g.clone());
    let cache = effective_cache_dir(&override_dir);
    let uuid = sample_uuid.lock().ok().and_then(|g| g.clone())?;
    Some(sample_dir(&cache, &uuid))
}

// ---------------------------------------------------------------------------
// Async folder picker result queue
// ---------------------------------------------------------------------------

pub fn queue_folder_result(
    pending_folder_result: &Arc<Mutex<Option<FolderSelectionResult>>>,
    pending_folder_dirty: &Arc<AtomicBool>,
    result: FolderSelectionResult,
) {
    if let Ok(mut guard) = pending_folder_result.lock() {
        *guard = Some(result);
    }
    pending_folder_dirty.store(true, Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// Cache I/O
// ---------------------------------------------------------------------------

pub fn load_cached_sample(sample_dir: &Path) -> Result<Option<CachedSample>, String> {
    let array_path = sample_dir.join("sample.array");
    let json_path = sample_dir.join("sample.json");
    if !(array_path.is_file() && json_path.is_file()) {
        return Ok(None);
    }

    let metadata_bytes = std::fs::read(&json_path)
        .map_err(|err| format!("Failed to read sample.json: {err}"))?;
    let metadata: SampleMetadata = serde_json::from_slice(&metadata_bytes)
        .map_err(|err| format!("Failed to parse sample.json: {err}"))?;

    if metadata.sample_rate == 0 {
        return Err("Sample rate cannot be zero.".to_string());
    }

    let data = std::fs::read(&array_path)
        .map_err(|err| format!("Failed to read sample.array: {err}"))?;
    let expected_len = metadata.frames as u64 * metadata.channels as u64 * 4;
    if data.len() as u64 != expected_len {
        return Err(format!(
            "Sample cache size mismatch (expected {expected_len} bytes, got {})",
            data.len()
        ));
    }

    Ok(Some(CachedSample {
        name: metadata.name,
        sample_rate: metadata.sample_rate,
        channels: metadata.channels,
        frames: metadata.frames,
        data_base64: general_purpose::STANDARD.encode(data),
    }))
}

pub fn send_cached_sample_if_available(
    ctx: &WindowHandler,
    sample_dir: &Path,
    sample_start: f32,
    sample_end: f32,
) -> Result<bool, String> {
    match load_cached_sample(sample_dir)? {
        Some(sample) => {
            ctx.send_json(json!({
                "type": "CachedSample",
                "name": sample.name,
                "sample_rate": sample.sample_rate,
                "channels": sample.channels,
                "frames": sample.frames,
                "data_base64": sample.data_base64,
                "sample_start": sample_start,
                "sample_end": sample_end,
            }));
            Ok(true)
        }
        None => Ok(false),
    }
}

pub fn save_sample_to_cache(
    sample_dir: &Path,
    name: &str,
    sample_rate: u32,
    channels: u16,
    frames: u32,
    data_base64: &str,
) -> Result<(), String> {
    if sample_rate == 0 {
        return Err("Sample rate cannot be zero.".to_string());
    }
    let decoded = general_purpose::STANDARD
        .decode(data_base64.as_bytes())
        .map_err(|err| format!("Failed to decode sample data: {err}"))?;
    let expected_len = frames as u64 * channels as u64 * 4;
    if decoded.len() as u64 != expected_len {
        return Err(format!(
            "Sample data size mismatch (expected {expected_len} bytes, got {})",
            decoded.len()
        ));
    }

    std::fs::create_dir_all(sample_dir)
        .map_err(|err| format!("Failed to create sample dir: {err}"))?;

    let array_path = sample_dir.join("sample.array");
    std::fs::write(&array_path, decoded)
        .map_err(|err| format!("Failed to write sample.array: {err}"))?;

    let metadata = json!({
        "name": name,
        "sample_rate": sample_rate,
        "channels": channels,
        "frames": frames,
        "length_seconds": frames as f32 / sample_rate as f32,
        "format": "f32le",
        "layout": "interleaved"
    });
    let json_path = sample_dir.join("sample.json");
    let json_bytes = serde_json::to_vec_pretty(&metadata)
        .map_err(|err| format!("Failed to serialize sample.json: {err}"))?;
    std::fs::write(&json_path, json_bytes)
        .map_err(|err| format!("Failed to write sample.json: {err}"))?;

    Ok(())
}

pub fn sample_cache_exists(sample_dir: &Path) -> bool {
    sample_dir.join("sample.array").is_file() && sample_dir.join("sample.json").is_file()
}

pub fn load_sample_data(sample_dir: &Path) -> Result<(SampleMetadata, Vec<f32>), String> {
    let array_path = sample_dir.join("sample.array");
    let json_path = sample_dir.join("sample.json");
    if !(array_path.is_file() && json_path.is_file()) {
        return Err("Sample cache is missing.".to_string());
    }

    let metadata_bytes = std::fs::read(&json_path)
        .map_err(|err| format!("Failed to read sample.json: {err}"))?;
    let metadata: SampleMetadata = serde_json::from_slice(&metadata_bytes)
        .map_err(|err| format!("Failed to parse sample.json: {err}"))?;

    if metadata.sample_rate == 0 {
        return Err("Sample rate cannot be zero.".to_string());
    }

    let data_bytes =
        std::fs::read(&array_path).map_err(|err| format!("Failed to read sample.array: {err}"))?;
    let expected_len = metadata.frames as usize * metadata.channels as usize * 4;
    if data_bytes.len() != expected_len {
        return Err(format!(
            "Sample cache size mismatch (expected {expected_len} bytes, got {}).",
            data_bytes.len()
        ));
    }

    let mut data = Vec::with_capacity(data_bytes.len() / 4);
    for chunk in data_bytes.chunks_exact(4) {
        data.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }

    Ok((metadata, data))
}

pub fn load_resampled_metadata(sample_dir: &Path) -> Result<Option<ResampledMetadata>, String> {
    let json_path = sample_dir.join("resampled.json");
    if !json_path.is_file() {
        return Ok(None);
    }
    let metadata_bytes = std::fs::read(&json_path)
        .map_err(|err| format!("Failed to read resampled.json: {err}"))?;
    let metadata: ResampledMetadata = serde_json::from_slice(&metadata_bytes)
        .map_err(|err| format!("Failed to parse resampled.json: {err}"))?;
    Ok(Some(metadata))
}

pub fn load_resampled_data(sample_dir: &Path) -> Result<(ResampledMetadata, Vec<f32>), String> {
    let array_path = sample_dir.join("resampled.array");
    let json_path = sample_dir.join("resampled.json");
    if !(array_path.is_file() && json_path.is_file()) {
        return Err("Resampled cache is missing.".to_string());
    }

    let metadata_bytes = std::fs::read(&json_path)
        .map_err(|err| format!("Failed to read resampled.json: {err}"))?;
    let metadata: ResampledMetadata = serde_json::from_slice(&metadata_bytes)
        .map_err(|err| format!("Failed to parse resampled.json: {err}"))?;

    if metadata.sample_rate == 0 {
        return Err("Resampled sample rate cannot be zero.".to_string());
    }

    let data_bytes = std::fs::read(&array_path)
        .map_err(|err| format!("Failed to read resampled.array: {err}"))?;
    let expected_len = metadata.frames as usize * metadata.channels as usize * 4;
    if data_bytes.len() != expected_len {
        return Err(format!(
            "Resampled cache size mismatch (expected {expected_len} bytes, got {}).",
            data_bytes.len()
        ));
    }

    let mut data = Vec::with_capacity(data_bytes.len() / 4);
    for chunk in data_bytes.chunks_exact(4) {
        data.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }

    Ok((metadata, data))
}
