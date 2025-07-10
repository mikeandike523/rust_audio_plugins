mod ADSR;
use ADSR::{Adsr, CurveType};

use nih_plug::prelude::*;
use std::{collections::{HashMap, VecDeque}, sync::Arc, num::NonZeroU32};

const MAX_VOICES: usize = 16;

#[derive(Params)]
struct SynthParams {
    #[id = "gain"]
    pub gain: FloatParam,

    #[id = "attack"]
    pub attack: FloatParam,
    #[id = "decay"]
    pub decay: FloatParam,
    #[id = "sustain"]
    pub sustain: FloatParam,
    #[id = "release"]
    pub release: FloatParam,
}

impl Default for SynthParams {
    fn default() -> Self {
        Self {
            gain: FloatParam::new("Gain", -10.0, FloatRange::Linear { min: -30.0, max: 0.0 })
                .with_smoother(SmoothingStyle::Linear(3.0))
                .with_step_size(0.01)
                .with_unit(" dB"),

            attack: FloatParam::new("Attack", 0.01, FloatRange::Skewed { min: 0.001, max: 2.0, factor: 0.5 }),
            decay: FloatParam::new("Decay", 0.05, FloatRange::Skewed { min: 0.001, max: 2.0, factor: 0.5 }),
            sustain: FloatParam::new("Sustain", 0.7, FloatRange::Linear { min: 0.0, max: 1.0 }),
            release: FloatParam::new("Release", 0.1, FloatRange::Skewed { min: 0.001, max: 2.0, factor: 0.5 }),
        }
    }
}

struct Voice {
    note_id: u8,
    freq: f32,
    phase: f32,
    sample_rate: f32,
    env: Adsr,
    start_ts: u64,
}

impl Voice {
    pub fn new(sr: f32, params: &SynthParams) -> Self {
        Self {
            note_id: 0,
            freq: 0.0,
            phase: 0.0,
            sample_rate: sr,
            env: Adsr::new(
                params.attack.value(),
                params.decay.value(),
                params.sustain.value(),
                params.release.value(),
                sr,
                CurveType::Exponential,
            ),
            start_ts: 0,
        }
    }

    pub fn trigger(&mut self, note: u8, _velocity: f32, timestamp: u64, params: &SynthParams) {
        self.note_id = note;
        self.freq = util::midi_note_to_freq(note);
        self.env.set_attack_time(params.attack.value());
        self.env.set_decay_time(params.decay.value());
        self.env.set_sustain_level(params.sustain.value());
        self.env.set_release_time(params.release.value());
        self.env.trigger();
        self.start_ts = timestamp;
    }

    pub fn release(&mut self) {
        self.env.release();
    }

    pub fn next_sample(&mut self) -> f32 {
        let amp = self.env.next();
        let delta = self.freq / self.sample_rate;
        let val = (self.phase * std::f32::consts::TAU).sin() * amp;
        self.phase = (self.phase + delta) % 1.0;
        val
    }

    pub fn is_released_and_done(&self) -> bool {
        self.env.is_finished()
    }

    pub fn get_amplitude(&self) -> f32 {
        self.env.get_level()
    }
}

pub struct HarmonicNxo {
    params: Arc<SynthParams>,
    sample_rate: f32,
    voices: Vec<Voice>,
    active_voices: HashMap<u8, usize>,
    queue: VecDeque<usize>,
    ts: u64,
}

impl Default for HarmonicNxo {
    fn default() -> Self {
        Self {
            params: Arc::new(SynthParams::default()),
            sample_rate: 44100.0,
            voices: Vec::new(),
            active_voices: HashMap::new(),
            queue: VecDeque::new(),
            ts: 0,
        }
    }
}

impl Plugin for HarmonicNxo {
    const NAME: &'static str = "Harmonic NXO";
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

    fn initialize(
        &mut self,
        _layout: &AudioIOLayout,
        config: &BufferConfig,
        _context: &mut impl InitContext<Self>
    ) -> bool {
        self.sample_rate = config.sample_rate;
        true
    }
fn process(
    &mut self,
    buffer: &mut Buffer,
    _aux: &mut AuxiliaryBuffers,
    context: &mut impl ProcessContext<Self>
) -> ProcessStatus {
    let mut events = Vec::new();
    while let Some(evt) = context.next_event() {
        events.push(evt);
    }

    for (sample_id, mut channels) in buffer.iter_samples().enumerate() {
        self.ts = self.ts.wrapping_add(1);

        for evt in events.iter().filter(|e| e.timing() as usize == sample_id) {
            match evt {
                NoteEvent::NoteOn { note, velocity, .. } => {
                    self.note_garbage_collection();
                    let idx = if self.voices.len() < MAX_VOICES {
                        self.voices.push(Voice::new(self.sample_rate, &self.params));
                        let i = self.voices.len() - 1;
                        self.queue.push_back(i);
                        i
                    } else {
                        self.voices.iter().enumerate()
                            .min_by(|(_, a), (_, b)| a.get_amplitude().partial_cmp(&b.get_amplitude()).unwrap())
                            .map(|(i, _)| i)
                            .unwrap_or(0)
                    };
                    self.queue.retain(|&i| i != idx);
                    self.queue.push_back(idx);
                    self.voices[idx].trigger(*note, *velocity, self.ts, &self.params);
                    self.active_voices.insert(*note, idx);
                }
                NoteEvent::NoteOff { note, .. } => {
                    if let Some(&i) = self.active_voices.get(note) {
                        self.voices[i].release();
                    }
                }
                _ => {}
            }
        }

        let mut out_sample = 0.0;
        for v in &mut self.voices {
            let voice_sample = v.next_sample();
            if voice_sample != 0.0 {
                out_sample += voice_sample * util::db_to_gain_fast(self.params.gain.smoothed.next());
            }
        }

        for s in channels.iter_mut().take(2) {
            *s = out_sample;
        }
    }

    ProcessStatus::KeepAlive
}

}

impl HarmonicNxo {
    fn note_garbage_collection(&mut self) {
        self.active_voices.retain(|_, &mut i| !self.voices[i].is_released_and_done());
    }
}

impl Vst3Plugin for HarmonicNxo {
    const VST3_CLASS_ID: [u8; 16] = *b"WTH_Harmonic_NXO";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Instrument,
        Vst3SubCategory::Synth,
    ];
}

nih_export_vst3!(HarmonicNxo);
