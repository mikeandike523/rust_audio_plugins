
use nih_plug::prelude::*;
use nih_plug_webview::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{num::NonZeroU32, sync::Arc};

pub struct ProgFilt {
    params: Arc<ProgFiltParams>,
}

impl Default for ProgFilt {
    fn default() -> Self {
        Self {
            params: Arc::new(ProgFiltParams::default()),
        }
    }
}

impl Plugin for ProgFilt {
    const NAME: &'static str = "ProgFilt";
    const VENDOR: &'static str = "WTH Plugins";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // Stereo in → Stereo out
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _ctx: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let db = self.params.gain_db.unmodulated_plain_value();
        let gain = util::db_to_gain(db);
        for channel_samples in buffer.iter_samples() {
            for sample in channel_samples {
                *sample *= gain;
            }
        }
        ProcessStatus::Normal
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        // WebView-based GUI (separate dev server or bundled)
        let params = self.params.clone();
        let version = Self::VERSION;
        let editor = WebViewEditor::new(HTMLSource::URL("http://localhost:5173"), (800, 600))
            .with_event_loop(move |ctx, setter, _window| {
                // handle IPC messages from the UI
                while let Ok(value) = ctx.next_event() {
                    if let Ok(msg) = serde_json::from_value::<PluginMessage>(value) {
                        match msg {
                            PluginMessage::QueryGain => {
                                let db = params.gain_db.unmodulated_plain_value();
                                ctx.send_json(json!({ "type": "RespondGain", "gain": db }));
                            }
                            PluginMessage::SetGainDB { gain } => {
                                setter.begin_set_parameter(&params.gain_db);
                                setter.set_parameter_normalized(
                                    &params.gain_db,
                                    params.gain_db.preview_normalized(gain),
                                );
                                setter.end_set_parameter(&params.gain_db);
                            }
                            PluginMessage::QueryCargoPackageVersion => {
                                ctx.send_json(json!({
                                    "type": "RespondCargoPackageVersion",
                                    "version": version,
                                }));
                            }
                        }
                    }
                }
            });
        Some(Box::new(editor))
    }
}

// VST3 support
impl Vst3Plugin for ProgFilt {
    const VST3_CLASS_ID: [u8; 16] = *b"WTH_ProgFilt_FX_";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Fx,
        Vst3SubCategory::Stereo,
    ];
}
nih_export_vst3!(ProgFilt);

// CLAP support
impl ClapPlugin for ProgFilt {
    const CLAP_ID: &'static str = "wthplugins.progfilt";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Stereo identity filter (no-op)");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
    ];
}
nih_export_clap!(ProgFilt);

// Plugin parameters: gain in decibels
#[derive(Params)]
struct ProgFiltParams {
    /// Gain in decibels (-30 to 0 dB)
    #[id = "gain_db"]
    #[min = -30.0]
    #[max = 0.0]
    #[default = 0.0]
    #[unit = "dB"]
    pub gain_db: FloatParam,
}

/// Messages received from the front-end UI.
#[derive(Deserialize)]
#[serde(tag = "type")]
enum PluginMessage {
    QueryGain,
    SetGainDB { gain: f32 },
    QueryCargoPackageVersion,
}
