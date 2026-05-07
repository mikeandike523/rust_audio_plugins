use nih_plug::prelude::{Editor, GuiContext};
use nih_plug_iced::canvas;
use nih_plug_iced::widgets as nih_widgets;
use nih_plug_iced::*;
use std::sync::{Arc, Mutex};

use crate::tuning::{TuningFile, TuningState};
use crate::{StringSimParams, VisState};

pub(crate) fn default_state() -> Arc<IcedState> {
    IcedState::from_size(700, 560)
}

pub(crate) fn create(
    params: Arc<StringSimParams>,
    vis_state: Arc<Mutex<VisState>>,
    tuning_state: Arc<Mutex<TuningState>>,
    editor_state: Arc<IcedState>,
) -> Option<Box<dyn Editor>> {
    create_iced_editor::<StringSimEditor>(editor_state, (params, vis_state, tuning_state))
}

struct StringSimEditor {
    params:       Arc<StringSimParams>,
    context:      Arc<dyn GuiContext>,
    vis_state:    Arc<Mutex<VisState>>,
    tuning_state: Arc<Mutex<TuningState>>,

    // Pending file results written by background dialog threads.
    pending_scl: Arc<Mutex<Option<TuningFile>>>,
    pending_kbm: Arc<Mutex<Option<TuningFile>>>,

    tension_state:       nih_widgets::param_slider::State,
    spring_k_state:      nih_widgets::param_slider::State,
    bending_ei_state:    nih_widgets::param_slider::State,
    interior_damp_state: nih_widgets::param_slider::State,
    endpoint_damp_state: nih_widgets::param_slider::State,
    pickup_pos_state:    nih_widgets::param_slider::State,
    pluck_pos_state:     nih_widgets::param_slider::State,
    output_gain_state:   nih_widgets::param_slider::State,
    node_count_state:    nih_widgets::param_slider::State,

    // Button states (required by iced 0.4).
    btn_load_scl:  button::State,
    btn_clear_scl: button::State,
    btn_load_kbm:  button::State,
    btn_clear_kbm: button::State,
}

#[derive(Debug, Clone)]
enum Message {
    ParamUpdate(nih_widgets::ParamMessage),
    LoadScl,
    LoadKbm,
    ClearScl,
    ClearKbm,
    OnFrame,
}

impl IcedEditor for StringSimEditor {
    type Executor = executor::Default;
    type Message = Message;
    type InitializationFlags = (Arc<StringSimParams>, Arc<Mutex<VisState>>, Arc<Mutex<TuningState>>);

    fn new(
        (params, vis_state, tuning_state): Self::InitializationFlags,
        context: Arc<dyn GuiContext>,
    ) -> (Self, Command<Self::Message>) {
        let editor = Self {
            params,
            context,
            vis_state,
            tuning_state,
            pending_scl: Arc::new(Mutex::new(None)),
            pending_kbm: Arc::new(Mutex::new(None)),
            tension_state:       Default::default(),
            spring_k_state:      Default::default(),
            bending_ei_state:    Default::default(),
            interior_damp_state: Default::default(),
            endpoint_damp_state: Default::default(),
            pickup_pos_state:    Default::default(),
            pluck_pos_state:     Default::default(),
            output_gain_state:   Default::default(),
            node_count_state:    Default::default(),
            btn_load_scl:  Default::default(),
            btn_clear_scl: Default::default(),
            btn_load_kbm:  Default::default(),
            btn_clear_kbm: Default::default(),
        };
        (editor, Command::none())
    }

    fn context(&self) -> &dyn GuiContext {
        self.context.as_ref()
    }

    fn subscription(&self, window_subs: &mut WindowSubs<Self::Message>) -> Subscription<Self::Message> {
        window_subs.on_frame = Some(Message::OnFrame);
        Subscription::none()
    }

    fn update(
        &mut self,
        _window: &mut WindowQueue,
        message: Self::Message,
    ) -> Command<Self::Message> {
        match message {
            Message::ParamUpdate(msg) => self.handle_param_message(msg),

            Message::OnFrame => {
                // Poll pending SCL result from background dialog thread.
                if let Ok(mut slot) = self.pending_scl.try_lock() {
                    if let Some(file) = slot.take() {
                        self.apply_scl(Some(file));
                    }
                }
                // Poll pending KBM result.
                if let Ok(mut slot) = self.pending_kbm.try_lock() {
                    if let Some(file) = slot.take() {
                        self.apply_kbm(Some(file));
                    }
                }
            }

            Message::LoadScl => {
                let pending = Arc::clone(&self.pending_scl);
                std::thread::spawn(move || {
                    if let Some(path) = tinyfiledialogs::open_file_dialog(
                        "Load SCL tuning file",
                        "",
                        Some((&["*.scl"], "Scala scale files (*.scl)")),
                    ) {
                        if let Ok(contents) = std::fs::read_to_string(&path) {
                            let name = std::path::Path::new(&path)
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("unknown.scl")
                                .to_string();
                            if let Ok(mut slot) = pending.lock() {
                                *slot = Some(TuningFile { name, contents });
                            }
                        }
                    }
                });
            }

            Message::LoadKbm => {
                let pending = Arc::clone(&self.pending_kbm);
                std::thread::spawn(move || {
                    if let Some(path) = tinyfiledialogs::open_file_dialog(
                        "Load KBM keyboard mapping file",
                        "",
                        Some((&["*.kbm"], "Scala keyboard mapping files (*.kbm)")),
                    ) {
                        if let Ok(contents) = std::fs::read_to_string(&path) {
                            let name = std::path::Path::new(&path)
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("unknown.kbm")
                                .to_string();
                            if let Ok(mut slot) = pending.lock() {
                                *slot = Some(TuningFile { name, contents });
                            }
                        }
                    }
                });
            }

            Message::ClearScl => self.apply_scl(None),
            Message::ClearKbm => self.apply_kbm(None),
        }
        Command::none()
    }

    fn view(&mut self) -> Element<'_, Self::Message> {
        let label_color  = Color::from_rgb(0.70, 0.75, 0.85);
        let dim_color    = Color::from_rgb(0.45, 0.50, 0.60);
        let warn_color   = Color::from_rgb(1.00, 0.40, 0.40);
        let ok_color     = Color::from_rgb(0.35, 0.85, 0.50);
        macro_rules! param_row {
            ($lbl_a:expr, $state_a:expr, $param_a:expr,
             $lbl_b:expr, $state_b:expr, $param_b:expr) => {
                Row::new()
                    .spacing(8)
                    .push(
                        Column::new()
                            .spacing(2)
                            .push(Text::new($lbl_a).size(11).color(label_color).width(Length::Fill))
                            .push(
                                nih_widgets::ParamSlider::new($state_a, $param_a)
                                    .map(Message::ParamUpdate),
                            )
                            .width(Length::Fill),
                    )
                    .push(
                        Column::new()
                            .spacing(2)
                            .push(Text::new($lbl_b).size(11).color(label_color).width(Length::Fill))
                            .push(
                                nih_widgets::ParamSlider::new($state_b, $param_b)
                                    .map(Message::ParamUpdate),
                            )
                            .width(Length::Fill),
                    )
                    .padding(6)
            };
        }

        // Tuning status snapshot (don't hold lock across view build).
        let (scl_label, kbm_label, tuning_active, tuning_error) = {
            let ts = self.tuning_state.try_lock();
            match ts {
                Ok(t) => (
                    t.status.scl_name.clone().unwrap_or_else(|| "None".into()),
                    t.status.kbm_name.clone().unwrap_or_else(|| "None".into()),
                    t.status.active,
                    t.status.error.clone(),
                ),
                Err(_) => ("…".into(), "…".into(), false, None),
            }
        };

        let status_color = if tuning_error.is_some() {
            warn_color
        } else if tuning_active {
            ok_color
        } else {
            dim_color
        };

        let status_text = if let Some(err) = &tuning_error {
            format!("Error: {err}")
        } else if tuning_active {
            "Custom tuning active".into()
        } else {
            "12-TET (no tuning loaded)".into()
        };

        // SCL row
        let scl_row = Row::new()
            .spacing(6)
            .align_items(Alignment::Center)
            .push(Text::new("SCL:").size(11).color(dim_color))
            .push(Text::new(&scl_label).size(11).color(label_color).width(Length::Fill))
            .push(
                Button::new(&mut self.btn_load_scl, Text::new("Load").size(11))
                    .on_press(Message::LoadScl)
                    .padding([3, 8]),
            )
            .push(
                Button::new(&mut self.btn_clear_scl, Text::new("Clear").size(11))
                    .on_press(Message::ClearScl)
                    .padding([3, 8]),
            );

        // KBM row
        let kbm_row = Row::new()
            .spacing(6)
            .align_items(Alignment::Center)
            .push(Text::new("KBM:").size(11).color(dim_color))
            .push(Text::new(&kbm_label).size(11).color(label_color).width(Length::Fill))
            .push(
                Button::new(&mut self.btn_load_kbm, Text::new("Load").size(11))
                    .on_press(Message::LoadKbm)
                    .padding([3, 8]),
            )
            .push(
                Button::new(&mut self.btn_clear_kbm, Text::new("Clear").size(11))
                    .on_press(Message::ClearKbm)
                    .padding([3, 8]),
            );

        let tuning_section = Column::new()
            .spacing(4)
            .push(Text::new(&status_text).size(10).color(status_color))
            .push(scl_row)
            .push(kbm_row)
            .padding([4, 6]);

        Column::new()
            .align_items(Alignment::Center)
            // Title
            .push(
                Text::new("String Sim")
                    .font(assets::NOTO_SANS_LIGHT)
                    .size(24)
                    .color(Color::WHITE)
                    .width(Length::Fill)
                    .height(36.into())
                    .horizontal_alignment(alignment::Horizontal::Center)
                    .vertical_alignment(alignment::Vertical::Center),
            )
            // String canvas
            .push(
                Canvas::new(StringCanvas {
                    vis_state: Arc::clone(&self.vis_state),
                })
                .width(Length::Fill)
                .height(160.into()),
            )
            .push(Space::with_height(4.into()))
            // Row 1 — Tension | Spring K
            .push(param_row!(
                "Tension (N)",      &mut self.tension_state,       &self.params.tension,
                "Spring K (N/m)",   &mut self.spring_k_state,      &self.params.spring_k
            ))
            // Row 2 — Bending EI | Interior Damp
            .push(param_row!(
                "Bending EI (N·m²)", &mut self.bending_ei_state,   &self.params.bending_ei,
                "Interior Damp",     &mut self.interior_damp_state, &self.params.interior_damp
            ))
            // Row 3 — Endpoint Damp | Pickup Pos
            .push(param_row!(
                "Endpoint Damp", &mut self.endpoint_damp_state, &self.params.endpoint_damp,
                "Pickup Pos",    &mut self.pickup_pos_state,    &self.params.pickup_pos
            ))
            // Row 4 — Pluck Pos | Output Gain
            .push(param_row!(
                "Pluck Pos",   &mut self.pluck_pos_state,   &self.params.pluck_pos,
                "Output Gain", &mut self.output_gain_state, &self.params.output_gain
            ))
            // Row 5 — Node Count (restarts sim)
            .push(
                Row::new()
                    .push(
                        Column::new()
                            .spacing(2)
                            .push(
                                Text::new("Node Count (restarts sim)")
                                    .size(11)
                                    .color(warn_color)
                                    .width(Length::Fill),
                            )
                            .push(
                                nih_widgets::ParamSlider::new(
                                    &mut self.node_count_state,
                                    &self.params.node_count,
                                )
                                .map(Message::ParamUpdate),
                            )
                            .width(Length::Fill),
                    )
                    .padding(6),
            )
            // Tuning section
            .push(tuning_section)
            .into()
    }

    fn background_color(&self) -> Color {
        Color::from_rgb(0.10, 0.10, 0.14)
    }
}

impl StringSimEditor {
    fn rebuild_tuning(&self, scl: Option<TuningFile>, kbm: Option<TuningFile>) {
        let new_state = TuningState::from_files(scl.as_ref(), kbm.as_ref());
        if let Ok(mut ts) = self.tuning_state.lock() {
            *ts = new_state;
        }
    }

    fn apply_scl(&self, file: Option<TuningFile>) {
        *self.params.scl_file.lock().unwrap() = file.clone();
        let kbm = self.params.kbm_file.lock().unwrap().clone();
        self.rebuild_tuning(file, kbm);
    }

    fn apply_kbm(&self, file: Option<TuningFile>) {
        *self.params.kbm_file.lock().unwrap() = file.clone();
        let scl = self.params.scl_file.lock().unwrap().clone();
        self.rebuild_tuning(scl, file);
    }
}

// ─── Canvas ──────────────────────────────────────────────────────────────────

struct StringCanvas {
    vis_state: Arc<Mutex<VisState>>,
}

impl canvas::Program<Message> for StringCanvas {
    fn draw(&self, bounds: Rectangle, _cursor: canvas::Cursor) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(bounds.size());

        frame.fill_rectangle(
            Point::ORIGIN,
            bounds.size(),
            Color::from_rgb(0.04, 0.04, 0.09),
        );

        let vis = match self.vis_state.try_lock() {
            Ok(v) => v,
            Err(_) => return vec![frame.into_geometry()],
        };

        let eff = vis.effective_end;
        if eff < 2 {
            return vec![frame.into_geometry()];
        }

        let w = bounds.width;
        let h = bounds.height;
        let cx_scale = w / eff as f32;
        let cy_mid = h / 2.0;
        let y_scale = 60_000.0_f32;

        let spring_color  = Color::from_rgb(0.20, 0.60, 0.95);
        let mass_color    = Color::from_rgb(0.55, 0.88, 1.00);
        let pin_color     = Color::from_rgb(1.00, 0.40, 0.40);
        let eq_line_color = Color::from_rgba(1.0, 1.0, 1.0, 0.06);

        frame.stroke(
            &canvas::Path::line(Point::new(0.0, cy_mid), Point::new(w, cy_mid)),
            canvas::Stroke { color: eq_line_color, width: 1.0, ..Default::default() },
        );

        for i in 0..eff {
            let x1 = i as f32 * cx_scale;
            let x2 = (i + 1) as f32 * cx_scale;
            let y1 = cy_mid - vis.y[i] * y_scale;
            let y2 = cy_mid - vis.y[i + 1] * y_scale;
            frame.stroke(
                &canvas::Path::line(Point::new(x1, y1), Point::new(x2, y2)),
                canvas::Stroke { color: spring_color, width: 1.5, ..Default::default() },
            );
        }

        for i in 1..eff {
            let x = i as f32 * cx_scale;
            let y = cy_mid - vis.y[i] * y_scale;
            frame.fill(&canvas::Path::circle(Point::new(x, y), 2.5), mass_color);
        }

        for &idx in &[0usize, eff] {
            let x = idx as f32 * cx_scale;
            frame.fill(&canvas::Path::circle(Point::new(x, cy_mid), 4.0), pin_color);
        }

        vec![frame.into_geometry()]
    }
}
