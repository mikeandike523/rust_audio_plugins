use nih_plug::prelude::{Editor, GuiContext};
use nih_plug_iced::canvas;
use nih_plug_iced::widgets as nih_widgets;
use nih_plug_iced::*;
use std::sync::{Arc, Mutex};

use crate::{StringSimParams, VisState};

pub(crate) fn default_state() -> Arc<IcedState> {
    IcedState::from_size(700, 500)
}

pub(crate) fn create(
    params: Arc<StringSimParams>,
    vis_state: Arc<Mutex<VisState>>,
    editor_state: Arc<IcedState>,
) -> Option<Box<dyn Editor>> {
    create_iced_editor::<StringSimEditor>(editor_state, (params, vis_state))
}

struct StringSimEditor {
    params:    Arc<StringSimParams>,
    context:   Arc<dyn GuiContext>,
    vis_state: Arc<Mutex<VisState>>,

    tension_state:       nih_widgets::param_slider::State,
    spring_k_state:      nih_widgets::param_slider::State,
    bending_ei_state:    nih_widgets::param_slider::State,
    interior_damp_state: nih_widgets::param_slider::State,
    endpoint_damp_state: nih_widgets::param_slider::State,
    pickup_pos_state:    nih_widgets::param_slider::State,
    pluck_pos_state:     nih_widgets::param_slider::State,
    output_gain_state:   nih_widgets::param_slider::State,
    node_count_state:    nih_widgets::param_slider::State,
}

#[derive(Debug, Clone, Copy)]
enum Message {
    ParamUpdate(nih_widgets::ParamMessage),
}

impl IcedEditor for StringSimEditor {
    type Executor = executor::Default;
    type Message = Message;
    type InitializationFlags = (Arc<StringSimParams>, Arc<Mutex<VisState>>);

    fn new(
        (params, vis_state): Self::InitializationFlags,
        context: Arc<dyn GuiContext>,
    ) -> (Self, Command<Self::Message>) {
        let editor = Self {
            params,
            context,
            vis_state,
            tension_state:       Default::default(),
            spring_k_state:      Default::default(),
            bending_ei_state:    Default::default(),
            interior_damp_state: Default::default(),
            endpoint_damp_state: Default::default(),
            pickup_pos_state:    Default::default(),
            pluck_pos_state:     Default::default(),
            output_gain_state:   Default::default(),
            node_count_state:    Default::default(),
        };
        (editor, Command::none())
    }

    fn context(&self) -> &dyn GuiContext {
        self.context.as_ref()
    }

    fn update(
        &mut self,
        _window: &mut WindowQueue,
        message: Self::Message,
    ) -> Command<Self::Message> {
        match message {
            Message::ParamUpdate(msg) => self.handle_param_message(msg),
        }
        Command::none()
    }

    fn view(&mut self) -> Element<'_, Self::Message> {
        let label_color = Color::from_rgb(0.70, 0.75, 0.85);

        // Helper to build a two-column param row.
        macro_rules! param_row {
            ($lbl_a:expr, $state_a:expr, $param_a:expr,
             $lbl_b:expr, $state_b:expr, $param_b:expr) => {
                Row::new()
                    .spacing(8)
                    .push(
                        Column::new()
                            .spacing(2)
                            .push(
                                Text::new($lbl_a)
                                    .size(11)
                                    .color(label_color)
                                    .width(Length::Fill),
                            )
                            .push(
                                nih_widgets::ParamSlider::new($state_a, $param_a)
                                    .map(Message::ParamUpdate),
                            )
                            .width(Length::Fill),
                    )
                    .push(
                        Column::new()
                            .spacing(2)
                            .push(
                                Text::new($lbl_b)
                                    .size(11)
                                    .color(label_color)
                                    .width(Length::Fill),
                            )
                            .push(
                                nih_widgets::ParamSlider::new($state_b, $param_b)
                                    .map(Message::ParamUpdate),
                            )
                            .width(Length::Fill),
                    )
                    .padding(6)
            };
        }

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
            // Physical params: row 1 — Tension | Spring K
            .push(param_row!(
                "Tension (N)",
                &mut self.tension_state,
                &self.params.tension,
                "Spring K (N/m)",
                &mut self.spring_k_state,
                &self.params.spring_k
            ))
            // Row 2 — Bending EI | Interior Damp
            .push(param_row!(
                "Bending EI (N·m²)",
                &mut self.bending_ei_state,
                &self.params.bending_ei,
                "Interior Damp",
                &mut self.interior_damp_state,
                &self.params.interior_damp
            ))
            // Row 3 — Endpoint Damp | Pickup Pos
            .push(param_row!(
                "Endpoint Damp",
                &mut self.endpoint_damp_state,
                &self.params.endpoint_damp,
                "Pickup Pos",
                &mut self.pickup_pos_state,
                &self.params.pickup_pos
            ))
            // Row 4 — Pluck Pos | Output Gain
            .push(param_row!(
                "Pluck Pos",
                &mut self.pluck_pos_state,
                &self.params.pluck_pos,
                "Output Gain",
                &mut self.output_gain_state,
                &self.params.output_gain
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
                                    .color(Color::from_rgb(1.0, 0.75, 0.35))
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
            .into()
    }

    fn background_color(&self) -> Color {
        Color::from_rgb(0.10, 0.10, 0.14)
    }
}

// ─── Canvas ──────────────────────────────────────────────────────────────────

struct StringCanvas {
    vis_state: Arc<Mutex<VisState>>,
}

impl canvas::Program<Message> for StringCanvas {
    fn draw(&self, bounds: Rectangle, _cursor: canvas::Cursor) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(bounds.size());

        // Background
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
        // 60 000 px/m → 1 mm pluck ≈ 60 px (fits well in ~160 px canvas height)
        let y_scale = 60_000.0_f32;

        let spring_color  = Color::from_rgb(0.20, 0.60, 0.95);
        let mass_color    = Color::from_rgb(0.55, 0.88, 1.00);
        let pin_color     = Color::from_rgb(1.00, 0.40, 0.40);
        let eq_line_color = Color::from_rgba(1.0, 1.0, 1.0, 0.06);

        // Equilibrium centre line
        frame.stroke(
            &canvas::Path::line(
                Point::new(0.0, cy_mid),
                Point::new(w, cy_mid),
            ),
            canvas::Stroke {
                color: eq_line_color,
                width: 1.0,
                ..Default::default()
            },
        );

        // Spring segments (lines between adjacent nodes)
        for i in 0..eff {
            let x1 = i as f32 * cx_scale;
            let x2 = (i + 1) as f32 * cx_scale;
            let y1 = cy_mid - vis.y[i] * y_scale;
            let y2 = cy_mid - vis.y[i + 1] * y_scale;
            frame.stroke(
                &canvas::Path::line(Point::new(x1, y1), Point::new(x2, y2)),
                canvas::Stroke {
                    color: spring_color,
                    width: 1.5,
                    ..Default::default()
                },
            );
        }

        // Mass circles (every node, skip endpoints which get pin circles)
        for i in 1..eff {
            let x = i as f32 * cx_scale;
            let y = cy_mid - vis.y[i] * y_scale;
            frame.fill(
                &canvas::Path::circle(Point::new(x, y), 2.5),
                mass_color,
            );
        }

        // Pinned endpoints (slightly larger, red-ish)
        for &idx in &[0usize, eff] {
            let x = idx as f32 * cx_scale;
            let y = cy_mid;
            frame.fill(
                &canvas::Path::circle(Point::new(x, y), 4.0),
                pin_color,
            );
        }

        vec![frame.into_geometry()]
    }
}
