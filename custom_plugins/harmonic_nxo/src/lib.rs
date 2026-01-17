mod ADSR;
mod remote_logging;
use ADSR::{EnvelopeParams, is_finished, value_at};
// #[cfg(feature = "webview")]
use nih_plug_webview::*;
use remote_logging::RemoteLogger;

use nih_plug::prelude::*;
use ureq::json;
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
use serde_json::{self};
use tune::scala::{Kbm, Scl};
use tune::tuning::KeyboardMapping;
use tune::key::PianoKey;

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

fn velocity_to_gain(velocity: f32) -> f32 {
    let v = velocity.clamp(0.0, 1.0);
    // Use a simple curve giving about 40 dB of dynamic range
    let db = (v.powf(2.0) * 40.0) - 40.0;
    util::db_to_gain_fast_branching(db)
}

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
    #[id = "bend_range_cents"]
    pub bend_range_cents: FloatParam,
    gain_value_changed: Arc<AtomicBool>,
    #[persist = "lua_code"]
    pub lua_code: Arc<Mutex<String>>,
    #[persist = "nxo_definition"]
    pub nxo_definition: Arc<Mutex<Option<String>>>,
    #[persist = "scl_file"]
    pub scl_file: Arc<Mutex<Option<TuningFile>>>,
    #[persist = "kbm_file"]
    pub kbm_file: Arc<Mutex<Option<TuningFile>>>,
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
            bend_range_cents: FloatParam::new(
                "Pitch Bend Range",
                200.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 2400.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(10.0))
            .with_step_size(1.0)
            .with_unit(" cents"),
            gain_value_changed,
            lua_code: Arc::new(Mutex::new(
                include_str!("../web-gui/src/exampleLua/guitar.lua").to_string(),
            )),
            nxo_definition: Arc::new(Mutex::new(None)),
            scl_file: Arc::new(Mutex::new(None)),
            kbm_file: Arc::new(Mutex::new(None)),
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Action {
    Init,
    QueryCargoPackageVersion,
    QueryGain,
    QueryPitchBendRange,
    QueryLuaCode,
    QueryNxoDefinition,
    QueryTuningStatus,
    SetGainDB {
        gain: f32,
    },
    SetPitchBendRange {
        cents: f32,
    },
    SetLuaCode {
        code: String,
    },
    SetNxoDefinition {
        definition: HashMap<String, RawOscillatorParams>,
    },
    SetSclFile {
        name: String,
        contents: String,
    },
    SetKbmFile {
        name: String,
        contents: String,
    },
    ClearSclFile,
    ClearKbmFile,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct TuningFile {
    name: String,
    contents: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct TuningStatus {
    active: bool,
    scl_name: Option<String>,
    kbm_name: Option<String>,
    error: Option<String>,
}

#[derive(Clone)]
struct TuningState {
    scl: Option<Scl>,
    kbm: Option<Kbm>,
    status: TuningStatus,
}

impl TuningState {
    fn from_files(scl_file: Option<&TuningFile>, kbm_file: Option<&TuningFile>) -> Self {
        let scl_name = scl_file.map(|file| file.name.clone());
        let kbm_name = kbm_file.map(|file| file.name.clone());

        // No tuning selected
        if scl_file.is_none() && kbm_file.is_none() {
            return Self {
                scl: None,
                kbm: None,
                status: TuningStatus { active: false, scl_name, kbm_name, error: None },
            };
        }

        // Load SCL
        let scl_text = scl_file.map(|f| f.contents.as_str()).unwrap_or(DEFAULT_SCL_TEXT);
        let scl = match Scl::import(scl_text.as_bytes()) {
            Ok(s) => s,
            Err(e) => {
                return Self {
                    scl: None,
                    kbm: None,
                    status: TuningStatus {
                        active: false,
                        scl_name,
                        kbm_name,
                        error: Some(format!("Failed to import SCL: {:?}", e)),
                    },
                };
            }
        };

        // Load KBM if provided
        let kbm = if let Some(file) = kbm_file {
            match Kbm::import(file.contents.as_bytes()) {
                Ok(m) => Some(m),
                Err(e) => {
                    return Self {
                        scl: None,
                        kbm: None,
                        status: TuningStatus {
                            active: false,
                            scl_name,
                            kbm_name,
                            error: Some(format!("Failed to import KBM: {:?}", e)),
                        },
                    };
                }
            }
        } else {
            None
        };

        Self {
            scl: Some(scl),
            kbm,
            status: TuningStatus { active: true, scl_name, kbm_name, error: None },
        }
    }

    fn frequency_for_note(&self, note: f32) -> f32 {
        let floor_n = note.floor() as i32;
        let frac = note - floor_n as f32;
        let f0 = self.freq_for_degree(floor_n);
        let f1 = self.freq_for_degree(floor_n + 1);
        f0 * (1.0 - frac) + f1 * frac
    }

    // Frequency for an integer MIDI degree, taking SCL and optional KBM into account
    fn freq_for_degree(&self, degree: i32) -> f32 {
        if let Some(scl) = &self.scl {
            if let Some(kbm) = &self.kbm {
                let key = PianoKey::from_midi_number(degree);
                if let Some(pitch) = (scl, kbm).maybe_pitch_of(key) {
                    return pitch.as_hz() as f32;
                }
            }
            // Pure SCL mapping relative to equal temperament baseline
            let ratio = scl.relative_pitch_of(degree);
            return util::f32_midi_note_to_freq(degree as f32) * ratio.as_float() as f32;
        }
        util::f32_midi_note_to_freq(degree as f32)
    }
}

const DEFAULT_SCL_TEXT: &str = r#"
! 12-ET
12 tone equal temperament
12
!
100.0
200.0
300.0
400.0
500.0
600.0
700.0
800.0
900.0
1000.0
1100.0
1200.0
"#;

struct Voice {
    note_id: u8,
    phase: f32,
    sample_rate: f32,
    start_ts: u64,
    release_ts: Option<u64>,
    velocity_gain: f32,
}

impl Voice {
    pub fn new(sr: f32) -> Self {
        Self {
            note_id: 0,
            phase: 0.0,
            sample_rate: sr,
            start_ts: 0,
            release_ts: None,
            velocity_gain: 1.0,
        }
    }

    pub fn trigger(&mut self, note: u8, velocity: f32, timestamp: u64) {
        self.note_id = note;
        self.start_ts = timestamp;
        self.release_ts = None;
        self.velocity_gain = velocity_to_gain(velocity);
    }

    pub fn release(&mut self, timestamp: u64) {
        if self.release_ts.is_none() {
            self.release_ts = Some(timestamp);
        }
    }

    pub fn next_sample(
        &mut self,
        now_ts: u64,
        nxo: &NxoDefinition,
        tuning: &TuningState,
        pitch_bend: f32,
        pitch_bend_range_cents: f32,
    ) -> f32 {
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
            let amp = value_at(t, note_off, &env) * params.v * self.velocity_gain;
            val += (self.phase * mul.0 * std::f32::consts::TAU).sin() * amp;
        }

        let bend_semitones =
            util::midi_pitch_bend_to_semitones(pitch_bend, pitch_bend_range_cents / 100.0);
        let note_with_bend = self.note_id as f32 + bend_semitones;
        let freq = tuning.frequency_for_note(note_with_bend);
        let delta = freq / self.sample_rate;
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
        }) * self.velocity_gain
    }
}

pub struct HarmonicNxo {
    params: Arc<PluginParams>,
    sample_rate: f32,
    voices: Vec<Voice>,
    active_voices: HashMap<u8, usize>,
    queue: VecDeque<usize>,
    nxo_definition: Arc<Mutex<NxoDefinition>>,
    tuning_state: Arc<Mutex<TuningState>>,
    ts: u64,
    /// Current normalized pitch bend value (0.5 = no bend).
    pitch_bend: f32,
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
        let initial_tuning = {
            let scl_file = params.scl_file.lock().unwrap().clone();
            let kbm_file = params.kbm_file.lock().unwrap().clone();
            TuningState::from_files(scl_file.as_ref(), kbm_file.as_ref())
        };
        Self {
            params,
            sample_rate: 44100.0,
            voices: Vec::new(),
            active_voices: HashMap::new(),
            queue: VecDeque::new(),
            nxo_definition: Arc::new(Mutex::new(initial_nxo)),
            tuning_state: Arc::new(Mutex::new(initial_tuning)),
            ts: 0,
            midi_states: Arc::new((0..128).map(|_| AtomicBool::new(false)).collect()),
            last_midi_send: Arc::new(Mutex::new(Instant::now())),
            remote_logger: RemoteLogger::new(9099),
            pitch_bend: 0.5,
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
    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
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
        let scl_file = self.params.scl_file.lock().unwrap().clone();
        let kbm_file = self.params.kbm_file.lock().unwrap().clone();
        *self.tuning_state.lock().unwrap() = TuningState::from_files(scl_file.as_ref(), kbm_file.as_ref());
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
        let tuning_state = { self.tuning_state.lock().unwrap().clone() };
        for (sample_id, mut channels) in buffer.iter_samples().enumerate() {
            self.ts = self.ts.wrapping_add(1);
            for evt in events.iter().filter(|e| e.timing() as usize == sample_id) {
                match evt {
                    NoteEvent::MidiPitchBend { value, .. } => {
                        // Update current pitch bend value (normalized 0..1, 0.5 = no bend).
                        self.pitch_bend = *value;
                    }
                    NoteEvent::NoteOn { note, velocity, .. } => {
                        self.garbage_collect();
                        let idx = if self.voices.len() < MAX_VOICES {
                            self.voices.push(Voice::new(self.sample_rate));
                            let i = self.voices.len() - 1;
                            self.queue.push_back(i);
                            i
                        } else {
                            // When several notes arrive on the same sample we
                            // may end up stealing the voice that was just
                            // allocated for the previous note because its
                            // amplitude is still zero. To avoid this we first
                            // try to pick a voice that was not triggered on
                            // the current sample.
                            self.voices
                                .iter()
                                .enumerate()
                                .filter(|(_, v)| v.start_ts != self.ts || v.release_ts.is_some())
                                .min_by(|(_, a), (_, b)| {
                                    a.get_amplitude(self.ts, &nxo)
                                        .partial_cmp(&b.get_amplitude(self.ts, &nxo))
                                        .unwrap()
                                })
                                .map(|(i, _)| i)
                                // If all voices were started this sample then
                                // fall back to the normal amplitude based
                                // selection.
                                .unwrap_or_else(|| {
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
                                })
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
                let bend_range_cents = self.params.bend_range_cents.smoothed.next();
                let voice_sample = v.next_sample(
                    self.ts,
                    &nxo,
                    &tuning_state,
                    self.pitch_bend,
                    bend_range_cents,
                );
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

    // #[cfg(feature = "webview")]
    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let midi_states = self.midi_states.clone();
        let last_midi_send = self.last_midi_send.clone();
        let nxo_def = self.nxo_definition.clone();
        let tuning_state = self.tuning_state.clone();
        let logger = self.remote_logger.clone();
        let url = {
            // Try to connect to local dev server with a 500ms timeout
            let local_url = "http://localhost:5173";
            let production_url = "https://wth-plugins-harmonic-nxo.vercel.app";
            
            match std::thread::spawn(move || {
                use std::time::Duration;
                let client = std::sync::Arc::new(
                    ureq::AgentBuilder::new()
                        .timeout_connect(Duration::from_millis(500))
                        .timeout_read(Duration::from_millis(500))
                        .build()
                );
                
                client.get(local_url).call()
            }).join() {
                Ok(Ok(_)) => {
                    println!("Local dev server detected at {}", local_url);
                    local_url
                },
                _ => {
                    println!("Local dev server not available, using production URL: {}", production_url);
                    production_url
                }
            }
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
                            Action::SetPitchBendRange { cents } => {
                                setter.begin_set_parameter(&params.bend_range_cents);
                                setter.set_parameter(&params.bend_range_cents, cents);
                                setter.end_set_parameter(&params.bend_range_cents);
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
                            Action::SetSclFile { name, contents } => {
                                *params.scl_file.lock().unwrap() = Some(TuningFile { name, contents });
                                let scl_file = params.scl_file.lock().unwrap().clone();
                                let kbm_file = params.kbm_file.lock().unwrap().clone();
                                let new_state = TuningState::from_files(
                                    scl_file.as_ref(),
                                    kbm_file.as_ref(),
                                );
                                let status = new_state.status.clone();
                                *tuning_state.lock().unwrap() = new_state;
                                ctx.send_json(json!({
                                    "type": "RespondTuningStatus",
                                    "status": status
                                }));
                            }
                            Action::SetKbmFile { name, contents } => {
                                *params.kbm_file.lock().unwrap() = Some(TuningFile { name, contents });
                                let scl_file = params.scl_file.lock().unwrap().clone();
                                let kbm_file = params.kbm_file.lock().unwrap().clone();
                                let new_state = TuningState::from_files(
                                    scl_file.as_ref(),
                                    kbm_file.as_ref(),
                                );
                                let status = new_state.status.clone();
                                *tuning_state.lock().unwrap() = new_state;
                                ctx.send_json(json!({
                                    "type": "RespondTuningStatus",
                                    "status": status
                                }));
                            }
                            Action::ClearSclFile => {
                                *params.scl_file.lock().unwrap() = None;
                                let scl_file = params.scl_file.lock().unwrap().clone();
                                let kbm_file = params.kbm_file.lock().unwrap().clone();
                                let new_state = TuningState::from_files(
                                    scl_file.as_ref(),
                                    kbm_file.as_ref(),
                                );
                                let status = new_state.status.clone();
                                *tuning_state.lock().unwrap() = new_state;
                                ctx.send_json(json!({
                                    "type": "RespondTuningStatus",
                                    "status": status
                                }));
                            }
                            Action::ClearKbmFile => {
                                *params.kbm_file.lock().unwrap() = None;
                                let scl_file = params.scl_file.lock().unwrap().clone();
                                let kbm_file = params.kbm_file.lock().unwrap().clone();
                                let new_state = TuningState::from_files(
                                    scl_file.as_ref(),
                                    kbm_file.as_ref(),
                                );
                                let status = new_state.status.clone();
                                *tuning_state.lock().unwrap() = new_state;
                                ctx.send_json(json!({
                                    "type": "RespondTuningStatus",
                                    "status": status
                                }));
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
                            Action::QueryPitchBendRange => {
                                ctx.send_json(json!({
                                    "type": "RespondPitchBendRange",
                                    "cents": params.bend_range_cents.value()
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
                            Action::QueryTuningStatus => {
                                let status = tuning_state.lock().unwrap().status.clone();
                                ctx.send_json(json!({
                                    "type": "RespondTuningStatus",
                                    "status": status
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
