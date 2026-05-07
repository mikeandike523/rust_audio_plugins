use nih_plug::prelude::util;
use serde::{Deserialize, Serialize};
use tune::key::PianoKey;
use tune::scala::{Kbm, Scl};
use tune::tuning::KeyboardMapping;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TuningFile {
    pub name: String,
    pub contents: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TuningStatus {
    pub active: bool,
    pub scl_name: Option<String>,
    pub kbm_name: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct TuningState {
    scl: Option<Scl>,
    kbm: Option<Kbm>,
    pub status: TuningStatus,
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

impl TuningState {
    pub fn from_files(scl_file: Option<&TuningFile>, kbm_file: Option<&TuningFile>) -> Self {
        let scl_name = scl_file.map(|file| file.name.clone());
        let kbm_name = kbm_file.map(|file| file.name.clone());

        if scl_file.is_none() && kbm_file.is_none() {
            return Self {
                scl: None,
                kbm: None,
                status: TuningStatus {
                    active: false,
                    scl_name,
                    kbm_name,
                    error: None,
                },
            };
        }

        let scl_text = scl_file
            .map(|f| f.contents.as_str())
            .unwrap_or(DEFAULT_SCL_TEXT);
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
            status: TuningStatus {
                active: true,
                scl_name,
                kbm_name,
                error: None,
            },
        }
    }

    pub fn frequency_for_note(&self, note: f32) -> f32 {
        let floor_n = note.floor() as i32;
        let frac = note - floor_n as f32;
        let f0 = self.freq_for_degree(floor_n);
        let f1 = self.freq_for_degree(floor_n + 1);
        f0 * (1.0 - frac) + f1 * frac
    }

    fn freq_for_degree(&self, degree: i32) -> f32 {
        if let Some(scl) = &self.scl {
            if let Some(kbm) = &self.kbm {
                let key = PianoKey::from_midi_number(degree);
                if let Some(pitch) = (scl, kbm).maybe_pitch_of(key) {
                    return pitch.as_hz() as f32;
                }
            }
            let ratio = scl.relative_pitch_of(degree);
            return util::f32_midi_note_to_freq(degree as f32) * ratio.as_float() as f32;
        }

        util::f32_midi_note_to_freq(degree as f32)
    }
}
