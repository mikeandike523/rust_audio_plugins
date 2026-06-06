use nih_plug::prelude::{Editor, GuiContext};
use nih_plug_iced::widgets as nih_widgets;
use nih_plug_iced::*;
use std::sync::Arc;

use crate::StrumParams;

pub(crate) fn default_state() -> Arc<IcedState> {
    IcedState::from_size(440, 420)
}

pub(crate) fn create(
    params: Arc<StrumParams>,
    editor_state: Arc<IcedState>,
) -> Option<Box<dyn Editor>> {
    create_iced_editor::<StrumEditor>(editor_state, params)
}

struct StrumEditor {
    params: Arc<StrumParams>,
    context: Arc<dyn GuiContext>,
    stagger_slider_state: nih_widgets::param_slider::State,
    randomize_slider_state: nih_widgets::param_slider::State,
    direction_slider_state: nih_widgets::param_slider::State,
}

#[derive(Debug, Clone, Copy)]
enum Message {
    ParamUpdate(nih_widgets::ParamMessage),
}

impl IcedEditor for StrumEditor {
    type Executor = executor::Default;
    type Message = Message;
    type InitializationFlags = Arc<StrumParams>;

    fn new(params: Self::InitializationFlags, context: Arc<dyn GuiContext>) -> (Self, Command<Self::Message>) {
        let editor = StrumEditor {
            params,
            context,
            stagger_slider_state: Default::default(),
            randomize_slider_state: Default::default(),
            direction_slider_state: Default::default(),
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
            Message::ParamUpdate(message) => self.handle_param_message(message),
        }
        Command::none()
    }

    fn view(&mut self) -> Element<'_, Self::Message> {
        let label_color = Color::from_rgb8(160, 168, 200);
        Column::new()
            .align_items(Alignment::Center)
            .push(
                Text::new("Strum")
                    .font(assets::NOTO_SANS_LIGHT)
                    .size(44)
                    .color(Color::from_rgb8(225, 230, 255))
                    .height(75.into())
                    .width(Length::Fill)
                    .horizontal_alignment(alignment::Horizontal::Center)
                    .vertical_alignment(alignment::Vertical::Bottom),
            )
            .push(
                Text::new("Stagger")
                    .size(17)
                    .color(label_color)
                    .height(30.into())
                    .width(Length::Fill)
                    .horizontal_alignment(alignment::Horizontal::Center)
                    .vertical_alignment(alignment::Vertical::Center),
            )
            .push(
                nih_widgets::ParamSlider::new(
                    &mut self.stagger_slider_state,
                    &self.params.stagger_ms,
                )
                .width(Length::Units(360))
                .height(Length::Units(40))
                .text_size(15)
                .map(Message::ParamUpdate),
            )
            .push(Space::with_height(16.into()))
            .push(
                Text::new("Randomize")
                    .size(17)
                    .color(label_color)
                    .height(30.into())
                    .width(Length::Fill)
                    .horizontal_alignment(alignment::Horizontal::Center)
                    .vertical_alignment(alignment::Vertical::Center),
            )
            .push(
                nih_widgets::ParamSlider::new(
                    &mut self.randomize_slider_state,
                    &self.params.randomize_ms,
                )
                .width(Length::Units(360))
                .height(Length::Units(40))
                .text_size(15)
                .map(Message::ParamUpdate),
            )
            .push(Space::with_height(16.into()))
            .push(
                Text::new("Direction")
                    .size(17)
                    .color(label_color)
                    .height(30.into())
                    .width(Length::Fill)
                    .horizontal_alignment(alignment::Horizontal::Center)
                    .vertical_alignment(alignment::Vertical::Center),
            )
            .push(
                nih_widgets::ParamSlider::new(
                    &mut self.direction_slider_state,
                    &self.params.direction,
                )
                .width(Length::Units(360))
                .height(Length::Units(40))
                .text_size(15)
                .map(Message::ParamUpdate),
            )
            .into()
    }

    fn background_color(&self) -> Color {
        Color {
            r: 0.12,
            g: 0.13,
            b: 0.18,
            a: 1.0,
        }
    }
}
