use nih_plug::prelude::*;
use nih_plug_webview::*;
use serde::Deserialize;
use serde_json::json;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const GUI_WIDTH: u32 = 520;
const GUI_HEIGHT: u32 = 300;
const GUI_HTML: &str = include_str!("../web-gui/embedded.html");
const GUI_DEV_SERVER_URL: &str = "http://localhost:5173";
const METER_UPDATE_SECONDS: f32 = 0.1;

#[derive(Params)]
struct BasicPluginExampleParams {
    #[id = "saturation"]
    saturation: FloatParam,
    #[id = "gain"]
    gain: FloatParam,
    saturation_changed: Arc<AtomicBool>,
    gain_changed: Arc<AtomicBool>,
}

impl Default for BasicPluginExampleParams {
    fn default() -> Self {
        let saturation_changed = Arc::new(AtomicBool::new(false));
        let gain_changed = Arc::new(AtomicBool::new(false));

        let sat_changed = saturation_changed.clone();
        let gain_changed_cb = gain_changed.clone();

        let saturation_callback = Arc::new(move |_: f32| {
            sat_changed.store(true, Ordering::Relaxed);
        });

        let gain_callback = Arc::new(move |_: f32| {
            gain_changed_cb.store(true, Ordering::Relaxed);
        });

        Self {
            saturation: FloatParam::new(
                "Saturation",
                1.0,
                FloatRange::Linear { min: 0.0, max: 10.0 },
            )
            .with_smoother(SmoothingStyle::Linear(5.0))
            .with_step_size(0.01)
            .with_callback(saturation_callback),
            gain: FloatParam::new(
                "Gain",
                0.0,
                FloatRange::Linear { min: -24.0, max: 24.0 },
            )
            .with_smoother(SmoothingStyle::Linear(5.0))
            .with_step_size(0.1)
            .with_unit(" dB")
            .with_callback(gain_callback),
            saturation_changed,
            gain_changed,
        }
    }
}

pub struct BasicPluginExample {
    params: Arc<BasicPluginExampleParams>,
    sample_rate: f32,
    meter_interval_samples: usize,
    meter_samples_remaining: usize,
    meter_input_peak_l: f32,
    meter_input_peak_r: f32,
    meter_output_peak_l: f32,
    meter_output_peak_r: f32,
    meter_input_l: Arc<AtomicF32>,
    meter_input_r: Arc<AtomicF32>,
    meter_output_l: Arc<AtomicF32>,
    meter_output_r: Arc<AtomicF32>,
    meter_dirty: Arc<AtomicBool>,
}

impl Default for BasicPluginExample {
    fn default() -> Self {
        let meter_input_l = Arc::new(AtomicF32::new(0.0));
        let meter_input_r = Arc::new(AtomicF32::new(0.0));
        let meter_output_l = Arc::new(AtomicF32::new(0.0));
        let meter_output_r = Arc::new(AtomicF32::new(0.0));
        let meter_dirty = Arc::new(AtomicBool::new(false));

        Self {
            params: Arc::new(BasicPluginExampleParams::default()),
            sample_rate: 44100.0,
            meter_interval_samples: 4410,
            meter_samples_remaining: 4410,
            meter_input_peak_l: 0.0,
            meter_input_peak_r: 0.0,
            meter_output_peak_l: 0.0,
            meter_output_peak_r: 0.0,
            meter_input_l,
            meter_input_r,
            meter_output_l,
            meter_output_r,
            meter_dirty,
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Action {
    Init,
    SetSaturation { value: f32 },
    SetGain { value: f32 },
}

impl BasicPluginExample {
    fn update_meter_interval(&mut self, sample_rate: f32) {
        if (self.sample_rate - sample_rate).abs() < f32::EPSILON {
            return;
        }

        self.sample_rate = sample_rate;
        let interval = (self.sample_rate * METER_UPDATE_SECONDS).round() as usize;
        self.meter_interval_samples = interval.max(1);
        self.meter_samples_remaining = self.meter_interval_samples;
        self.reset_meter_peaks();
    }

    fn reset_meter_peaks(&mut self) {
        self.meter_input_peak_l = 0.0;
        self.meter_input_peak_r = 0.0;
        self.meter_output_peak_l = 0.0;
        self.meter_output_peak_r = 0.0;
    }

    fn publish_meter_values(&mut self) {
        self.meter_input_l
            .store(self.meter_input_peak_l, Ordering::Relaxed);
        self.meter_input_r
            .store(self.meter_input_peak_r, Ordering::Relaxed);
        self.meter_output_l
            .store(self.meter_output_peak_l, Ordering::Relaxed);
        self.meter_output_r
            .store(self.meter_output_peak_r, Ordering::Relaxed);
        self.meter_dirty.store(true, Ordering::Relaxed);
        self.reset_meter_peaks();
    }
}

impl Plugin for BasicPluginExample {
    const NAME: &'static str = "Basic Plugin Example";
    const VENDOR: &'static str = "WTH Plugins";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.update_meter_interval(buffer_config.sample_rate);
        true
    }

    fn reset(&mut self) {
        self.reset_meter_peaks();
        self.meter_samples_remaining = self.meter_interval_samples;
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.update_meter_interval(context.transport().sample_rate);

        let mut samples_remaining = self.meter_samples_remaining;

        for (_sample_id, mut channels) in buffer.iter_samples().enumerate() {
            let saturation = self.params.saturation.smoothed.next();
            let gain = util::db_to_gain_fast(self.params.gain.smoothed.next());

            let in_l = channels.get_mut(0).map(|value| *value).unwrap_or(0.0);
            let in_r = channels.get_mut(1).map(|value| *value).unwrap_or(in_l);

            let out_l = (saturation * in_l).clamp(-1.0, 1.0) * gain;
            let out_r = (saturation * in_r).clamp(-1.0, 1.0) * gain;

            if let Some(left) = channels.get_mut(0) {
                *left = out_l;
            }
            if let Some(right) = channels.get_mut(1) {
                *right = out_r;
            }

            self.meter_input_peak_l = self.meter_input_peak_l.max(in_l.abs());
            self.meter_input_peak_r = self.meter_input_peak_r.max(in_r.abs());
            self.meter_output_peak_l = self.meter_output_peak_l.max(out_l.abs());
            self.meter_output_peak_r = self.meter_output_peak_r.max(out_r.abs());

            samples_remaining = samples_remaining.saturating_sub(1);
            if samples_remaining == 0 {
                self.publish_meter_values();
                samples_remaining = self.meter_interval_samples;
            }
        }

        self.meter_samples_remaining = samples_remaining;

        ProcessStatus::Normal
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let saturation_changed = self.params.saturation_changed.clone();
        let gain_changed = self.params.gain_changed.clone();
        let meter_input_l = self.meter_input_l.clone();
        let meter_input_r = self.meter_input_r.clone();
        let meter_output_l = self.meter_output_l.clone();
        let meter_output_r = self.meter_output_r.clone();
        let meter_dirty = self.meter_dirty.clone();

        let source = if std::env::var("BASIC_PLUGIN_EXAMPLE_GUI_DEV_SERVER").as_deref() == Ok("1")
        {
            HTMLSource::URL(GUI_DEV_SERVER_URL)
        } else {
            HTMLSource::String(GUI_HTML)
        };

        let editor = WebViewEditor::new(source, (GUI_WIDTH, GUI_HEIGHT))
            .with_developer_mode(true)
            .with_event_loop(move |ctx, setter, _window| {
                while let Ok(value) = ctx.next_event() {
                    if let Ok(action) = serde_json::from_value::<Action>(value) {
                        match action {
                            Action::Init => {
                                ctx.send_json(json!({
                                    "type": "ParamChange",
                                    "saturation": params.saturation.value(),
                                    "gain": params.gain.value(),
                                }));
                            }
                            Action::SetSaturation { value } => {
                                setter.begin_set_parameter(&params.saturation);
                                setter.set_parameter(&params.saturation, value);
                                setter.end_set_parameter(&params.saturation);
                            }
                            Action::SetGain { value } => {
                                setter.begin_set_parameter(&params.gain);
                                setter.set_parameter(&params.gain, value);
                                setter.end_set_parameter(&params.gain);
                            }
                        }
                    }
                }

                if saturation_changed.swap(false, Ordering::Relaxed)
                    || gain_changed.swap(false, Ordering::Relaxed)
                {
                    ctx.send_json(json!({
                        "type": "ParamChange",
                        "saturation": params.saturation.value(),
                        "gain": params.gain.value(),
                    }));
                }

                if meter_dirty.swap(false, Ordering::Relaxed) {
                    ctx.send_json(json!({
                        "type": "Meter",
                        "input": {
                            "l": meter_input_l.load(Ordering::Relaxed),
                            "r": meter_input_r.load(Ordering::Relaxed),
                        },
                        "output": {
                            "l": meter_output_l.load(Ordering::Relaxed),
                            "r": meter_output_r.load(Ordering::Relaxed),
                        },
                    }));
                }
            });

        Some(Box::new(editor))
    }
}

impl Vst3Plugin for BasicPluginExample {
    const VST3_CLASS_ID: [u8; 16] = *b"WTH_BasicPlugEx_";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nih_export_vst3!(BasicPluginExample);

impl ClapPlugin for BasicPluginExample {
    const CLAP_ID: &'static str = "wthplugins.basic_plugin_example";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Basic saturation example with gain and a webview UI");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;

    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Distortion,
        ClapFeature::Stereo,
    ];
}

nih_export_clap!(BasicPluginExample);
