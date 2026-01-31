use base64::{engine::general_purpose, Engine as _};
use nih_plug::prelude::*;
use nih_plug_webview::*;
use serde::Deserialize;
use serde_json::json;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const GUI_WIDTH: u32 = 800;
const GUI_HEIGHT: u32 = 800;
const GUI_DEV_SERVER_URL: &str = "http://localhost:5173";
const GUI_PUBLISHED_URL: &str = "https://tunable-sampler-web-gui.vercel.app";
const CACHE_FOLDER_NAME: &str = "tunable_sampler_cache";

#[derive(Params)]
struct TunableSamplerParams {
    #[id = "gain"]
    gain: FloatParam,
    gain_changed: Arc<AtomicBool>,
    #[persist = "project_folder"]
    project_folder: Arc<Mutex<Option<String>>>,
}

impl Default for TunableSamplerParams {
    fn default() -> Self {
        let gain_changed = Arc::new(AtomicBool::new(false));
        let gain_changed_cb = gain_changed.clone();
        let gain_callback = Arc::new(move |_: f32| {
            gain_changed_cb.store(true, Ordering::Relaxed);
        });

        Self {
            gain: FloatParam::new(
                "Gain",
                0.0,
                FloatRange::Linear {
                    min: -24.0,
                    max: 24.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(5.0))
            .with_step_size(0.1)
            .with_unit(" dB")
            .with_callback(gain_callback),
            gain_changed,
            project_folder: Arc::new(Mutex::new(None)),
        }
    }
}

pub struct TunableSampler {
    params: Arc<TunableSamplerParams>,
    pending_folder_result: Arc<Mutex<Option<FolderSelectionResult>>>,
    pending_folder_dirty: Arc<AtomicBool>,
}

impl Default for TunableSampler {
    fn default() -> Self {
        let pending_folder_result = Arc::new(Mutex::new(None));
        let pending_folder_dirty = Arc::new(AtomicBool::new(false));

        Self {
            params: Arc::new(TunableSamplerParams::default()),
            pending_folder_result,
            pending_folder_dirty,
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Action {
    Init,
    RequestState,
    PickProjectFolder,
    SetProjectFolder { path: String },
    SetGain { value: f32 },
    SaveSample {
        name: String,
        sample_rate: u32,
        channels: u16,
        frames: u32,
        data_base64: String,
    },
}

enum FolderSelectionResult {
    Selected {
        folder: PathBuf,
        cache_folder: PathBuf,
    },
    Canceled,
    Error {
        message: String,
    },
}

impl TunableSampler {
    fn normalize_project_folder(path: PathBuf) -> Result<PathBuf, String> {
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

    fn ensure_cache_folder(project_folder: &Path) -> Result<PathBuf, String> {
        let cache_folder = project_folder.join(CACHE_FOLDER_NAME);
        std::fs::create_dir_all(&cache_folder)
            .map_err(|err| format!("Failed to create cache folder: {err}"))?;
        Ok(cache_folder)
    }

    fn resolve_project_folder(
        project_folder: &Arc<Mutex<Option<String>>>,
        path: String,
    ) -> Result<(PathBuf, PathBuf), String> {
        let folder = Self::normalize_project_folder(PathBuf::from(path))?;
        let cache_folder = Self::ensure_cache_folder(&folder)?;
        if let Ok(mut guard) = project_folder.lock() {
            *guard = Some(folder.to_string_lossy().to_string());
        }
        Ok((folder, cache_folder))
    }

    fn queue_folder_result(
        pending_folder_result: &Arc<Mutex<Option<FolderSelectionResult>>>,
        pending_folder_dirty: &Arc<AtomicBool>,
        result: FolderSelectionResult,
    ) {
        if let Ok(mut guard) = pending_folder_result.lock() {
            *guard = Some(result);
        }
        pending_folder_dirty.store(true, Ordering::Relaxed);
    }

    fn build_project_state(
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

    fn send_state(ctx: &WindowHandler, params: &Arc<TunableSamplerParams>) {
        let (project_folder, cache_folder, project_name) =
            Self::build_project_state(&params.project_folder);

        ctx.send_json(json!({
            "type": "State",
            "pluginVersion": env!("CARGO_PKG_VERSION"),
            "projectFolder": project_folder,
            "cachePath": cache_folder,
            "projectName": project_name,
            "gain": params.gain.value(),
        }));
    }

    fn save_sample_to_cache(
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
            Ok(Ok(response)) => GUI_DEV_SERVER_URL,
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
        _buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let _ = context;
        for channel in buffer.as_slice().iter_mut() {
            channel.fill(0.0);
        }

        ProcessStatus::Normal
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let pending_folder_result = self.pending_folder_result.clone();
        let pending_folder_dirty = self.pending_folder_dirty.clone();

        let source = HTMLSource::URL(Self::resolve_gui_url());
        let editor = WebViewEditor::new(source, (GUI_WIDTH, GUI_HEIGHT))
            .with_developer_mode(true)
            .with_event_loop(move |ctx, setter, _window| {
                while let Ok(value) = ctx.next_event() {
                    match serde_json::from_value::<Action>(value) {
                        Ok(action) => match action {
                            Action::Init => {
                                TunableSampler::send_state(ctx, &params);
                            }
                            Action::RequestState => {
                                TunableSampler::send_state(ctx, &params);
                            }
                            Action::PickProjectFolder => {
                                let project_folder = params.project_folder.clone();
                                let pending_folder_result = pending_folder_result.clone();
                                let pending_folder_dirty = pending_folder_dirty.clone();
                                std::thread::spawn(move || {
                                    let default_path =
                                        project_folder.lock().ok().and_then(|guard| guard.clone());
                                    let selection = tinyfiledialogs::select_folder_dialog(
                                        "Select project folder",
                                        default_path.as_deref().unwrap_or(""),
                                    );
                                    let result = match selection {
                                        Some(path) => TunableSampler::resolve_project_folder(
                                            &project_folder,
                                            path,
                                        )
                                        .map(|(folder, cache_folder)| {
                                            FolderSelectionResult::Selected {
                                                folder,
                                                cache_folder,
                                            }
                                        })
                                        .unwrap_or_else(|message| {
                                            FolderSelectionResult::Error { message }
                                        }),
                                        None => FolderSelectionResult::Canceled,
                                    };
                                    TunableSampler::queue_folder_result(
                                        &pending_folder_result,
                                        &pending_folder_dirty,
                                        result,
                                    );
                                });
                            }
                            Action::SetProjectFolder { path } => {
                                let project_folder = params.project_folder.clone();
                                let pending_folder_result = pending_folder_result.clone();
                                let pending_folder_dirty = pending_folder_dirty.clone();
                                std::thread::spawn(move || {
                                    let result = TunableSampler::resolve_project_folder(
                                        &project_folder,
                                        path,
                                    );
                                    TunableSampler::queue_folder_result(
                                        &pending_folder_result,
                                        &pending_folder_dirty,
                                        match result {
                                            Ok((folder, cache_folder)) => {
                                                FolderSelectionResult::Selected {
                                                    folder,
                                                    cache_folder,
                                                }
                                            }
                                            Err(message) => {
                                                FolderSelectionResult::Error { message }
                                            }
                                        },
                                    );
                                });
                            }
                            Action::SetGain { value } => {
                                setter.begin_set_parameter(&params.gain);
                                setter.set_parameter(&params.gain, value);
                                setter.end_set_parameter(&params.gain);
                            }
                            Action::SaveSample {
                                name,
                                sample_rate,
                                channels,
                                frames,
                                data_base64,
                            } => {
                                let project_folder = params
                                    .project_folder
                                    .lock()
                                    .ok()
                                    .and_then(|guard| guard.clone());
                                let Some(project_folder) = project_folder else {
                                    ctx.send_json(json!({
                                        "type": "SampleSaveError",
                                        "message": "Project folder not set.",
                                    }));
                                    continue;
                                };

                                let cache_folder =
                                    match Self::ensure_cache_folder(&PathBuf::from(project_folder))
                                    {
                                        Ok(folder) => folder,
                                        Err(message) => {
                                            ctx.send_json(json!({
                                                "type": "SampleSaveError",
                                                "message": message,
                                            }));
                                            continue;
                                        }
                                    };

                                match Self::save_sample_to_cache(
                                    &cache_folder,
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
                                FolderSelectionResult::Selected {
                                    folder,
                                    cache_folder,
                                } => {
                                    let project_name = folder
                                        .file_name()
                                        .and_then(|name| name.to_str())
                                        .map(|name| name.to_string());
                                    ctx.send_json(json!({
                                        "type": "State",
                                        "projectFolder": folder.to_string_lossy(),
                                        "cachePath": cache_folder.to_string_lossy(),
                                        "projectName": project_name,
                                    }));
                                }
                                FolderSelectionResult::Error { message } => {
                                    ctx.send_json(json!({
                                        "type": "ProjectFolderError",
                                        "message": message,
                                    }));
                                }
                                FolderSelectionResult::Canceled => {
                                    ctx.send_json(json!({
                                        "type": "ProjectFolderCanceled",
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
