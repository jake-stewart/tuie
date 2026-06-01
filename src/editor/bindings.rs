//! Editor input bindings trait and shared edit-op coalescing helpers.

use crate::prelude::*;
use chord_macro::chord;
use std::any::Any;

pub(crate) fn ibeam_click_pos(event: &InputEvent) -> Vec2<i32> {
    let mut pos = event.mouse_pos;
    if event.mouse_window_subpx.x >= 0 {
        let cell_w = crate::get_terminal_info()
            .and_then(|i| i.cell_px)
            .map(|c| c.x as i32)
            .unwrap_or(1);
        if event.mouse_window_subpx.x >= cell_w / 2 {
            pos.x += 1;
        }
    }
    pos
}

/// Translates input chords into edits on an [`EditorState`].
pub trait InputBindings<T: TextDocument> {
    /// Returns the binding as `&dyn Any`.
    fn as_any(&self) -> &dyn Any;
    /// Returns the binding as `&mut dyn Any`.
    fn as_any_mut(&mut self) -> &mut dyn Any;
    /// Configures the editor state when the binding is installed.
    fn configure_state(&self, _state: &mut EditorState<T>) {}
    /// Consumes input from `queue` and applies matching edits to `text`.
    fn on_input(
        &mut self, state: &mut EditorState<T>,
        text: &mut T,
        queue: &mut InputQueue,
    ) -> InputResult;
    /// Called when the editor gains focus.
    fn on_focus(&mut self, _state: &mut EditorState<T>, _text: &T) {}
    /// Called when the editor loses focus.
    fn on_blur(&mut self, _state: &mut EditorState<T>, _text: &T) {}
    /// Returns the cursor shape for the current state.
    fn get_cursor_shape(&self, state: &EditorState<T>) -> CursorShape {
        if state.inclusive_selection {
            CursorShape::Block
        } else {
            CursorShape::Beam
        }
    }
    /// Returns the byte offset for cursor rendering.
    fn get_cursor_index(&self, state: &EditorState<T>, text: &T) -> usize {
        if state.inclusive_selection
            && state.cursor > state.anchor
            && !(state.cursor.at_eof(text) && state.get_wrap_bias() == Sign::Positive)
        {
            let mut prev = state.cursor.clone();
            prev.prev_grapheme(text);
            prev.get_index()
        } else {
            state.cursor.get_index()
        }
    }
    /// Returns the selected byte range to render.
    fn get_selected_range(&self, state: &EditorState<T>, _text: &T) -> std::ops::Range<usize> {
        let (start, end) = state.get_selection();
        start.get_index()..end.get_index()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditOpType {
    Insert,
    DeleteForward,
    DeleteBackward,
}

pub(crate) fn classify_edit_op(event: &InputEvent) -> Option<EditOpType> {
    match &event.chord {
        chord!(Char(_)) | chord!(Enter) | chord!(Tab) => {
            Some(EditOpType::Insert)
        }
        chord!(Backspace) | chord!(Ctrl + h) => {
            Some(EditOpType::DeleteBackward)
        }
        chord!(Delete) | chord!(Ctrl + d) => Some(EditOpType::DeleteForward),
        _ => None,
    }
}

pub(crate) fn try_handle_paste<T: TextDocument>(
    state: &mut EditorState<T>,
    text: &mut T,
    queue: &mut InputQueue,
) -> bool {
    let Some(event) = queue.peek() else {
        return false;
    };
    let Trigger::Paste(paste) = &event.chord.trigger else {
        return false;
    };
    let paste = paste.clone();
    queue.next();
    state.seal_undo_group();
    state.insert_str(text, &paste);
    true
}

pub(crate) fn seal_if_op_changed<T: TextDocument>(
    state: &mut EditorState<T>,
    last_op: Option<EditOpType>,
    op: Option<EditOpType>,
) {
    if !matches!((last_op, op), (Some(prev), Some(cur)) if prev == cur) {
        state.seal_undo_group();
    }
}

/// Handles paste, pulls the next event, and seals the undo group when the edit-op kind changes.
pub(crate) fn begin_input<'a, T: TextDocument>(
    state: &mut EditorState<T>,
    text: &mut T,
    queue: &mut InputQueue<'a>,
    last_op: Option<EditOpType>,
) -> Result<(&'a InputEvent, Option<EditOpType>), InputResult> {
    if try_handle_paste(state, text, queue) {
        return Err(InputResult::Handled);
    }
    let Some(event) = queue.next() else {
        return Err(InputResult::Rejected);
    };
    let op = classify_edit_op(event);
    seal_if_op_changed(state, last_op, op);
    Ok((event, op))
}
