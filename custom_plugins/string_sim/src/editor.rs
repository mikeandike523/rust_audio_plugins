use nih_plug::prelude::{Editor, GuiContext};
use nih_plug_iced::*;
use std::sync::Arc;

use crate::StringSimParams;

pub(crate) fn default_state() -> Arc<IcedState> {
    IcedState::from_size(400, 300)
}

pub(crate) fn create(
    params: Arc<StringSimParams>,
    editor_state: Arc<IcedState>,
) -> Option<Box<dyn Editor>> {
    create_iced_editor::<StringSimEditor>(editor_state, params)
}

struct StringSimEditor {
    #[allow(dead_code)]
    params: Arc<StringSimParams>,
    context: Arc<dyn GuiContext>,
}

#[derive(Debug, Clone, Copy)]
enum Message {}

impl IcedEditor for StringSimEditor {
    type Executor = executor::Default;
    type Message = Message;
    type InitializationFlags = Arc<StringSimParams>;

    fn new(
        params: Self::InitializationFlags,
        context: Arc<dyn GuiContext>,
    ) -> (Self, Command<Self::Message>) {
        let editor = StringSimEditor { params, context };
        (editor, Command::none())
    }

    fn context(&self) -> &dyn GuiContext {
        self.context.as_ref()
    }

    fn update(
        &mut self,
        _window: &mut WindowQueue,
        _message: Self::Message,
    ) -> Command<Self::Message> {
        Command::none()
    }

    fn view(&mut self) -> Element<'_, Self::Message> {
        Column::new()
            .align_items(Alignment::Center)
            .push(
                Text::new("String Sim")
                    .font(assets::NOTO_SANS_LIGHT)
                    .size(40)
                    .height(Length::Fill)
                    .width(Length::Fill)
                    .horizontal_alignment(alignment::Horizontal::Center)
                    .vertical_alignment(alignment::Vertical::Center),
            )
            .into()
    }

    fn background_color(&self) -> Color {
        Color {
            r: 0.12,
            g: 0.12,
            b: 0.16,
            a: 1.0,
        }
    }
}
