use std::f32::consts::E;

/// How many time constants should pass before the envelope is considered
/// finished with a stage. The higher this number, the closer the envelope
/// will get to the target value at the end of the stage.
pub const NUM_TAU_FINISHED: f32 = 5.0;

/// Parameters for an ADSR envelope.
#[derive(Debug, Clone, Copy)]
pub struct EnvelopeParams {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

fn tau(time: f32) -> f32 {
    if time <= 0.0 {
        // Avoid division by zero. A zero time constant results in an
        // instantaneous jump to the target.
        f32::EPSILON
    } else {
        time / NUM_TAU_FINISHED
    }
}

fn pre_release_value(t: f32, params: &EnvelopeParams) -> f32 {
    if t <= params.attack {
        let tau_a = tau(params.attack);
        1.0 - (-t / tau_a).exp()
    } else if t <= params.attack + params.decay {
        let tau_d = tau(params.decay);
        let dt = t - params.attack;
        params.sustain + (1.0 - params.sustain) * (-dt / tau_d).exp()
    } else {
        params.sustain
    }
}

fn release_value(start_level: f32, t: f32, release: f32) -> f32 {
    if release <= 0.0 {
        0.0
    } else {
        let tau_r = tau(release);
        start_level * (-t / tau_r).exp()
    }
}

/// Compute the envelope's amplitude at time `t_since_on` (seconds) given an
/// optional `note_off` time (seconds since note on).
pub fn value_at(t_since_on: f32, note_off: Option<f32>, params: &EnvelopeParams) -> f32 {
    if let Some(off) = note_off {
        if t_since_on >= off {
            let start_level = pre_release_value(off, params);
            release_value(start_level, t_since_on - off, params.release)
        } else {
            pre_release_value(t_since_on, params)
        }
    } else {
        pre_release_value(t_since_on, params)
    }
}

/// Returns `true` if the envelope has effectively finished after
/// `t_since_on` seconds.
pub fn is_finished(t_since_on: f32, note_off: Option<f32>, params: &EnvelopeParams) -> bool {
    if let Some(off) = note_off {
        t_since_on >= off + params.release
    } else {
        false
    }
}
