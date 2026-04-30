use nih_plug::prelude::*;
use nih_plug_iced::IcedState;
use std::num::NonZeroU32;
use std::sync::Arc;

mod editor;
mod string_physics;

use string_physics::StringPhysics;

const PITCH_BEND_DEPTH: f64 = 2.0;

fn midi_to_freq(note: u8, bend_semitones: f64) -> f64 {
    440.0 * 2.0_f64.powf((note as f64 + bend_semitones - 69.0) / 12.0)
}

struct StringSim {
    params:               Arc<StringSimParams>,
    physics:              Option<StringPhysics>,
    current_note:         Option<u8>,
    pitch_bend_semitones: f64,
}

#[derive(Params)]
pub(crate) struct StringSimParams {
    #[persist = "editor-state"]
    editor_state: Arc<IcedState>,
}

impl Default for StringSim {
    fn default() -> Self {
        Self {
            params:               Arc::new(StringSimParams::default()),
            physics:              None,
            current_note:         None,
            pitch_bend_semitones: 0.0,
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

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
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
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.physics = Some(StringPhysics::new(buffer_config.sample_rate));
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Drain all MIDI events before the sample loop.
        while let Some(event) = context.next_event() {
            match event {
                NoteEvent::NoteOn { note, velocity, .. } => {
                    self.current_note = Some(note);
                    let freq = midi_to_freq(note, self.pitch_bend_semitones);
                    if let Some(physics) = &mut self.physics {
                        physics.set_pitch(freq);
                        physics.pluck((velocity * 127.0) as u8);
                    }
                }
                NoteEvent::NoteOff { note, .. } => {
                    if self.current_note == Some(note) {
                        // Let ring — no explicit note-off action.
                    }
                }
                NoteEvent::MidiPitchBend { value, .. } => {
                    self.pitch_bend_semitones = (value as f64 - 0.5) * 2.0 * PITCH_BEND_DEPTH;
                    if let Some(note) = self.current_note {
                        let freq = midi_to_freq(note, self.pitch_bend_semitones);
                        if let Some(physics) = &mut self.physics {
                            physics.set_pitch(freq);
                        }
                    }
                }
                _ => {}
            }
        }

        if let Some(physics) = &mut self.physics {
            for samples in buffer.iter_samples() {
                physics.step();
                let out = physics.output();
                for s in samples {
                    *s = out;
                }
            }
        }

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
