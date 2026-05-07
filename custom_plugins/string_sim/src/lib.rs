use nih_plug::prelude::*;
use nih_plug_iced::IcedState;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};

mod editor;
mod string_physics;
mod tuning;

use string_physics::StringPhysics;
use tuning::{TuningFile, TuningState};

const PITCH_BEND_DEPTH: f64 = 2.0;
// Update vis state every N samples (≈5.8 ms at 44.1 kHz — fast enough for 60 fps GUI).
const VIS_UPDATE_PERIOD: u32 = 256;

fn midi_to_freq(note: u8, bend_semitones: f64) -> f64 {
    440.0 * 2.0_f64.powf((note as f64 + bend_semitones - 69.0) / 12.0)
}

fn tuned_freq(tuning: &TuningState, note: u8, bend_semitones: f64) -> f64 {
    if tuning.status.active {
        let base = tuning.frequency_for_note(note as f32) as f64;
        base * 2.0_f64.powf(bend_semitones / 12.0)
    } else {
        midi_to_freq(note, bend_semitones)
    }
}

// Shared string displacement state for the GUI canvas.
pub(crate) struct VisState {
    pub y: Vec<f32>,
    pub effective_end: usize,
}

impl Default for VisState {
    fn default() -> Self {
        Self { y: vec![0.0; 260], effective_end: 0 }
    }
}

struct StringSim {
    params:               Arc<StringSimParams>,
    physics:              Option<StringPhysics>,
    current_note:         Option<u8>,
    pitch_bend_semitones: f64,
    sample_rate:          f32,
    vis_state:            Arc<Mutex<VisState>>,
    vis_counter:          u32,
    tuning_state:         Arc<Mutex<TuningState>>,
}

#[derive(Params)]
pub(crate) struct StringSimParams {
    #[persist = "editor-state"]
    editor_state: Arc<IcedState>,

    #[id = "tension"]
    pub tension: FloatParam,

    #[id = "spring_k"]
    pub spring_k: FloatParam,

    #[id = "bending_ei"]
    pub bending_ei: FloatParam,

    #[id = "interior_damp"]
    pub interior_damp: FloatParam,

    #[id = "endpoint_damp"]
    pub endpoint_damp: FloatParam,

    #[id = "pickup_pos"]
    pub pickup_pos: FloatParam,

    #[id = "pluck_pos"]
    pub pluck_pos: FloatParam,

    #[id = "output_gain"]
    pub output_gain: FloatParam,

    #[id = "node_count"]
    pub node_count: IntParam,

    #[persist = "scl_file"]
    pub scl_file: Arc<Mutex<Option<TuningFile>>>,

    #[persist = "kbm_file"]
    pub kbm_file: Arc<Mutex<Option<TuningFile>>>,
}

impl Default for StringSim {
    fn default() -> Self {
        Self {
            params:               Arc::new(StringSimParams::default()),
            physics:              None,
            current_note:         None,
            pitch_bend_semitones: 0.0,
            sample_rate:          44100.0,
            vis_state:            Arc::new(Mutex::new(VisState::default())),
            vis_counter:          0,
            tuning_state:         Arc::new(Mutex::new(TuningState::from_files(None, None))),
        }
    }
}

impl Default for StringSimParams {
    fn default() -> Self {
        Self {
            editor_state: editor::default_state(),
            tension: FloatParam::new(
                "Tension",
                40.0,
                FloatRange::Linear { min: 5.0, max: 200.0 },
            ),
            spring_k: FloatParam::new(
                "Spring K",
                30_000.0,
                FloatRange::Skewed { min: 100.0, max: 200_000.0, factor: 2.0 },
            ),
            bending_ei: FloatParam::new(
                "Bending EI",
                3e-8,
                FloatRange::Skewed { min: 0.0, max: 1e-7, factor: 2.0 },
            ),
            interior_damp: FloatParam::new(
                "Int. Damp",
                0.25,
                FloatRange::Linear { min: 0.0, max: 3.0 },
            ),
            endpoint_damp: FloatParam::new(
                "End Damp",
                0.75,
                FloatRange::Linear { min: 0.0, max: 3.0 },
            ),
            pickup_pos: FloatParam::new(
                "Pickup Pos",
                0.15,
                FloatRange::Linear { min: 0.05, max: 0.95 },
            ),
            pluck_pos: FloatParam::new(
                "Pluck Pos",
                1.0 / 3.0,
                FloatRange::Linear { min: 0.05, max: 0.95 },
            ),
            output_gain: FloatParam::new(
                "Output Gain",
                2.5,
                FloatRange::Skewed { min: 0.01, max: 20.0, factor: 2.0 },
            ),
            node_count: IntParam::new("Nodes", 142, IntRange::Linear { min: 10, max: 260 }),
            scl_file: Arc::new(Mutex::new(None)),
            kbm_file: Arc::new(Mutex::new(None)),
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
        editor::create(
            self.params.clone(),
            self.vis_state.clone(),
            self.tuning_state.clone(),
            self.params.editor_state.clone(),
        )
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate;
        let n = self.params.node_count.value() as usize;
        self.physics = Some(StringPhysics::new_with_n(self.sample_rate, n));
        // Pre-size vis_state.y to the maximum node count so no allocation in process().
        if let Ok(mut vis) = self.vis_state.lock() {
            vis.y.resize(n.max(260), 0.0);
            vis.effective_end = n.saturating_sub(2);
        }
        // Rebuild tuning state from persisted files.
        let scl = self.params.scl_file.lock().unwrap().clone();
        let kbm = self.params.kbm_file.lock().unwrap().clone();
        *self.tuning_state.lock().unwrap() = TuningState::from_files(scl.as_ref(), kbm.as_ref());
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Rebuild physics if node_count changed (restarts simulation).
        let desired_n = self.params.node_count.value() as usize;
        if self.physics.as_ref().map_or(true, |p| p.n_total() != desired_n) {
            self.physics = Some(StringPhysics::new_with_n(self.sample_rate, desired_n));
            self.current_note = None;
        }

        let physics = match self.physics.as_mut() {
            Some(p) => p,
            None => return ProcessStatus::Normal,
        };

        // Sync all continuously-variable params every block (cheap assignments).
        physics.set_tension(self.params.tension.value() as f64);
        physics.set_spring_k(self.params.spring_k.value() as f64);
        physics.set_bending_ei(self.params.bending_ei.value() as f64);
        physics.set_interior_damp(self.params.interior_damp.value() as f64);
        physics.set_endpoint_damp(self.params.endpoint_damp.value() as f64);
        physics.set_pickup_fraction(self.params.pickup_pos.value() as f64);
        physics.set_pluck_fraction(self.params.pluck_pos.value() as f64);
        physics.set_output_gain(self.params.output_gain.value() as f64);

        // Reposition fret so desired pitch is maintained under current param set.
        physics.recompute_fret();

        // Clone tuning state once per block (cheap — editor rarely changes it).
        let tuning = self.tuning_state
            .try_lock()
            .map(|g| g.clone())
            .unwrap_or_else(|_| TuningState::from_files(None, None));

        // Drain all MIDI events before the sample loop.
        while let Some(event) = context.next_event() {
            match event {
                NoteEvent::NoteOn { note, velocity, .. } => {
                    self.current_note = Some(note);
                    let freq = tuned_freq(&tuning, note, self.pitch_bend_semitones);
                    physics.set_pitch(freq);
                    physics.pluck((velocity * 127.0) as u8);
                }
                NoteEvent::NoteOff { note, .. } => {
                    if self.current_note == Some(note) {
                        // Let ring — no explicit note-off action.
                    }
                }
                NoteEvent::MidiPitchBend { value, .. } => {
                    self.pitch_bend_semitones = (value as f64 - 0.5) * 2.0 * PITCH_BEND_DEPTH;
                    if let Some(note) = self.current_note {
                        let freq = tuned_freq(&tuning, note, self.pitch_bend_semitones);
                        physics.set_pitch(freq);
                    }
                }
                _ => {}
            }
        }

        for samples in buffer.iter_samples() {
            physics.step();
            let out = physics.output();
            for s in samples {
                *s = out;
            }

            // Periodically copy string displacement into the shared vis buffer.
            self.vis_counter += 1;
            if self.vis_counter >= VIS_UPDATE_PERIOD {
                self.vis_counter = 0;
                if let Ok(mut vis) = self.vis_state.try_lock() {
                    let eff = physics.effective_end();
                    vis.effective_end = eff;
                    let src = physics.y_slice();
                    let copy_len = (eff + 1).min(vis.y.len());
                    for i in 0..copy_len {
                        vis.y[i] = src[i] as f32;
                    }
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
