#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

pub struct Adsr {
    state: State,
    alpha_a: f32,
    alpha_d: f32,
    alpha_r: f32,
    sustain_level: f32,
    level: f32,
}

impl Adsr {
    /// alpha values are smoothing coefficients in [0..1]
    pub fn new(alpha_attack: f32, alpha_decay: f32, sustain: f32, alpha_release: f32) -> Self {
        Self {
            state: State::Idle,
            alpha_a: alpha_attack,
            alpha_d: alpha_decay,
            alpha_r: alpha_release,
            sustain_level: sustain,
            level: 0.0,
        }
    }

    /// Start the envelope (go to attack)
    pub fn trigger(&mut self) {
        self.state = State::Attack;
    }

    /// Release the envelope (go to release)
    pub fn release(&mut self) {
        if self.state != State::Idle {
            self.state = State::Release;
        }
    }

    /// Advance one sample; return current level [0.0..1.0]
    pub fn next(&mut self) -> f32 {
        match self.state {
            State::Idle => {
                self.level = 0.0;
            }
            State::Attack => {
                // move toward 1.0
                self.level = self.alpha_a * 1.0 + (1.0 - self.alpha_a) * self.level;
                if self.level >= 0.999 {
                    self.state = State::Decay;
                }
            }
            State::Decay => {
                // move toward sustain_level
                self.level = self.alpha_d * self.sustain_level + (1.0 - self.alpha_d) * self.level;
                if (self.level - self.sustain_level).abs() < 0.001 {
                    self.state = State::Sustain;
                }
            }
            State::Sustain => {
                self.level = self.sustain_level;
            }
            State::Release => {
                // move toward 0.0
                self.level = (1.0 - self.alpha_r) * self.level;
                if self.level <= 0.001 {
                    self.level = 0.0;
                    self.state = State::Idle;
                }
            }
        }
        self.level
    }

    /// Returns true when envelope has finished its release phase
    pub fn is_idle(&self) -> bool {
        self.state == State::Idle
    }
}
