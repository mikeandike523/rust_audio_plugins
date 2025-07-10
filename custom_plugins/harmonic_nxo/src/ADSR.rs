use std::f32::consts::E;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveType {
    Exponential,
    Linear,
}

pub struct Adsr {
    state: State,
    sustain_level: f32,
    level: f32,
    sample_rate: f32,
    attack_time: f32,
    decay_time: f32,
    release_time: f32,
    alpha_a: f32,
    alpha_d: f32,
    alpha_r: f32,
    attack_elapsed_samples: usize,
    decay_elapsed_samples: usize,
    release_elapsed_samples: usize,
    release_start_level: f32,
    curve_type: CurveType,
}

const TAU_UNTIL_FINISHED: f32 = 5.0;
const EPSILON: f32 = 1e-5;

impl Adsr {
    fn compute_alpha(time_secs: f32, sample_rate: f32) -> f32 {
        let tau = time_secs / TAU_UNTIL_FINISHED;
        1.0 - (-1.0 / (tau * sample_rate)).exp()
    }

    pub fn new(
        attack_time: f32,
        decay_time: f32,
        sustain: f32,
        release_time: f32,
        sample_rate: f32,
        curve_type: CurveType,
    ) -> Self {
        let alpha_a = Self::compute_alpha(attack_time, sample_rate);
        let alpha_d = Self::compute_alpha(decay_time, sample_rate);
        let alpha_r = Self::compute_alpha(release_time, sample_rate);

        Self {
            state: State::Idle,
            sustain_level: sustain,
            level: 0.0,
            sample_rate,
            attack_time,
            decay_time,
            release_time,
            alpha_a,
            alpha_d,
            alpha_r,
            attack_elapsed_samples: 0,
            decay_elapsed_samples: 0,
            release_elapsed_samples: 0,
            release_start_level: 0.0,
            curve_type,
        }
    }

    pub fn trigger(&mut self) {
        self.state = State::Attack;
        self.attack_elapsed_samples = 0;
        self.decay_elapsed_samples = 0;
        self.release_elapsed_samples = 0;
    }

    pub fn release(&mut self) {
        if self.state != State::Idle {
            self.state = State::Release;
            self.release_elapsed_samples = 0;
            self.release_start_level = self.level;
        }
    }

    pub fn reset(&mut self) {
        self.state = State::Idle;
        self.level = 0.0;
        self.attack_elapsed_samples = 0;
        self.decay_elapsed_samples = 0;
        self.release_elapsed_samples = 0;
    }

    pub fn next(&mut self) -> f32 {
        match self.state {
            State::Idle => {
                self.level = 0.0;
            }
            State::Attack => {
                match self.curve_type {
                    CurveType::Exponential => {
                        self.level = self.alpha_a * 1.0 + (1.0 - self.alpha_a) * self.level;
                    }
                    CurveType::Linear => {
                        let total = (self.attack_time * self.sample_rate).ceil();
                        self.level += 1.0 / total;
                    }
                }
                self.attack_elapsed_samples += 1;
                let total_attack_samples = (self.attack_time * self.sample_rate).ceil() as usize;
                if self.attack_elapsed_samples >= total_attack_samples {
                    self.level = 1.0;
                    self.state = State::Decay;
                }
            }
            State::Decay => {
                match self.curve_type {
                    CurveType::Exponential => {
                        self.level = self.alpha_d * self.sustain_level + (1.0 - self.alpha_d) * self.level;
                    }
                    CurveType::Linear => {
                        let total = (self.decay_time * self.sample_rate).ceil();
                        self.level -= (1.0 - self.sustain_level) / total;
                    }
                }
                self.decay_elapsed_samples += 1;
                let total_decay_samples = (self.decay_time * self.sample_rate).ceil() as usize;
                if self.decay_elapsed_samples >= total_decay_samples {
                    self.level = self.sustain_level;
                    self.state = State::Sustain;
                }
            }
            State::Sustain => {
                self.level = self.sustain_level;
            }
            State::Release => {
                match self.curve_type {
                    CurveType::Exponential => {
                        self.level = self.alpha_r * 0.0 + (1.0 - self.alpha_r) * self.level;
                    }
                    CurveType::Linear => {
                        let total = (self.release_time * self.sample_rate).ceil();
                        self.level -= self.release_start_level / total;
                    }
                }
                self.release_elapsed_samples += 1;
                let total_release_samples = (self.release_time * self.sample_rate).ceil() as usize;
                if self.release_elapsed_samples >= total_release_samples || self.level < EPSILON {
                    self.level = 0.0;
                    self.state = State::Idle;
                }
            }
        }
        self.level
    }

    pub fn is_idle(&self) -> bool {
        self.state == State::Idle
    }

    pub fn is_finished(&self) -> bool {
        self.state == State::Idle && self.level < EPSILON
    }

    pub fn get_state(&self) -> State {
        self.state
    }

    pub fn get_level(&self) -> f32 {
        self.level
    }

    // Dynamic setters

    pub fn set_attack_time(&mut self, attack_time: f32) {
        self.attack_time = attack_time;
        self.alpha_a = Self::compute_alpha(attack_time, self.sample_rate);
    }

    pub fn set_decay_time(&mut self, decay_time: f32) {
        self.decay_time = decay_time;
        self.alpha_d = Self::compute_alpha(decay_time, self.sample_rate);
    }

    pub fn set_release_time(&mut self, release_time: f32) {
        self.release_time = release_time;
        self.alpha_r = Self::compute_alpha(release_time, self.sample_rate);
    }

    pub fn set_sustain_level(&mut self, sustain: f32) {
        self.sustain_level = sustain;
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.alpha_a = Self::compute_alpha(self.attack_time, sample_rate);
        self.alpha_d = Self::compute_alpha(self.decay_time, sample_rate);
        self.alpha_r = Self::compute_alpha(self.release_time, sample_rate);
    }

    pub fn set_curve_type(&mut self, curve_type: CurveType) {
        self.curve_type = curve_type;
    }
}
