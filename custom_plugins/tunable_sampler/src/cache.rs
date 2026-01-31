use crate::constants::CACHE_FOLDER_NAME;
use crate::types::{CachedSample, FolderSelectionResult, ResampledMetadata, SampleMetadata};
use base64::{engine::general_purpose, Engine as _};
use nih_plug_webview::WindowHandler;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub fn normalize_project_folder(path: PathBuf) -> Result<PathBuf, String> {
    if path.is_dir() {
        return Ok(path);
    }
    if path.is_file() {
        return path
            .parent()
            .map(|parent| parent.to_path_buf())
            .ok_or_else(|| "Dropped file has no parent directory".to_string());
    }
    Err("Selected path does not exist".to_string())
}

pub fn ensure_cache_folder(project_folder: &Path) -> Result<PathBuf, String> {
    let cache_folder = project_folder.join(CACHE_FOLDER_NAME);
    std::fs::create_dir_all(&cache_folder)
        .map_err(|err| format!("Failed to create cache folder: {err}"))?;
    Ok(cache_folder)
}

pub fn resolve_project_folder(
    project_folder: &Arc<Mutex<Option<String>>>,
    path: String,
) -> Result<(PathBuf, PathBuf), String> {
    let folder = normalize_project_folder(PathBuf::from(path))?;
    let cache_folder = ensure_cache_folder(&folder)?;
    if let Ok(mut guard) = project_folder.lock() {
        *guard = Some(folder.to_string_lossy().to_string());
    }
    Ok((folder, cache_folder))
}

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

pub fn build_project_state(
    project_folder: &Arc<Mutex<Option<String>>>,
) -> (Option<String>, Option<String>, Option<String>) {
    if let Ok(guard) = project_folder.lock() {
        if let Some(existing) = guard.as_ref() {
            let folder_path = PathBuf::from(existing);
            let project_name = folder_path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.to_string());
            let cache_folder = folder_path.join(CACHE_FOLDER_NAME);
            return (
                Some(existing.clone()),
                Some(cache_folder.to_string_lossy().to_string()),
                project_name,
            );
        }
    }

    (None, None, None)
}

pub fn cache_folder_from_params(project_folder: &Arc<Mutex<Option<String>>>) -> Option<PathBuf> {
    project_folder
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
        .map(PathBuf::from)
        .map(|folder| folder.join(CACHE_FOLDER_NAME))
}

pub fn load_cached_sample(cache_folder: &Path) -> Result<Option<CachedSample>, String> {
    let array_path = cache_folder.join("sample.array");
    let json_path = cache_folder.join("sample.json");
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
    cache_folder: &Path,
) -> Result<bool, String> {
    match load_cached_sample(cache_folder)? {
        Some(sample) => {
            ctx.send_json(json!({
                "type": "CachedSample",
                "name": sample.name,
                "sample_rate": sample.sample_rate,
                "channels": sample.channels,
                "frames": sample.frames,
                "data_base64": sample.data_base64,
            }));
            Ok(true)
        }
        None => Ok(false),
    }
}

pub fn save_sample_to_cache(
    cache_folder: &Path,
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

    let array_path = cache_folder.join("sample.array");
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
    let json_path = cache_folder.join("sample.json");
    let json_bytes = serde_json::to_vec_pretty(&metadata)
        .map_err(|err| format!("Failed to serialize sample.json: {err}"))?;
    std::fs::write(&json_path, json_bytes)
        .map_err(|err| format!("Failed to write sample.json: {err}"))?;

    Ok(())
}

pub fn sample_cache_exists(cache_folder: &Path) -> bool {
    cache_folder.join("sample.array").is_file() && cache_folder.join("sample.json").is_file()
}

pub fn load_sample_data(cache_folder: &Path) -> Result<(SampleMetadata, Vec<f32>), String> {
    let array_path = cache_folder.join("sample.array");
    let json_path = cache_folder.join("sample.json");
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

pub fn load_resampled_metadata(cache_folder: &Path) -> Result<Option<ResampledMetadata>, String> {
    let json_path = cache_folder.join("resampled.json");
    if !json_path.is_file() {
        return Ok(None);
    }
    let metadata_bytes = std::fs::read(&json_path)
        .map_err(|err| format!("Failed to read resampled.json: {err}"))?;
    let metadata: ResampledMetadata = serde_json::from_slice(&metadata_bytes)
        .map_err(|err| format!("Failed to parse resampled.json: {err}"))?;
    Ok(Some(metadata))
}
