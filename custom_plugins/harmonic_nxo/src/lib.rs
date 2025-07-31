mod ADSR;
mod remote_logging;
use ADSR::{EnvelopeParams, is_finished, value_at};
use nih_plug_webview::*;
use remote_logging::RemoteLogger;

use nih_plug::prelude::*;
use std::{
    any::Any,
    collections::{HashMap, VecDeque},
    num::NonZeroU32,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use ordered_float::OrderedFloat;
use serde::{Deserialize, Serialize};
use serde_json::{self, json};

use std::sync::atomic::{AtomicBool, Ordering};

const MAX_VOICES: usize = 16;

// Default ADSR constants (handled in WebView)
const DEFAULT_ATTACK: f32 = 0.01;
const DEFAULT_DECAY: f32 = 0.05;
const DEFAULT_SUSTAIN: f32 = 0.7;
const DEFAULT_RELEASE: f32 = 0.1;

// Default per-oscillator ADSR constants for NXO entries
const DEFAULT_V: f32 = 1.0;
const DEFAULT_A: f32 = 0.005;
const DEFAULT_D: f32 = 0.005;
const DEFAULT_S: f32 = 1.0;
const DEFAULT_R: f32 = 0.005;

/// Compile the bundled example Lua file into an NXO definition.
///
/// This replicates the Lua script's algorithm in Rust so that the example can
/// be compiled on first launch without relying on an embedded Lua interpreter.
fn compile_example_nxo_json() -> String {
    let base_amp_db = -6.0f32;
    let amp_step_db = -3.0f32;
    let base_sus_db = -12.0f32;
    let sus_step_db = -2.0f32;
    let attack_seconds = 0.005f32;
    let decay_seconds = 0.1f32;
    let base_rel_sec = 0.5f32;
    let rel_step_sec = -0.05f32;

    let mut map = HashMap::new();
    for i in 1..=6 {
        let n = i as f32;
        let peak_db = base_amp_db + (n - 1.0) * amp_step_db;
        let sus_db = base_sus_db + (n - 1.0) * sus_step_db;
        let rel = (base_rel_sec + (n - 1.0) * rel_step_sec).max(0.0);

        map.insert(
            i.to_string(),
            RawOscillatorParams {
                v: 10f32.powf(peak_db / 20.0),
                a: attack_seconds,
                d: decay_seconds,
                s: 10f32.powf(sus_db / 20.0),
                r: rel,
            },
        );
    }

    serde_json::to_string(&map).unwrap()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct RawOscillatorParams {
    v: f32,
    a: f32,
    d: f32,
    s: f32,
    r: f32,
}

#[derive(Debug, Clone)]
struct OscillatorParams {
    v: f32,
    a: f32,
    d: f32,
    s: f32,
    r: f32,
}

type RawNxoDefinition = HashMap<String, RawOscillatorParams>;

#[derive(Debug, Clone)]
struct NxoDefinition(HashMap<OrderedFloat<f32>, OscillatorParams>);

impl Default for NxoDefinition {
    fn default() -> Self {
        let mut map = HashMap::new();
        map.insert(
            OrderedFloat(1.0),
            OscillatorParams {
                v: DEFAULT_V,
                a: DEFAULT_A,
                d: DEFAULT_D,
                s: DEFAULT_S,
                r: DEFAULT_R,
            },
        );
        NxoDefinition(map)
    }
}

impl TryFrom<RawNxoDefinition> for NxoDefinition {
    type Error = &'static str;

    fn try_from(raw: RawNxoDefinition) -> Result<Self, Self::Error> {
        if raw.is_empty() {
            return Err("NXO definition cannot be empty");
        }

        let mut out = HashMap::with_capacity(raw.len());
        for (k, v) in raw {
            let key: f32 = k.parse().map_err(|_| "Invalid frequency multiplier")?;
            if !key.is_finite() {
                return Err("Invalid frequency multiplier");
            }
            out.insert(
                OrderedFloat(key),
                OscillatorParams {
                    v: v.v,
                    a: v.a,
                    d: v.d,
                    s: v.s,
                    r: v.r,
                },
            );
        }
        Ok(NxoDefinition(out))
    }
}

/// Plugin parameters: only Gain param here
#[derive(Params)]
struct PluginParams {
    #[id = "gain"]
    pub gain: FloatParam,
    gain_value_changed: Arc<AtomicBool>,
    #[persist = "lua_code"]
    pub lua_code: Arc<Mutex<String>>,
    #[persist = "nxo_definition"]
    pub nxo_definition: Arc<Mutex<Option<String>>>,
}

impl Default for PluginParams {
    fn default() -> Self {
        let gain_value_changed = Arc::new(AtomicBool::new(false));

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
            .with_callback(param_callback.clone()),
            gain_value_changed,
            lua_code: Arc::new(Mutex::new(
                include_str!("../web-gui/src/exampleLua/guitar.lua").to_string(),
            )),
            nxo_definition: Arc::new(Mutex::new(None)),
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Action {
    Init,
    QueryCargoPackageVersion,
    QueryGain,
    QueryLuaCode,
    QueryNxoDefinition,
    SetGainDB {
        gain: f32,
    },
    SetLuaCode {
        code: String,
    },
    SetNxoDefinition {
        definition: HashMap<String, RawOscillatorParams>,
    },
}

struct Voice {
    note_id: u8,
    freq: f32,
    phase: f32,
    sample_rate: f32,
    start_ts: u64,
    release_ts: Option<u64>,
}

impl Voice {
    pub fn new(sr: f32) -> Self {
        Self {
            note_id: 0,
            freq: 0.0,
            phase: 0.0,
            sample_rate: sr,
            start_ts: 0,
            release_ts: None,
        }
    }

    pub fn trigger(&mut self, note: u8, _velocity: f32, timestamp: u64) {
        self.note_id = note;
        self.freq = util::midi_note_to_freq(note);
        self.start_ts = timestamp;
        self.release_ts = None;
    }

    pub fn release(&mut self, timestamp: u64) {
        if self.release_ts.is_none() {
            self.release_ts = Some(timestamp);
        }
    }

    pub fn next_sample(&mut self, now_ts: u64, nxo: &NxoDefinition) -> f32 {
        let t = (now_ts - self.start_ts) as f32 / self.sample_rate;
        let note_off = self
            .release_ts
            .map(|off| (off - self.start_ts) as f32 / self.sample_rate);

        let mut val = 0.0;
        for (mul, params) in &nxo.0 {
            let env = EnvelopeParams {
                attack: params.a,
                decay: params.d,
                sustain: params.s,
                release: params.r,
            };
            let amp = value_at(t, note_off, &env) * params.v;
            val += (self.phase * mul.0 * std::f32::consts::TAU).sin() * amp;
        }

        let delta = self.freq / self.sample_rate;
        self.phase = (self.phase + delta) % 1.0;
        val
    }

    pub fn is_released_and_done(&self, now_ts: u64, nxo: &NxoDefinition) -> bool {
        let t = (now_ts - self.start_ts) as f32 / self.sample_rate;
        let note_off = self
            .release_ts
            .map(|off| (off - self.start_ts) as f32 / self.sample_rate);

        nxo.0.values().all(|params| {
            let env = EnvelopeParams {
                attack: params.a,
                decay: params.d,
                sustain: params.s,
                release: params.r,
            };
            is_finished(t, note_off, &env)
        })
    }
    pub fn get_amplitude(&self, now_ts: u64, nxo: &NxoDefinition) -> f32 {
        let t = (now_ts - self.start_ts) as f32 / self.sample_rate;
        let note_off = self
            .release_ts
            .map(|off| (off - self.start_ts) as f32 / self.sample_rate);

        nxo.0.values().fold(0.0, |acc, params| {
            let env = EnvelopeParams {
                attack: params.a,
                decay: params.d,
                sustain: params.s,
                release: params.r,
            };
            acc + value_at(t, note_off, &env) * params.v
        })
    }
}

pub struct HarmonicNxo {
    params: Arc<PluginParams>,
    sample_rate: f32,
    voices: Vec<Voice>,
    active_voices: HashMap<u8, usize>,
    queue: VecDeque<usize>,
    nxo_definition: Arc<Mutex<NxoDefinition>>,
    ts: u64,
    midi_states: Arc<Vec<AtomicBool>>,
    last_midi_send: Arc<Mutex<Instant>>,
    remote_logger: RemoteLogger,
}

impl Default for HarmonicNxo {
    fn default() -> Self {
        let params = Arc::new(PluginParams::default());
        let initial_nxo = {
            let mut guard = params.nxo_definition.lock().unwrap();
            if let Some(json) = guard.as_ref() {
                if let Ok(def) = serde_json::from_str::<HashMap<String, RawOscillatorParams>>(json)
                {
                    NxoDefinition::try_from(def).unwrap_or_default()
                } else {
                    NxoDefinition::default()
                }
            } else {
                let compiled = compile_example_nxo_json();
                *guard = Some(compiled.clone());
                let def = serde_json::from_str::<HashMap<String, RawOscillatorParams>>(&compiled).unwrap();
                NxoDefinition::try_from(def).unwrap_or_default()
            }
        };
        Self {
            params,
            sample_rate: 44100.0,
            voices: Vec::new(),
            active_voices: HashMap::new(),
            queue: VecDeque::new(),
            nxo_definition: Arc::new(Mutex::new(initial_nxo)),
            ts: 0,
            midi_states: Arc::new((0..128).map(|_| AtomicBool::new(false)).collect()),
            last_midi_send: Arc::new(Mutex::new(Instant::now())),
            remote_logger: RemoteLogger::new(9099),
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
        for voice in &mut self.voices {
            voice.sample_rate = self.sample_rate;
        }
        let mut guard = self.params.nxo_definition.lock().unwrap();
        if let Some(json) = guard.as_ref() {
            if let Ok(def) = serde_json::from_str::<HashMap<String, RawOscillatorParams>>(json) {
                if let Ok(nxo) = NxoDefinition::try_from(def) {
                    *self.nxo_definition.lock().unwrap() = nxo;
                }
            }
        } else {
            let compiled = compile_example_nxo_json();
            *guard = Some(compiled.clone());
            if let Ok(def) = serde_json::from_str::<HashMap<String, RawOscillatorParams>>(&compiled)
            {
                if let Ok(nxo) = NxoDefinition::try_from(def) {
                    *self.nxo_definition.lock().unwrap() = nxo;
                }
            }
        }
        true
    }

    fn reset(&mut self) {
        for voice in &mut self.voices {
            voice.sample_rate = self.sample_rate;
        }
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
        let nxo = { self.nxo_definition.lock().unwrap().clone() };
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
                                    a.get_amplitude(self.ts, &nxo)
                                        .partial_cmp(&b.get_amplitude(self.ts, &nxo))
                                        .unwrap()
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
                            self.voices[i].release(self.ts);
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
                let voice_sample = v.next_sample(self.ts, &nxo);
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
        let nxo_def = self.nxo_definition.clone();
        let logger = self.remote_logger.clone();
        let url = if cfg!(debug_assertions) {
            "http://localhost:3000"
        } else {
            "https://wth-plugins-harmonic-nxo.vercel.app"
        };

        let editor = WebViewEditor::new(HTMLSource::URL(url), (1000, 750))
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

                            Action::SetLuaCode { code } => {
                                *params.lua_code.lock().unwrap() = code;
                            }

                            Action::SetNxoDefinition { definition } => {
                                if let Ok(nxo) = NxoDefinition::try_from(definition.clone()) {
                                    *nxo_def.lock().unwrap() = nxo;
                                    *params.nxo_definition.lock().unwrap() =
                                        Some(serde_json::to_string(&definition).unwrap());
                                    logger.log(&json!({
                                        "event": "SetNxoDefinition",
                                        "definition": definition
                                    }));
                                }
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
                            Action::QueryLuaCode => {
                                let code = params.lua_code.lock().unwrap().clone();
                                ctx.send_json(json!({
                                    "type": "RespondLuaCode",
                                    "code": code
                                }));
                            }
                            Action::QueryNxoDefinition => {
                                if let Some(json) =
                                    params.nxo_definition.lock().unwrap().as_ref().cloned()
                                {
                                    if let Ok(def) = serde_json::from_str::<
                                        HashMap<String, RawOscillatorParams>,
                                    >(&json)
                                    {
                                        ctx.send_json(json!({
                                            "type": "RespondNxoDefinition",
                                            "definition": def
                                        }));
                                    }
                                }
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
        let ts = self.ts;
        let nxo = { self.nxo_definition.lock().unwrap().clone() };
        self.active_voices
            .retain(|_, &mut i| !self.voices[i].is_released_and_done(ts, &nxo));
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
