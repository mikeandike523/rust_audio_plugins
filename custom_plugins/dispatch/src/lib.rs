use nih_plug::prelude::*;
use nih_plug_webview::*;
use serde::Deserialize;
use serde_json::json;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const GUI_WIDTH: u32 = 960;
const GUI_HEIGHT: u32 = 640;
const GUI_DEV_SERVER_URL: &str = "http://localhost:5173";
const GUI_DEV_SERVER_ROUTE: &str = "/wth-dispatch";
const GUI_DEV_SERVER_PROBE_URL: &str = "http://localhost:5173/wth-dispatch";
const GUI_PUBLISHED_URL: &str = "https://dispatch-web-gui.vercel.app";

// ---------------------------------------------------------------------------
// Config persistence
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
    // Windows: %APPDATA%\dispatch\config.json
    // Unix:    ~/.config/dispatch/config.json
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
    let Some(path) = config_file_path() else {
        return Config::default();
    };
    std::fs::read_to_string(&path)
        .ok()
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
// Plugin params (empty for now — drum rack uses MIDI, not param automation)
// ---------------------------------------------------------------------------

#[derive(Params)]
struct DispatchParams {}

impl Default for DispatchParams {
    fn default() -> Self {
        Self {}
    }
}

// ---------------------------------------------------------------------------
// Plugin struct
// ---------------------------------------------------------------------------

pub struct Dispatch {
    params: Arc<DispatchParams>,
    config: Arc<Mutex<Config>>,
}

impl Default for Dispatch {
    fn default() -> Self {
        Self {
            params: Arc::new(DispatchParams::default()),
            config: Arc::new(Mutex::new(load_config())),
        }
    }
}

// ---------------------------------------------------------------------------
// IPC: messages from the UI
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Action {
    /// UI has loaded — respond with current state
    Init,
    /// User confirmed a cache directory path
    SetCacheDir { path: String },
    /// User cleared the cache directory (prompt again next time)
    ClearCacheDir,
}

// ---------------------------------------------------------------------------
// GUI URL resolution (dev probe vs. production)
// ---------------------------------------------------------------------------

impl Dispatch {
    fn resolve_gui_url() -> &'static str {
        match std::thread::spawn(move || {
            use std::time::Duration;
            let client = Arc::new(
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
                    println!(
                        "Dev server detected at {}{}",
                        GUI_DEV_SERVER_URL, GUI_DEV_SERVER_ROUTE
                    );
                    GUI_DEV_SERVER_PROBE_URL
                } else {
                    println!(
                        "Dev server response not text ({}), using production URL: {}",
                        content_type, GUI_PUBLISHED_URL
                    );
                    GUI_PUBLISHED_URL
                }
            }
            _ => {
                println!(
                    "Dev server not available, using production URL: {}",
                    GUI_PUBLISHED_URL
                );
                GUI_PUBLISHED_URL
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin trait impl
// ---------------------------------------------------------------------------

impl Plugin for Dispatch {
    const NAME: &'static str = "Dispatch";
    const VENDOR: &'static str = "WTH Plugins";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // Instrument: no audio input, stereo output
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
        _buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        true
    }

    fn reset(&mut self) {}

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Drain MIDI events (pad triggering will be wired here in a future feature)
        while let Some(event) = context.next_event() {
            match event {
                NoteEvent::NoteOn { note, velocity, .. } => {
                    let _ = (note, velocity);
                    // TODO: trigger pad, mix sample into output buffer
                }
                _ => {}
            }
        }

        // Output silence until sample playback is implemented
        for channel_samples in buffer.iter_samples() {
            for sample in channel_samples {
                *sample = 0.0;
            }
        }

        ProcessStatus::Normal
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let config = self.config.clone();
        let source = HTMLSource::URL(Self::resolve_gui_url());

        let editor = WebViewEditor::new(source, (GUI_WIDTH, GUI_HEIGHT))
            .with_developer_mode(true)
            .with_event_loop(move |ctx, _setter, _window| {
                while let Ok(value) = ctx.next_event() {
                    if let Ok(action) = serde_json::from_value::<Action>(value) {
                        match action {
                            Action::Init => {
                                let cfg = config.lock().unwrap();
                                ctx.send_json(json!({
                                    "type": "State",
                                    "cacheDir": cfg.cache_dir,
                                    "needsCacheDir": cfg.cache_dir.is_none(),
                                    "pluginVersion": env!("CARGO_PKG_VERSION"),
                                }));
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
                                let mut cfg = config.lock().unwrap();
                                cfg.cache_dir = None;
                                save_config(&cfg);
                                ctx.send_json(json!({
                                    "type": "State",
                                    "cacheDir": null,
                                    "needsCacheDir": true,
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
