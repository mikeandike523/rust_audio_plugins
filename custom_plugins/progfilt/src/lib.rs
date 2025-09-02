
use nih_plug::prelude::*;
use nih_plug_webview::*;
use std::{num::NonZeroU32, sync::Arc};

#[derive(Default)]
pub struct ProgFilt;

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
        Arc::new(EmptyParams {})
    }

    fn process(
        &mut self,
        _buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _ctx: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // identity/pass-through filter: leave buffer unchanged
        ProcessStatus::Normal
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        // WebView-based GUI (separate dev server or bundled)
        let editor = WebViewEditor::new(
            HTMLSource::URL("http://localhost:5173"),
            (800, 600),
        );
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

// No parameters
#[derive(Params)]
struct EmptyParams {}
