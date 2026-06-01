//! Modern desktop-style editor key bindings.

use crate::prelude::*;
use crate::editor::bindings::{begin_input, EditOpType};
use crate::editor::default::on_input_shared;
use chord_macro::chord;

/// Modern desktop-style editor key bindings.
pub struct ModernBindings<T: TextDocument> {
    last_op: Option<EditOpType>,
    _marker: std::marker::PhantomData<T>,
}

impl<T: TextDocument + 'static> InputBindings<T> for ModernBindings<T> {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn configure_state(&self, state: &mut EditorState<T>) {
        state.inclusive_selection = false;
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
            chord!(Ctrl + z) => state.undo(text),
            chord!(Ctrl + Z) | chord!(Ctrl + y) => state.redo(text),
            chord!(Ctrl + x) => state.cut(text),
            chord!(Ctrl + v) => state.paste(text),
            chord!(Ctrl + a) => state.select_all(text),
            chord!(Ctrl + c) => state.copy(text),
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

impl<T: TextDocument> ModernBindings<T> {
    /// Returns the modern desktop-style bindings.
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
