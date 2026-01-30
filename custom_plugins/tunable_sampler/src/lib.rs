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
struct TunableSamplerParams {}

impl Default for TunableSamplerParams {
    fn default() -> Self {
        Self {}
    }
}

pub struct TunableSampler {
    params: Arc<TunableSamplerParams>,
    project_folder: Arc<Mutex<Option<PathBuf>>>,
    pending_folder_result: Arc<Mutex<Option<FolderSelectionResult>>>,
    pending_folder_dirty: Arc<AtomicBool>,
}

impl Default for TunableSampler {
    fn default() -> Self {
        let project_folder = Arc::new(Mutex::new(None));
        let pending_folder_result = Arc::new(Mutex::new(None));
        let pending_folder_dirty = Arc::new(AtomicBool::new(false));

        Self {
            params: Arc::new(TunableSamplerParams::default()),
            project_folder,
            pending_folder_result,
            pending_folder_dirty,
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Action {
    Init,
    RequestPluginInfo,
}

enum FolderSelectionResult {
    Selected {
        folder: PathBuf,
        cache_folder: PathBuf,
    },
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
        project_folder: &Arc<Mutex<Option<PathBuf>>>,
        path: PathBuf,
    ) -> Result<(PathBuf, PathBuf), String> {
        let folder = Self::normalize_project_folder(path)?;
        let cache_folder = Self::ensure_cache_folder(&folder)?;
        if let Ok(mut guard) = project_folder.lock() {
            *guard = Some(folder.clone());
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

    fn send_plugin_info(
        ctx: &WindowHandler,
        project_folder: &Arc<Mutex<Option<PathBuf>>>,
    ) {
        ctx.send_json(json!({
            "type": "PluginInfo",
            "pluginVersion": env!("CARGO_PKG_VERSION"),
        }));
        if let Ok(guard) = project_folder.lock() {
            if let Some(existing) = guard.as_ref() {
                let cache_folder = existing.join(CACHE_FOLDER_NAME);
                ctx.send_json(json!({
                    "type": "ProjectFolderSelected",
                    "path": existing.to_string_lossy(),
                    "cachePath": cache_folder.to_string_lossy(),
                }));
            }
        }
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
        let project_folder = self.project_folder.clone();
        let pending_folder_result = self.pending_folder_result.clone();
        let pending_folder_dirty = self.pending_folder_dirty.clone();

        let source = HTMLSource::URL(Self::resolve_gui_url());
        let editor = WebViewEditor::new(source, (GUI_WIDTH, GUI_HEIGHT))
            .with_developer_mode(true)
            .with_mouse_handler({
                let project_folder = project_folder.clone();
                let pending_folder_result = pending_folder_result.clone();
                let pending_folder_dirty = pending_folder_dirty.clone();
                move |event| match event {
                    MouseEvent::DragEntered { .. } | MouseEvent::DragMoved { .. } => {
                        EventStatus::AcceptDrop(DropEffect::Copy)
                    }
                    MouseEvent::DragLeft => EventStatus::Ignored,
                    MouseEvent::DragDropped { data, .. } => {
                        if let DropData::Files(files) = data {
                            if let Some(path) = files.into_iter().next() {
                                let project_folder = project_folder.clone();
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
                            } else {
                                TunableSampler::queue_folder_result(
                                    &pending_folder_result,
                                    &pending_folder_dirty,
                                    FolderSelectionResult::Error {
                                        message: "Dropped data did not include any files."
                                            .to_string(),
                                    },
                                );
                            }
                        }

                        EventStatus::AcceptDrop(DropEffect::Copy)
                    }
                    _ => EventStatus::Ignored,
                }
            })
            .with_event_loop(move |ctx, _setter, _window| {
                while let Ok(value) = ctx.next_event() {
                    if let Ok(action) = serde_json::from_value::<Action>(value) {
                        match action {
                            Action::Init => {
                                TunableSampler::send_plugin_info(ctx, &project_folder);
                            }
                            Action::RequestPluginInfo => {
                                TunableSampler::send_plugin_info(ctx, &project_folder);
                            }
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
                                    ctx.send_json(json!({
                                        "type": "ProjectFolderSelected",
                                        "path": folder.to_string_lossy(),
                                        "cachePath": cache_folder.to_string_lossy(),
                                    }));
                                }
                                FolderSelectionResult::Error { message } => {
                                    ctx.send_json(json!({
                                        "type": "ProjectFolderError",
                                        "message": message,
                                    }));
                                }
                            }
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
