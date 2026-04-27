use nih_plug::prelude::*;
use nih_plug_iced::IcedState;
use std::num::NonZeroU32;
use std::sync::Arc;

mod editor;

struct StringSim {
    params: Arc<StringSimParams>,
}

#[derive(Params)]
pub(crate) struct StringSimParams {
    #[persist = "editor-state"]
    editor_state: Arc<IcedState>,
}

impl Default for StringSim {
    fn default() -> Self {
        Self {
            params: Arc::new(StringSimParams::default()),
        }
    }
}

impl Default for StringSimParams {
    fn default() -> Self {
        Self {
            editor_state: editor::default_state(),
        }
    }
}

impl Plugin for StringSim {
    const NAME: &'static str = "String Sim";
    const VENDOR: &'static str = "WTH Plugins";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create(self.params.clone(), self.params.editor_state.clone())
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
        _buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        ProcessStatus::Normal
    }
}

impl ClapPlugin for StringSim {
    const CLAP_ID: &'static str = "wthplugins.string_sim";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("String simulation synthesizer");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Mono,
        ClapFeature::Synthesizer,
    ];
}

impl Vst3Plugin for StringSim {
    const VST3_CLASS_ID: [u8; 16] = *b"StringSimWTH____";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth];
}

nih_export_clap!(StringSim);
nih_export_vst3!(StringSim);
