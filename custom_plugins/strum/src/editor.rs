use nih_plug::prelude::{Editor, GuiContext};
use nih_plug_iced::widgets as nih_widgets;
use nih_plug_iced::*;
use std::sync::Arc;

use crate::StrumParams;

pub(crate) fn default_state() -> Arc<IcedState> {
    IcedState::from_size(300, 220)
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
        Column::new()
            .align_items(Alignment::Center)
            .push(
                Text::new("Strum")
                    .font(assets::NOTO_SANS_LIGHT)
                    .size(32)
                    .height(50.into())
                    .width(Length::Fill)
                    .horizontal_alignment(alignment::Horizontal::Center)
                    .vertical_alignment(alignment::Vertical::Bottom),
            )
            .push(
                Text::new("Stagger")
                    .height(20.into())
                    .width(Length::Fill)
                    .horizontal_alignment(alignment::Horizontal::Center)
                    .vertical_alignment(alignment::Vertical::Center),
            )
            .push(
                nih_widgets::ParamSlider::new(
                    &mut self.stagger_slider_state,
                    &self.params.stagger_ms,
                )
                .map(Message::ParamUpdate),
            )
            .push(Space::with_height(10.into()))
            .push(
                Text::new("Direction")
                    .height(20.into())
                    .width(Length::Fill)
                    .horizontal_alignment(alignment::Horizontal::Center)
                    .vertical_alignment(alignment::Vertical::Center),
            )
            .push(
                nih_widgets::ParamSlider::new(
                    &mut self.direction_slider_state,
                    &self.params.direction,
                )
                .map(Message::ParamUpdate),
            )
            .into()
    }

    fn background_color(&self) -> Color {
        Color {
            r: 0.15,
            g: 0.15,
            b: 0.20,
            a: 1.0,
        }
    }
}
