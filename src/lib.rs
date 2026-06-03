//! Composable widget toolkit for terminal UIs, optionally rendered as a GUI.

extern crate self as tuie;

#[doc(hidden)]
pub use paste;

pub mod ansi;
pub mod editor;
pub mod emulator;
#[cfg(feature = "gui")]
pub mod gui;
pub mod input;
pub mod render;
pub mod runtime;
pub mod theme;
pub mod util;
pub mod widget;

#[cfg(test)]
mod tests;

#[doc(hidden)]
pub use runtime::{
    config, dirty_layout, dirty_paint,
    disable, emit, enable, ensure_focused, focus_widget, get_focus_chain,
    get_focused_measure, get_focused_widget, get_terminal_info, focus_next_tab_order, focus_next_directionally,
    on_quit, quit, reveal, schedule, send, set_output, set_spawner, spawn, start_tui,
    spawn_stream, suspend,
};
pub use runtime::clipboard;
#[doc(hidden)]
pub use render::{terminal_display_width, terminal_grapheme_width};
#[doc(hidden)]
pub use runtime::popup::{close_popup, dismiss_popup, open_popup};

#[cfg(feature = "gui")]
#[doc(hidden)]
pub use runtime::start_gui;

#[cfg(feature = "gui")]
pub use gui::title_bar_insets;

/// Returns the columns to keep clear of OS title-bar chrome on the left and right of row 0.
#[cfg(not(feature = "gui"))]
pub fn title_bar_insets() -> (u16, u16) {
    (0, 0)
}

#[macro_export]
macro_rules! config_module {
    ($cfg:ident { $($body:tt)* }) => {
        impl ::std::default::Default for $cfg {
            fn default() -> Self {
                Self { $($body)* }
            }
        }

        pub mod config {
            #[allow(unused_imports)]
            use super::*;
            ::std::thread_local! {
                static CONFIG: ::std::cell::Cell<super::$cfg> =
                    const { ::std::cell::Cell::new(super::$cfg { $($body)* }) };
            }

            /// Returns a copy of the current configuration.
            pub fn get() -> super::$cfg {
                CONFIG.with(|c| c.get())
            }

            /// Replaces the configuration.
            pub fn set(cfg: super::$cfg) {
                CONFIG.with(|c| c.set(cfg));
                $crate::dirty_layout();
            }

            /// Applies `f` to the configuration in place.
            pub fn update(f: impl FnOnce(&mut super::$cfg)) {
                let mut cfg = get();
                f(&mut cfg);
                set(cfg);
            }
        }
    };
}

/// Common types and traits for building widgets.
#[allow(unused_imports)]
pub mod prelude {
    pub use sign::Sign;
    pub use sign::SignIndex;

    pub use axis2d::Axis2D;
    pub use axis2d::Direction2D;
    pub use axis2d::Edge2D;
    pub use axis2d::HorizontalEdge;
    pub use axis2d::Rect;
    pub use axis2d::VerticalEdge;
    pub use axis2d::Vec2;

    pub use super::widget::align::Align;
    pub use super::widget::align::FlexAlign;
    pub use super::widget::align::Place;
    pub use super::render::border::Border;
    pub use super::render::border::BorderConfig;
    pub use super::widget::chrome::Chrome;
    pub use super::widget::chrome::ChromeTitle;
    pub use super::render::color::Color;
    pub use super::render::cursor::CursorShape;
    pub use super::runtime::event::ColorScheme;
    pub use super::runtime::event::RuntimeEvent;
    pub use super::widget::events::ChangeEvent;
    pub use super::widget::events::ClickEvent;
    pub use super::widget::events::ScrollEvent;
    #[cfg(feature = "images")]
    pub use super::render::image::{ImageConfig, ImageProtocol, ImageSource, ImageSourceError};
    #[cfg(feature = "images")]
    pub use super::widget::widgets::image::Image;
    pub use super::widget::widgets::input::{Input, InputBindingsFactory};
    pub use super::widget::DirtyImpact;
    pub use super::widget::Layer;
    pub use super::widget::widgets::grid::{Cell, CellMut, Grid, Track};
    pub use super::widget::widgets::list::List;
    pub use super::widget::widgets::list::ListRequestEvent;
    pub use super::widget::widgets::pane::Pane;
    pub use super::widget::widgets::stack::Stack;
    pub use super::widget::widgets::split::Split;
    pub use super::widget::widgets::split::SplitPane;
    pub use super::widget::widgets::split::SplitPaneChild;
    pub use super::widget::widgets::split::SplitPaneContent;
    pub use super::render::style::StyledStr;
    pub use super::render::style::Style;
    pub use super::render::style::Stylize;
    pub use super::render::style::StyledString;
    pub use super::widget::widgets::text::Text;
    pub use super::widget::widgets::text::TextClickEvent;
    pub use super::util::text_overflow::TextOverflow;
    pub use super::render::underline::UnderlineType;
    pub use super::runtime::popup::Placement;
    pub use super::runtime::popup::Popup;
    pub use super::runtime::popup::PopupDismissRequested;
    pub use super::runtime::popup::PopupClosed;
    pub use super::widget::Spacing;
    pub use super::widget::AnyWidget;
    pub use super::widget::DelegateWidget;
    pub use super::widget::Downcastable;
    pub use super::widget::Widget;
    pub use super::widget::WidgetEvent;
    pub use super::widget::WidgetId;
    pub use super::widget::WidgetPath;
    pub use super::widget::Layout;
    pub use super::widget::Constraints;
    pub use super::widget::WidgetMethods;
    pub use super::widget::constrain_child;
    pub use super::widget::flow_child;
    pub use super::widget::measure_child;
    pub use super::widget::WidgetState;
    pub use super::widget::Revelation;

    pub use super::input::modifiers::Modifier;
    pub use super::input::modifiers::Modifiers;


    pub use super::widget::input::InputEvent;
    pub use super::widget::input::InputQueue;
    pub use super::widget::input::InputResult;

    pub use super::input::chord::Chord;
    pub use super::input::key::Key;
    pub use super::input::mouse::MouseButton;
    pub use super::input::trigger::Trigger;

    pub use super::runtime::FocusedMeasure;
    pub use super::runtime::TaskHandle;
    pub use super::runtime::TerminalInfo;
    pub use super::runtime::TuiConfig;

    pub use super::render::GridRenderer;
    pub use super::render::RenderContext;

    pub use super::editor::text_buffer::CursorMethods;
    pub use super::editor::text_buffer::Cursor;
    pub use super::editor::text_buffer::TextBuffer;
    pub use super::editor::text_buffer::TextDocument;
    pub use super::editor::text_buffer::TextLayout;

    pub use crate::widget::scrollbar::ScrollbarInputResult;
    pub use crate::widget::scrollbar::ScrollbarState;
    pub use crate::widget::scrollbar::ScrollbarThumb;
    pub use crate::widget::scrollbar::Scrollbar;
    pub use crate::widget::scrollbar::ScrollbarConfig;
    pub use crate::widget::scrollbar::ScrollbarStyle;
    pub use crate::widget::widgets::tooltip::Tooltip;
    pub use super::runtime::clipboard::Clipboard;
    pub use super::runtime::clipboard::ClipboardItem;
    pub use super::runtime::clipboard::LocalClipboard;

    pub use super::editor::bindings::InputBindings;
    pub use super::editor::Editor;
    pub use super::editor::Affinity;
    pub use super::editor::state::EditorState;
    pub use super::editor::default::DefaultBindings;
    pub use super::editor::emacs::EmacsBindings;
    pub use super::editor::modern::ModernBindings;
    pub use super::editor::vi::{ViBindings, ViMode};

    pub use super::render::style::AnsiStyleParser;
}
