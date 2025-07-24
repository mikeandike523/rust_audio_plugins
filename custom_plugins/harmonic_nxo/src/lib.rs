mod ADSR;
use ADSR::{Adsr, CurveType};
use nih_plug_webview::*;

use nih_plug::prelude::*;
use std::{
    any::Any,
    collections::{HashMap, VecDeque},
    num::NonZeroU32,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use serde::Deserialize;
use serde_json::json;

use std::sync::atomic::{AtomicBool, Ordering};

const MAX_VOICES: usize = 16;

// Default ADSR constants (handled in WebView)
const DEFAULT_ATTACK: f32 = 0.01;
const DEFAULT_DECAY: f32 = 0.05;
const DEFAULT_SUSTAIN: f32 = 0.7;
const DEFAULT_RELEASE: f32 = 0.1;

/// Plugin parameters: only Gain param here
#[derive(Params)]
struct PluginParams {
    #[id = "gain"]
    pub gain: FloatParam,
    gain_value_changed: Arc<AtomicBool>,
}

impl Default for PluginParams {
    fn default() -> Self {        let gain_value_changed = Arc::new(AtomicBool::new(false));

        let v = gain_value_changed.clone();
        let param_callback = Arc::new(move |_: f32| {
            v.store(true, Ordering::Relaxed);
        });


        PluginParams {
            gain: FloatParam::new(
                "Gain",
                -9.0,
                FloatRange::Linear {
                    min: -30.0,
                    max: 0.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(3.0))
            .with_step_size(0.01)
            .with_unit(" dB")
            .with_callback(param_callback.clone())
            ,
             gain_value_changed
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Action {
    Init,
    QueryCargoPackageVersion,
    QueryGain,
    SetGainDB { gain: f32 },
    
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
        let mut env = Adsr::new(
            DEFAULT_ATTACK,
            DEFAULT_DECAY,
            DEFAULT_SUSTAIN,
            DEFAULT_RELEASE,
            sr,
            CurveType::Exponential,
        );
        env.set_attack_time(DEFAULT_ATTACK);
        env.set_decay_time(DEFAULT_DECAY);
        env.set_sustain_level(DEFAULT_SUSTAIN);
        env.set_release_time(DEFAULT_RELEASE);
        Self {
            note_id: 0,
            freq: 0.0,
            phase: 0.0,
            sample_rate: sr,
            env,
            start_ts: 0,
        }
    }

    pub fn trigger(&mut self, note: u8, _velocity: f32, timestamp: u64) {
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
        self.env.is_finished()
    }
    pub fn get_amplitude(&self) -> f32 {
        self.env.get_level()
    }
}

pub struct HarmonicNxo {
    params: Arc<PluginParams>,
    sample_rate: f32,
    voices: Vec<Voice>,
    active_voices: HashMap<u8, usize>,
    queue: VecDeque<usize>,
    ts: u64,
    midi_states: Arc<Vec<AtomicBool>>,
    last_midi_send: Arc<Mutex<Instant>>,
}

impl Default for HarmonicNxo {
    fn default() -> Self {
        Self {
            params: Arc::new(PluginParams::default()),
            sample_rate: 44100.0,
            voices: Vec::new(),
            active_voices: HashMap::new(),
            queue: VecDeque::new(),
            ts: 0,
            midi_states: Arc::new((0..128).map(|_| AtomicBool::new(false)).collect()),
            last_midi_send: Arc::new(Mutex::new(Instant::now())),
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
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = config.sample_rate;
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
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
                        self.garbage_collect();
                        let idx = if self.voices.len() < MAX_VOICES {
                            self.voices.push(Voice::new(self.sample_rate));
                            let i = self.voices.len() - 1;
                            self.queue.push_back(i);
                            i
                        } else {
                            self.voices
                                .iter()
                                .enumerate()
                                .min_by(|(_, a), (_, b)| {
                                    a.get_amplitude().partial_cmp(&b.get_amplitude()).unwrap()
                                })
                                .map(|(i, _)| i)
                                .unwrap_or(0)
                        };
                        self.queue.retain(|&i| i != idx);
                        self.queue.push_back(idx);
                        self.voices[idx].trigger(*note, *velocity, self.ts);
                        self.active_voices.insert(*note, idx);
                        if let Some(state) = self.midi_states.get(*note as usize) {
                            state.store(true, Ordering::Relaxed);
                        }
                    }
                    NoteEvent::NoteOff { note, .. } => {
                        if let Some(&i) = self.active_voices.get(note) {
                            self.voices[i].release();
                        }
                        if let Some(state) = self.midi_states.get(*note as usize) {
                            state.store(false, Ordering::Relaxed);
                        }
                    }
                    _ => {}
                }
            }
            let mut out_sample = 0.0;
            for v in &mut self.voices {
                let voice_sample = v.next_sample();
                if voice_sample != 0.0 {
                    let gain = util::db_to_gain_fast(self.params.gain.smoothed.next());
                    out_sample += voice_sample * gain;
                }
            }
            for s in channels.iter_mut().take(2) {
                *s = out_sample;
            }
        }
        ProcessStatus::KeepAlive
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let midi_states = self.midi_states.clone();
        let last_midi_send = self.last_midi_send.clone();
        let editor = WebViewEditor::new(HTMLSource::URL("http://localhost:5173"), (1000, 750))
            .with_developer_mode(true)
            .with_keyboard_handler(move |event| {
                println!("keyboard event: {event:#?}");
                event.key == Key::Escape
            })
            .with_event_loop(move |ctx, setter, window| {
                while let Ok(value) = ctx.next_event() {
                    if let Ok(action) = serde_json::from_value(value) {
                        match action {
                            Action::SetGainDB { gain } => {
                                setter.begin_set_parameter(&params.gain);
                                setter.set_parameter(&params.gain, gain);
                                setter.end_set_parameter(&params.gain);
                            }

                            Action::Init => {
                                // no-op
                            }
                            Action::QueryCargoPackageVersion => {
                                ctx.send_json(json!({
                                    "type": "RespondCargoPackageVersion",
                                    "version": env!("CARGO_PKG_VERSION")
                                }));
                            }
                            Action::QueryGain => {
                                ctx.send_json(json!({
                                    "type": "RespondGain",
                                    "gain": params.gain.value()
                                }));
                            }

                        }
                    } else {
                        panic!("Invalid action received from web UI.")
                    }
                }

                {
                    let mut last = last_midi_send.lock().unwrap();
                    if last.elapsed() >= Duration::from_millis(100) {
                        let states: Vec<bool> = midi_states
                            .iter()
                            .map(|s| s.load(Ordering::Relaxed))
                            .collect();
                        ctx.send_json(json!({
                            "type": "MidiStateUpdate",
                            "states": states
                        }));
                        *last = Instant::now();
                    }
                }
            });
        Some(Box::new(editor))
    }
}

impl HarmonicNxo {
    fn garbage_collect(&mut self) {
        self.active_voices
            .retain(|_, &mut i| !self.voices[i].is_released_and_done());
    }
}

impl Vst3Plugin for HarmonicNxo {
    const VST3_CLASS_ID: [u8; 16] = *b"WTH_Harmonic_NXO";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth];
}

nih_export_vst3!(HarmonicNxo);


impl ClapPlugin for HarmonicNxo {
    // Reverse‑DNS style, all lowercase, no spaces
    const CLAP_ID: &'static str = "wthplugins.harmonic_nxo";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Harmonic NXO synth");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;

    // Pick what applies
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Stereo,
    ];
}

nih_export_clap!(HarmonicNxo);