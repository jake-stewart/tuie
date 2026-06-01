//! Default editor key bindings combining shared chords with Emacs-style controls.

use crate::prelude::*;
use crate::editor::bindings::{begin_input, ibeam_click_pos, EditOpType};
use chord_macro::chord;

pub(crate) fn on_input_shared<T: TextDocument>(
    state: &mut EditorState<T>,
    text: &mut T,
    event: &InputEvent,
) -> bool {
    let click = event.get_click_cycle(3);
    match &event.chord {
        chord!(LeftClick) if click == 1 => {
            let pos = if state.inclusive_selection {
                event.mouse_pos
            } else {
                ibeam_click_pos(event)
            };
            state.click(text, pos);
        }
        chord!(LeftClick) if click == 2 => state.double_click(text, event.mouse_pos),
        chord!(LeftClick) if click == 3 => state.triple_click(text, event.mouse_pos),
        chord!(LeftDrag) if click == 1 => {
            if !state.inclusive_selection && event.mouse_window_subpx.x >= 0 {
                state.drag_ibeam(text, ibeam_click_pos(event));
            } else {
                state.drag(text, event.mouse_pos);
            }
        }
        chord!(LeftDrag) if click == 2 => state.double_click_drag(text, event.mouse_pos),
        chord!(LeftDrag) if click == 3 => state.triple_click_drag(text, event.mouse_pos),
        chord!(Arrow(direction)) => state.move_cursor(text, *direction),
        chord!(Shift + Arrow(direction)) => state.extend_selection(text, *direction),
        chord!(Ctrl + Arrow(direction) | Alt + Arrow(direction)) => {
            match direction.axis() {
                Axis2D::X => state.move_cursor_word(text, direction.screen_sign()),
                Axis2D::Y => state.move_cursor_document_edge(text, direction.screen_sign()),
            }
        }
        chord!(Ctrl + Shift + Arrow(direction) | Alt + Shift + Arrow(direction)) => {
            match direction.axis() {
                Axis2D::X => state.extend_selection_word(text, direction.screen_sign()),
                Axis2D::Y => state.grow_extend_selection_document_edge(text, direction.screen_sign()),
            }
        }
        chord!(Home) => state.move_cursor_line_edge(text, Sign::Negative),
        chord!(End) => state.move_cursor_line_edge(text, Sign::Positive),
        chord!(Shift + Home) => state.extend_selection_line_edge(text, Sign::Negative),
        chord!(Shift + End) => state.extend_selection_line_edge(text, Sign::Positive),
        chord!(Ctrl + Home) => state.move_cursor_document_edge(text, Sign::Negative),
        chord!(Ctrl + End) => state.move_cursor_document_edge(text, Sign::Positive),
        chord!(Ctrl + Shift + Home) => state.extend_selection_document_edge(text, Sign::Negative),
        chord!(Ctrl + Shift + End) => state.extend_selection_document_edge(text, Sign::Positive),
        chord!(Backspace) => state.delete_char(text, Sign::Negative),
        chord!(Delete) => state.delete_char(text, Sign::Positive),
        chord!(Ctrl + Backspace | Alt + Backspace) => state.delete_word(text, Sign::Negative),
        chord!(Ctrl + Delete | Alt + Delete) => state.delete_word(text, Sign::Positive),
        chord!(Enter) => state.insert_char(text, '\n'),
        chord!(Char(c)) => state.insert_char(text, *c),
        chord!(Tab) => state.insert_char(text, '\t'),
        _ => return false,
    }
    true
}

/// Default editor key bindings.
pub struct DefaultBindings<T: TextDocument> {
    last_op: Option<EditOpType>,
    _marker: std::marker::PhantomData<T>,
}

impl<T: TextDocument + 'static> InputBindings<T> for DefaultBindings<T> {
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
            chord!(Ctrl + z) => state.undo(text),
            chord!(Ctrl + Z) => state.redo(text),
            chord!(Ctrl + x) => state.cut(text),
            chord!(Ctrl + v) | chord!(Ctrl + y) => state.paste(text),
            chord!(Ctrl + t) => state.transpose_chars(text),

            chord!(Ctrl + c) | chord!(Alt + w) => state.copy(text),
            chord!(Ctrl + A) => state.select_all(text),
            chord!(Ctrl + f) => state.move_cursor(text, Direction2D::Right),
            chord!(Ctrl + b) => state.move_cursor(text, Direction2D::Left),
            chord!(Ctrl + n) => state.move_cursor(text, Direction2D::Down),
            chord!(Ctrl + p) => state.move_cursor(text, Direction2D::Up),
            chord!(Ctrl + a) => state.move_cursor_line_edge(text, Sign::Negative),
            chord!(Ctrl + e) => state.move_cursor_line_edge(text, Sign::Positive),
            chord!(Alt + f) => state.move_cursor_word(text, Sign::Positive),
            chord!(Alt + b) => state.move_cursor_word(text, Sign::Negative),
            chord!(Ctrl + d) => state.delete_char(text, Sign::Positive),
            chord!(Ctrl + h) => state.delete_char(text, Sign::Negative),
            chord!(Ctrl + w) => state.delete_word(text, Sign::Negative),
            chord!(Alt + d) => state.delete_word(text, Sign::Positive),
            chord!(Ctrl + k) => state.delete_to_line_edge(text, Sign::Positive),
            chord!(Ctrl + u) => state.delete_to_line_edge(text, Sign::Negative),

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

impl<T: TextDocument> DefaultBindings<T> {
    /// Returns the default bindings.
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
