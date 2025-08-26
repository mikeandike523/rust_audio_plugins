// src/lib.rs

use nih_plug::prelude::*;
use std::num::NonZeroU32;
use std::sync::Arc;

#[derive(Default)]
pub struct MixMean;

impl Plugin for MixMean {
    const NAME: &'static str = "MixMean";
    const VENDOR: &'static str = "WTH Plugins";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // Stereo in → Stereo out, no sidechains/aux
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    // It’s a plain audio effect
    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        Arc::new(EmptyParams {})
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _ctx: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Iterate per-sample. `channels` acts like &mut [f32] for that sample across channels.
        for (_i, mut channels) in buffer.iter_samples().enumerate() {
            // Read the current values first
            let l = channels.get_mut(0).map(|x| *x).unwrap_or(0.0);
            let r = channels.get_mut(1).map(|x| *x).unwrap_or(l);
            let s = 0.5 * (l + r);

            // Now write the computed mean to both channels
            if let Some(left) = channels.get_mut(0) {
                *left = s;
            }
            if let Some(right) = channels.get_mut(1) {
                *right = s;
            }
        }

        ProcessStatus::Normal
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        None
    }
}

// ---- VST3 ----
impl Vst3Plugin for MixMean {
    // 16-byte identifier; change to your own if you like.
    const VST3_CLASS_ID: [u8; 16] = *b"WTH_MixMean_FX__";
    // Categorize it as an effect/utility
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Analyzer];
}
nih_export_vst3!(MixMean);

// ---- CLAP ----
impl ClapPlugin for MixMean {
    const CLAP_ID: &'static str = "wthplugins.mixmean";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Stereo mean mixer: (L+R)/2 → L/R");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;

    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Utility,
        ClapFeature::Stereo,
    ];
}
nih_export_clap!(MixMean);

// No parameters
#[derive(Params)]
struct EmptyParams {}
