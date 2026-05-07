const MU: f64 = 0.003;
const TENSION_DEFAULT: f64 = 40.0;
const SPRING_K_DEFAULT: f64 = 30_000.0;
const EI_DEFAULT: f64 = 3e-8;
const INTERIOR_DAMP_DEFAULT: f64 = 0.25;
const EP_DAMP_DEFAULT: f64 = 0.75;
const EP_TAPER_COUNT: usize = 3;
const PLUCK_FRACTION_DEFAULT: f64 = 1.0 / 3.0;
const PLUCK_AMPLITUDE: f64 = 0.001;
const PICKUP_FRACTION_DEFAULT: f64 = 0.15;
const OUTPUT_GAIN_DEFAULT: f64 = 2.5;

const LOWEST_MIDI_NOTE: u8 = 40;
const MAX_SEG_LENGTH: f64 = 0.005;

pub struct StringPhysics {
    n_total:            usize,
    seg_len:            f64,
    node_mass:          f64,
    c_wave:             f64,
    base_tension:       f64,
    spring_k:           f64,
    bfc:                f64,
    interior_damping:   f64,
    endpoint_damp_base: f64,
    ep_taper_count:     usize,
    output_gain:        f64,
    pluck_fraction:     f64,
    pickup_fraction:    f64,
    dt:                 f64,
    sample_rate:        f64,
    effective_end:      usize,
    pickup_index:       usize,
    desired_freq:       Option<f64>,
    y:       Vec<f64>,
    vy:      Vec<f64>,
    ay:      Vec<f64>,
    y_pred:  Vec<f64>,
    vy_pred: Vec<f64>,
    ay_pred: Vec<f64>,
}

impl StringPhysics {
    #[allow(dead_code)]
    pub fn new(sample_rate: f32) -> Self {
        Self::new_with_n(sample_rate, Self::default_n_total())
    }

    pub fn new_with_n(sample_rate: f32, n_total: usize) -> Self {
        let n_total = n_total.max(6);
        let sr = sample_rate as f64;
        let f_min = 440.0 * 2.0_f64.powf((LOWEST_MIDI_NOTE as f64 - 69.0) / 12.0);
        let c_wave = (TENSION_DEFAULT / MU).sqrt();
        let l_max = c_wave / (2.0 * f_min);
        let seg_len = l_max / (n_total - 1) as f64;
        let node_mass = MU * seg_len;
        let bfc = EI_DEFAULT / (seg_len * seg_len * seg_len);
        let dt = 1.0 / sr;
        let effective_end = (n_total - 2).max(4);
        let pickup_index = (PICKUP_FRACTION_DEFAULT * effective_end as f64).round() as usize;

        Self {
            n_total,
            seg_len,
            node_mass,
            c_wave,
            base_tension: TENSION_DEFAULT,
            spring_k: SPRING_K_DEFAULT,
            bfc,
            interior_damping: INTERIOR_DAMP_DEFAULT,
            endpoint_damp_base: EP_DAMP_DEFAULT,
            ep_taper_count: EP_TAPER_COUNT,
            output_gain: OUTPUT_GAIN_DEFAULT,
            pluck_fraction: PLUCK_FRACTION_DEFAULT,
            pickup_fraction: PICKUP_FRACTION_DEFAULT,
            dt,
            sample_rate: sr,
            effective_end,
            pickup_index,
            desired_freq: None,
            y:       vec![0.0; n_total],
            vy:      vec![0.0; n_total],
            ay:      vec![0.0; n_total],
            y_pred:  vec![0.0; n_total],
            vy_pred: vec![0.0; n_total],
            ay_pred: vec![0.0; n_total],
        }
    }

    #[allow(dead_code)]
    pub fn default_n_total() -> usize {
        let f_min = 440.0 * 2.0_f64.powf((LOWEST_MIDI_NOTE as f64 - 69.0) / 12.0);
        let c_wave = (TENSION_DEFAULT / MU).sqrt();
        let l_max = c_wave / (2.0 * f_min);
        (l_max / MAX_SEG_LENGTH).ceil() as usize + 1
    }

    // --- Setters ---

    pub fn set_tension(&mut self, v: f64) {
        self.base_tension = v;
        self.c_wave = (v / MU).sqrt();
    }
    pub fn set_spring_k(&mut self, v: f64) {
        self.spring_k = v;
    }
    pub fn set_bending_ei(&mut self, v: f64) {
        let dx3 = self.seg_len * self.seg_len * self.seg_len;
        self.bfc = v / dx3;
    }
    pub fn set_interior_damp(&mut self, v: f64) {
        self.interior_damping = v;
    }
    pub fn set_endpoint_damp(&mut self, v: f64) {
        self.endpoint_damp_base = v;
    }
    pub fn set_pickup_fraction(&mut self, v: f64) {
        self.pickup_fraction = v;
        let idx = (v * self.effective_end as f64).round() as usize;
        self.pickup_index = idx.clamp(1, self.effective_end.saturating_sub(1).max(1));
    }
    pub fn set_pluck_fraction(&mut self, v: f64) {
        self.pluck_fraction = v;
    }
    pub fn set_output_gain(&mut self, v: f64) {
        self.output_gain = v;
    }

    // --- Getters ---

    pub fn n_total(&self) -> usize {
        self.n_total
    }
    #[allow(dead_code)]
    pub fn sample_rate_f64(&self) -> f64 {
        self.sample_rate
    }
    pub fn y_slice(&self) -> &[f64] {
        &self.y
    }
    pub fn effective_end(&self) -> usize {
        self.effective_end
    }

    // --- Core methods ---

    pub fn set_pitch(&mut self, freq_hz: f64) {
        self.desired_freq = Some(freq_hz);
        self.effective_end =
            freq_to_effective_end(freq_hz, self.c_wave, self.seg_len, self.n_total);
        let idx = (self.pickup_fraction * self.effective_end as f64).round() as usize;
        self.pickup_index = idx.clamp(1, self.effective_end.saturating_sub(1).max(1));
        let eff = self.effective_end;
        self.y[eff..].fill(0.0);
        self.vy[eff..].fill(0.0);
    }

    /// Call once per block after all params are applied. Repositions the fret so the
    /// resonant frequency stays at the desired pitch regardless of param changes.
    pub fn recompute_fret(&mut self) {
        if let Some(freq) = self.desired_freq {
            self.effective_end =
                freq_to_effective_end(freq, self.c_wave, self.seg_len, self.n_total);
            let idx = (self.pickup_fraction * self.effective_end as f64).round() as usize;
            self.pickup_index = idx.clamp(1, self.effective_end.saturating_sub(1).max(1));
        }
    }

    pub fn pluck(&mut self, velocity: u8) {
        self.y.fill(0.0);
        self.vy.fill(0.0);
        let amp = PLUCK_AMPLITUDE * (velocity as f64 / 127.0);
        let apex = ((self.pluck_fraction * self.effective_end as f64).round() as usize)
            .clamp(1, self.effective_end - 1);
        for i in 1..apex {
            self.y[i] = amp * (i as f64 / apex as f64);
        }
        for i in apex..self.effective_end {
            let t = (i - apex) as f64 / (self.effective_end - apex) as f64;
            self.y[i] = amp * (1.0 - t);
        }
    }

    pub fn step(&mut self) {
        let dt = self.dt;
        let n = self.n_total;
        let eff = self.effective_end;

        compute_accelerations(
            &self.y, &self.vy, &mut self.ay,
            eff, self.seg_len, self.node_mass,
            self.base_tension, self.spring_k, self.interior_damping,
            self.bfc, self.endpoint_damp_base, self.ep_taper_count,
            self.sample_rate,
        );

        for i in 0..n {
            self.y_pred[i]  = self.y[i]  + self.vy[i] * dt + 0.5 * self.ay[i] * dt * dt;
            self.vy_pred[i] = self.vy[i] + self.ay[i] * dt;
        }
        enforce_bc(&mut self.y_pred, &mut self.vy_pred, eff);

        compute_accelerations(
            &self.y_pred, &self.vy_pred, &mut self.ay_pred,
            eff, self.seg_len, self.node_mass,
            self.base_tension, self.spring_k, self.interior_damping,
            self.bfc, self.endpoint_damp_base, self.ep_taper_count,
            self.sample_rate,
        );

        for i in 0..n {
            let vy_new = self.vy[i] + 0.5 * (self.ay[i] + self.ay_pred[i]) * dt;
            self.y[i]  = self.y[i]  + 0.5 * (self.vy[i] + vy_new) * dt;
            self.vy[i] = vy_new;
        }
        enforce_bc(&mut self.y, &mut self.vy, eff);
    }

    pub fn output(&self) -> f32 {
        (self.vy[self.pickup_index] * self.output_gain) as f32
    }
}

fn freq_to_effective_end(freq_hz: f64, c_wave: f64, seg_len: f64, n_total: usize) -> usize {
    let l_eff = c_wave / (2.0 * freq_hz);
    let idx = (l_eff / seg_len).round() as usize;
    idx.clamp(4, n_total - 2)
}

fn enforce_bc(y: &mut [f64], vy: &mut [f64], effective_end: usize) {
    y[0] = 0.0;
    vy[0] = 0.0;
    for i in effective_end..y.len() {
        y[i] = 0.0;
        vy[i] = 0.0;
    }
}

fn compute_accelerations(
    y: &[f64],
    vy: &[f64],
    ay: &mut [f64],
    effective_end: usize,
    seg_len: f64,
    node_mass: f64,
    base_tension: f64,
    spring_k: f64,
    interior_damping: f64,
    bfc: f64,
    endpoint_damp_base: f64,
    ep_taper_count: usize,
    sample_rate: f64,
) {
    ay.fill(0.0);

    for i in 0..effective_end {
        let dy = y[i + 1] - y[i];
        let dist = (seg_len * seg_len + dy * dy).sqrt();
        if dist == 0.0 {
            continue;
        }
        let excess_ext = dist - seg_len;
        let rel_vy = vy[i + 1] - vy[i];
        let ext_rate = rel_vy * dy / dist;
        let force_mag = base_tension + spring_k * excess_ext - interior_damping * ext_rate;
        let force_y = force_mag * (dy / dist);
        ay[i]     += force_y / node_mass;
        ay[i + 1] -= force_y / node_mass;
    }

    if effective_end >= 4 {
        for i in 2..=(effective_end - 2) {
            let biharm = y[i - 2] - 4.0 * y[i - 1] + 6.0 * y[i] - 4.0 * y[i + 1] + y[i + 2];
            ay[i] -= bfc * biharm / node_mass;
        }
    }

    for d in 1..=ep_taper_count {
        let t = (1.0 - d as f64 / ep_taper_count as f64).max(0.0);
        let coeff = t * endpoint_damp_base * node_mass * sample_rate;
        ay[d] -= coeff * vy[d] / node_mass;
        if effective_end > d {
            ay[effective_end - d] -= coeff * vy[effective_end - d] / node_mass;
        }
    }
}
