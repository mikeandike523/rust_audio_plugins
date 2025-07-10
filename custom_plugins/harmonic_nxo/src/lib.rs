mod ADSR;
use ADSR::{Adsr};

use nih_plug::prelude::*;
use std::{sync::Arc, collections::VecDeque, num::NonZeroU32};

const MAX_VOICES: usize = 16;

#[derive(Params)]
struct SynthParams {
    #[id = "gain"]
    pub gain: FloatParam,
}

impl Default for SynthParams {
    fn default() -> Self {
        Self {
            gain: FloatParam::new(
                "Gain",
                -10.0,
                FloatRange::Linear { min: -30.0, max: 0.0 }
            )
            .with_smoother(SmoothingStyle::Linear(3.0))
            .with_step_size(0.01)
            .with_unit(" dB"),
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
    pub fn new(sr: f32) -> Self {
        Self {
            note_id: 0,
            freq: 0.0,
            phase: 0.0,
            sample_rate: sr,
            // e.g. alpha_a=0.01, alpha_d=0.05, sustain=0.7, alpha_r=0.1
            env: Adsr::new(0.01, 0.05, 0.7, 0.1),
            start_ts: 0,
        }
    }

    pub fn trigger(&mut self, note: u8, _velocity: f32, timestamp: u64) {
        // velocity can modulate env amplitude if desired
        self.note_id = note;
        self.freq = util::midi_note_to_freq(note);
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
        self.env.is_idle()
    }
}

pub struct PolySynth {
    params: Arc<SynthParams>,
    sample_rate: f32,
    voices: Vec<Voice>,
    queue: VecDeque<usize>,
    ts: u64,
}

impl Default for PolySynth {
    fn default() -> Self {
        Self {
            params: Arc::new(SynthParams::default()),
            sample_rate: 44100.0,
            voices: Vec::new(),
            queue: VecDeque::new(),
            ts: 0,
        }
    }
}

impl Plugin for PolySynth {
    const NAME: &'static str = "Harmonic NXO";
    const VENDOR: &'static str = "WTH Plugins";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        }
    ];
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
        // Collect events once per block
        let mut events = Vec::new();
        while let Some(evt) = context.next_event() {
            events.push(evt);
        }

        for (sample_id, channels) in buffer.iter_samples().enumerate() {
            self.ts = self.ts.wrapping_add(1);

            // Handle events scheduled for this sample
            for evt in events.iter().filter(|e| e.timing() as usize == sample_id) {
                match evt {
                    NoteEvent::NoteOn { note, velocity, .. } => {
                        // allocate or steal voice
                        let idx = if self.voices.len() < MAX_VOICES {
                            self.voices.push(Voice::new(self.sample_rate));
                            let i = self.voices.len() - 1;
                            self.queue.push_back(i);
                            i
                        } else {
                            // find a finished release-phase voice
                            if let Some((i, _)) = self.voices.iter().enumerate()
                                .filter(|(_,v)| v.is_released_and_done())
                                .min_by_key(|(_,v)| v.start_ts)
                            {
                                i
                            } else {
                                // steal oldest active
                                *self.queue.front().unwrap()
                            }
                        };
                        // update queue order (move to back)
                        self.queue.retain(|&i| i != idx);
                        self.queue.push_back(idx);
                        // trigger
                        self.voices[idx].trigger(*note, *velocity, self.ts);
                    }
                    NoteEvent::NoteOff { note, .. } => {
                        if let Some(i) = self.voices.iter().position(|v| v.note_id == *note) {
                            self.voices[i].release();
                        }
                    }
                    _ => {}
                }
            }

            // sum all voices
            let mut out_sample = 0.0;
            for v in &mut self.voices {
                out_sample += v.next_sample() * util::db_to_gain_fast(self.params.gain.smoothed.next());
            }

            // write same sample to all channels
            for s in channels {
                *s = out_sample;
            }
        }

        ProcessStatus::KeepAlive
    }
}

impl Vst3Plugin for PolySynth {
    const VST3_CLASS_ID: [u8; 16] = *b"WTH_Harmonic_NXO";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Instrument,
        Vst3SubCategory::Synth,
    ];
}

nih_export_vst3!(PolySynth);