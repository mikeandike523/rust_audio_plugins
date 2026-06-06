use nih_plug::prelude::*;
use nih_plug_iced::IcedState;
use std::collections::BTreeMap;
use std::sync::Arc;

mod editor;

#[derive(Debug, Clone, Copy, PartialEq, Enum)]
pub enum StrumDirection {
    #[name = "Up"]
    Up,
    #[name = "Down"]
    Down,
}

struct Strum {
    params: Arc<StrumParams>,
    sample_rate: f32,
    /// Queued (absolute_sample, NoteOn event) pairs waiting to fire.
    pending: Vec<(u64, NoteEvent<()>)>,
    /// Monotonically increasing sample counter across process() calls.
    sample_pos: u64,
    was_playing: bool,
    rng: fastrand::Rng,
}

#[derive(Params)]
pub(crate) struct StrumParams {
    #[persist = "editor-state"]
    editor_state: Arc<IcedState>,

    #[id = "stagger_ms"]
    pub stagger_ms: FloatParam,

    #[id = "randomize_ms"]
    pub randomize_ms: FloatParam,

    /// Automatable strum direction. With SAMPLE_ACCURATE_AUTOMATION the host
    /// delivers changes at the exact sample where notes arrive, so programming
    /// down/up/down patterns in the piano roll works correctly.
    #[id = "direction"]
    pub direction: EnumParam<StrumDirection>,
}

impl Default for Strum {
    fn default() -> Self {
        Self {
            params: Arc::new(StrumParams::default()),
            sample_rate: 44100.0,
            pending: Vec::new(),
            sample_pos: 0,
            was_playing: false,
            rng: fastrand::Rng::new(),
        }
    }
}

impl Default for StrumParams {
    fn default() -> Self {
        Self {
            editor_state: editor::default_state(),
            stagger_ms: FloatParam::new(
                "Stagger",
                20.0,
                FloatRange::Linear { min: 0.0, max: 200.0 },
            )
            .with_unit(" ms"),
            randomize_ms: FloatParam::new(
                "Randomize",
                0.0,
                FloatRange::Linear { min: 0.0, max: 100.0 },
            )
            .with_unit(" ms"),
            direction: EnumParam::new("Direction", StrumDirection::Up),
        }
    }
}

impl Plugin for Strum {
    const NAME: &'static str = "Strum";
    const VENDOR: &'static str = "WTH Plugins";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[];

    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::Basic;

    /// Allows the host to deliver direction automation changes at the same
    /// sample as a note-on, enabling programmable strum patterns.
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
        self.sample_rate = buffer_config.sample_rate;
        self.pending.clear();
        self.sample_pos = 0;
        self.was_playing = false;
        true
    }

    fn reset(&mut self) {
        self.pending.clear();
        self.sample_pos = 0;
        self.was_playing = false;
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let buf_len = buffer.samples() as u64;
        let playing = context.transport().playing;

        // Stop: flush pending and silence all notes downstream.
        if self.was_playing && !playing {
            self.pending.clear();
            for channel in 0..16u8 {
                for note in 0..128u8 {
                    context.send_event(NoteEvent::NoteOff {
                        timing: 0,
                        voice_id: None,
                        channel,
                        note,
                        velocity: 0.0,
                    });
                }
            }
        }

        // Start: reset counter so pending offsets stay consistent.
        if !self.was_playing && playing {
            self.pending.clear();
            self.sample_pos = 0;
        }

        self.was_playing = playing;

        // Collect all events from the host.
        let mut all_events: Vec<NoteEvent<()>> = Vec::new();
        while let Some(e) = context.next_event() {
            all_events.push(e);
        }

        // When stopped, forward everything unchanged (audition / note preview).
        if !playing {
            for event in all_events {
                context.send_event(event);
            }
            self.sample_pos += buf_len;
            return ProcessStatus::Normal;
        }

        let direction = self.params.direction.value();
        let stagger_samples = self.params.stagger_ms.value() * self.sample_rate / 1000.0;
        let rand_half_samples = self.params.randomize_ms.value() * self.sample_rate / 1000.0 * 0.5;

        // Group NoteOn (velocity > 0) events by their timing offset.
        // All other events (NoteOff, CC, NoteOn vel=0) forward immediately.
        let mut note_on_groups: BTreeMap<u32, Vec<NoteEvent<()>>> = BTreeMap::new();
        for event in all_events {
            match &event {
                NoteEvent::NoteOn { velocity, .. } if *velocity > 0.0 => {
                    let t = event.timing();
                    note_on_groups.entry(t).or_default().push(event);
                }
                _ => context.send_event(event),
            }
        }

        // For each simultaneous chord: sort by pitch and push staggered NoteOns.
        for (timing, mut chord) in note_on_groups {
            match direction {
                StrumDirection::Up => chord.sort_by_key(note_pitch),
                StrumDirection::Down => chord.sort_by_key(|e| std::cmp::Reverse(note_pitch(e))),
            }
            for (i, event) in chord.into_iter().enumerate() {
                let jitter = self.rng.f32() * 2.0 * rand_half_samples - rand_half_samples;
                let offset = (i as f32 * stagger_samples + jitter).max(0.0).round() as u64;
                let fire_at = self.sample_pos + timing as u64 + offset;
                self.pending.push((fire_at, event));
            }
        }

        // Pending is small (≤ a few chords); sort to keep drain pass simple.
        self.pending.sort_unstable_by_key(|(t, _)| *t);

        // Emit all pending events whose fire time falls within this buffer.
        let drain_end = self.sample_pos + buf_len;
        let mut i = 0;
        while i < self.pending.len() {
            let (fire_at, _) = self.pending[i];
            if fire_at < drain_end {
                let (fire_at, event) = self.pending.remove(i);
                let rel = fire_at.saturating_sub(self.sample_pos).min(buf_len - 1) as u32;
                context.send_event(with_timing(event, rel));
            } else {
                i += 1;
            }
        }

        self.sample_pos += buf_len;
        ProcessStatus::Normal
    }
}

fn note_pitch(e: &NoteEvent<()>) -> u8 {
    match e {
        NoteEvent::NoteOn { note, .. } => *note,
        _ => 0,
    }
}

fn with_timing(e: NoteEvent<()>, timing: u32) -> NoteEvent<()> {
    match e {
        NoteEvent::NoteOn { voice_id, channel, note, velocity, .. } => {
            NoteEvent::NoteOn { timing, voice_id, channel, note, velocity }
        }
        other => other,
    }
}

impl ClapPlugin for Strum {
    const CLAP_ID: &'static str = "wthplugins.strum";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("MIDI strum effect");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] =
        &[ClapFeature::NoteEffect, ClapFeature::Utility];
}

impl Vst3Plugin for Strum {
    const VST3_CLASS_ID: [u8; 16] = *b"StrumPlugin_WTH_";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Tools];
}

nih_export_clap!(Strum);
nih_export_vst3!(Strum);
