use nih_plug::prelude::*;
use nih_plug_webview::*;
use serde::Deserialize;
use serde_json::json;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const GUI_WIDTH: u32 = 420;
const GUI_HEIGHT: u32 = 240;
const GUI_HTML: &str = include_str!("../web-gui/index.html");
const GUI_DEV_SERVER_URL: &str = "http://localhost:5173";

#[derive(Params)]
struct DispatchParams {
    #[id = "gain"]
    gain: FloatParam,
    gain_changed: Arc<AtomicBool>,
}

impl Default for DispatchParams {
    fn default() -> Self {
        let gain_changed = Arc::new(AtomicBool::new(false));
        let changed = gain_changed.clone();

        let param_callback = Arc::new(move |_: f32| {
            changed.store(true, Ordering::Relaxed);
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
            .with_callback(param_callback),
            gain_changed,
        }
    }
}

pub struct Dispatch {
    params: Arc<DispatchParams>,
}

impl Default for Dispatch {
    fn default() -> Self {
        Self {
            params: Arc::new(DispatchParams::default()),
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Action {
    Init,
    SetGain { value: f32 },
}

impl Plugin for Dispatch {
    const NAME: &'static str = "Dispatch";
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
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        for (_sample_id, mut channels) in buffer.iter_samples().enumerate() {
            let gain = util::db_to_gain_fast(self.params.gain.smoothed.next());
            for sample in channels.iter_mut() {
                *sample *= gain;
            }
        }

        ProcessStatus::Normal
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let gain_changed = self.params.gain_changed.clone();

        let source = if std::env::var("DISPATCH_GUI_DEV_SERVER").as_deref() == Ok("1") {
            HTMLSource::URL(GUI_DEV_SERVER_URL)
        } else {
            HTMLSource::String(GUI_HTML)
        };

        let editor = WebViewEditor::new(source, (GUI_WIDTH, GUI_HEIGHT))
            .with_developer_mode(true)
            .with_event_loop(move |ctx, setter, _window| {
                while let Ok(value) = ctx.next_event() {
                    if let Ok(action) = serde_json::from_value::<Action>(value) {
                        match action {
                            Action::Init => {
                                ctx.send_json(json!({
                                    "type": "ParamChange",
                                    "value": params.gain.value(),
                                }));
                            }
                            Action::SetGain { value } => {
                                setter.begin_set_parameter(&params.gain);
                                setter.set_parameter(&params.gain, value);
                                setter.end_set_parameter(&params.gain);
                            }
                        }
                    }
                }

                if gain_changed.swap(false, Ordering::Relaxed) {
                    ctx.send_json(json!({
                        "type": "ParamChange",
                        "value": params.gain.value(),
                    }));
                }
            });

        Some(Box::new(editor))
    }
}

impl Vst3Plugin for Dispatch {
    const VST3_CLASS_ID: [u8; 16] = *b"WTH_Dispatch_FX_";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Utility];
}

nih_export_vst3!(Dispatch);

impl ClapPlugin for Dispatch {
    const CLAP_ID: &'static str = "wthplugins.dispatch";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Dispatch: gain with a webview GUI");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;

    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Utility,
        ClapFeature::Stereo,
    ];
}

nih_export_clap!(Dispatch);
