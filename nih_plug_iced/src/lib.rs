//! [iced](https://github.com/iced-rs/iced) editor support for NIH plug.

use baseview::WindowScalePolicy;
use crossbeam::atomic::AtomicCell;
use crossbeam::channel;
use nih_plug::params::persist::PersistentField;
use nih_plug::prelude::{Editor, GuiContext};
use serde::{Deserialize, Serialize};
// This doesn't need to be re-export but otherwise the compiler complains about
// `hidden_glob_reexports`
pub use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::widgets::ParamMessage;

/// Re-export for convenience.
// FIXME: Running `cargo doc` on nightly compilers without this attribute triggers an ICE
#[doc(no_inline)]
pub use iced_baseview::*;

pub mod assets;
mod editor;
pub mod widgets;
mod wrapper;

/// Create an [`Editor`] instance using [iced](https://github.com/iced-rs/iced).
pub fn create_iced_editor<E: IcedEditor>(
    iced_state: Arc<IcedState>,
    initialization_flags: E::InitializationFlags,
) -> Option<Box<dyn Editor>> {
    let (parameter_updates_sender, parameter_updates_receiver) = channel::bounded(1);

    Some(Box::new(editor::IcedEditorWrapper::<E> {
        iced_state,
        initialization_flags,

        #[cfg(target_os = "macos")]
        scaling_factor: AtomicCell::new(None),
        #[cfg(not(target_os = "macos"))]
        scaling_factor: AtomicCell::new(Some(1.0)),

        parameter_updates_sender,
        parameter_updates_receiver: Arc::new(parameter_updates_receiver),
    }))
}

/// A plugin editor using `iced`.
pub trait IcedEditor: 'static + Send + Sync + Sized {
    /// See [`Application::Executor`]. You'll likely want to use [`crate::executor::Default`].
    type Executor: Executor;
    /// See [`Application::Message`]. You should have one variant containing a [`ParamMessage`].
    type Message: 'static + Clone + Debug + Send;
    /// See [`Application::Flags`].
    type InitializationFlags: 'static + Clone + Send + Sync;

    /// See [`Application::new`]. This also receives the GUI context in addition to the flags.
    fn new(
        initialization_flags: Self::InitializationFlags,
        context: Arc<dyn GuiContext>,
    ) -> (Self, Command<Self::Message>);

    /// Returns a reference to the GUI context.
    fn context(&self) -> &dyn GuiContext;

    /// See [`Application::update`].
    fn update(
        &mut self,
        window: &mut WindowQueue,
        message: Self::Message,
    ) -> Command<Self::Message>;

    /// See [`Application::subscription`].
    fn subscription(
        &self,
        _window_subs: &mut WindowSubs<Self::Message>,
    ) -> Subscription<Self::Message> {
        Subscription::none()
    }

    /// See [`Application::view`].
    fn view(&mut self) -> Element<'_, Self::Message>;

    /// See [`Application::background_color`].
    fn background_color(&self) -> Color {
        Color::WHITE
    }

    /// See [`Application::scale_policy`].
    fn scale_policy(&self) -> WindowScalePolicy {
        WindowScalePolicy::SystemScaleFactor
    }

    /// See [`Application::renderer_settings`].
    fn renderer_settings() -> iced_baseview::backend::settings::Settings {
        iced_baseview::backend::settings::Settings {
            antialiasing: Some(iced_baseview::backend::settings::Antialiasing::MSAAx4),
            default_font: Some(crate::assets::fonts::NOTO_SANS_REGULAR),
            ..iced_baseview::backend::settings::Settings::default()
        }
    }

    /// Handle a parameter update using the GUI context.
    fn handle_param_message(&self, message: ParamMessage) {
        let context = self.context();
        match message {
            ParamMessage::BeginSetParameter(p) => unsafe { context.raw_begin_set_parameter(p) },
            ParamMessage::SetParameterNormalized(p, v) => unsafe {
                context.raw_set_parameter_normalized(p, v)
            },
            ParamMessage::EndSetParameter(p) => unsafe { context.raw_end_set_parameter(p) },
        }
    }
}

/// State for an `nih_plug_iced` editor.
#[derive(Debug, Serialize, Deserialize)]
pub struct IcedState {
    /// The window's size in logical pixels before applying `scale_factor`.
    #[serde(with = "nih_plug::params::persist::serialize_atomic_cell")]
    size: AtomicCell<(u32, u32)>,
    /// Whether the editor's window is currently open.
    #[serde(skip)]
    pub(crate) open: AtomicBool,
}

impl<'a> PersistentField<'a, IcedState> for Arc<IcedState> {
    fn set(&self, new_value: IcedState) {
        self.size.store(new_value.size.load());
    }

    fn map<F, R>(&self, f: F) -> R
    where
        F: Fn(&IcedState) -> R,
    {
        f(self)
    }
}

impl IcedState {
    /// Initialize the GUI's state. This value can be passed to [`create_iced_editor()`]. The window
    /// size is in logical pixels, so before it is multiplied by the DPI scaling factor.
    pub fn from_size(width: u32, height: u32) -> Arc<IcedState> {
        Arc::new(IcedState {
            size: AtomicCell::new((width, height)),
            open: AtomicBool::new(false),
        })
    }

    /// Returns a `(width, height)` pair for the current size of the GUI in logical pixels.
    pub fn size(&self) -> (u32, u32) {
        self.size.load()
    }

    /// Whether the GUI is currently visible.
    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::Acquire)
    }
}

/// A marker struct to indicate that a parameter update has happened.
pub(crate) struct ParameterUpdate;
