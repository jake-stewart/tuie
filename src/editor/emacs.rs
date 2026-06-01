//! Emacs-style editor key bindings.

use crate::prelude::*;
use crate::editor::bindings::{begin_input, EditOpType};
use crate::editor::default::on_input_shared;
use chord_macro::chord;

/// Emacs-style editor key bindings.
pub struct EmacsBindings<T: TextDocument> {
    last_op: Option<EditOpType>,
    _marker: std::marker::PhantomData<T>,
}

impl<T: TextDocument + 'static> InputBindings<T> for EmacsBindings<T> {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn configure_state(&self, state: &mut EditorState<T>) {
        state.inclusive_selection = true;
    }

    fn on_input(
        &mut self, state: &mut EditorState<T>,
        text: &mut T,
        queue: &mut InputQueue,
    ) -> InputResult {
        let (event, op) = match begin_input(state, text, queue, self.last_op) {
            Ok(v) => v,
            Err(r) => return r,
        };
        match &event.chord {
            chord!(Ctrl + w) => state.cut(text),
            chord!(Ctrl + y) => state.paste(text),
            chord!(Ctrl + t) => state.transpose_chars(text),

            chord!(Ctrl + f) => state.move_cursor(text, Direction2D::Right),
            chord!(Ctrl + b) => state.move_cursor(text, Direction2D::Left),
            chord!(Ctrl + n) => state.move_cursor(text, Direction2D::Down),
            chord!(Ctrl + p) => state.move_cursor(text, Direction2D::Up),
            chord!(Alt + f) => state.move_cursor_word(text, Sign::Positive),
            chord!(Alt + b) => state.move_cursor_word(text, Sign::Negative),
            chord!(Ctrl + a) => state.move_cursor_line_edge(text, Sign::Negative),
            chord!(Ctrl + e) => state.move_cursor_line_edge(text, Sign::Positive),
            chord!(Alt + w) => state.copy(text),
            chord!(Ctrl + k) => state.delete_to_line_edge(text, Sign::Positive),
            chord!(Ctrl + u) => state.delete_to_line_edge(text, Sign::Negative),
            chord!(Ctrl + d) => state.delete_char(text, Sign::Positive),
            chord!(Alt + d) => state.delete_word(text, Sign::Positive),
            chord!(Ctrl + h) => state.delete_char(text, Sign::Negative),
            _ => {
                if !on_input_shared(state, text, &event) {
                    return InputResult::Rejected;
                }
            }
        }
        self.last_op = op;
        InputResult::Handled
    }
}

impl<T: TextDocument> EmacsBindings<T> {
    /// Returns the Emacs bindings.
    pub fn new() -> Box<dyn InputBindings<T>>
    where
        T: 'static,
    {
        Box::new(Self {
            last_op: None,
            _marker: std::marker::PhantomData,
        })
    }
}
