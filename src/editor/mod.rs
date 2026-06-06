//! Text editor state and key bindings.

pub mod bindings;
pub mod char_class;
pub mod state;
pub mod default;
pub mod emacs;
pub mod modern;
pub mod text_buffer;
pub mod vi;

use crate::prelude::*;
use crate::editor::state::EditorState;

/// The edge a motion snaps to for wrap bias and preferred column.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Affinity {
    /// The start edge.
    Start,
    /// The trailing edge.
    End,
    /// Vertical motion that preserves the preferred column.
    Column,
}

/// An editor pairing an [`EditorState`] with the [`InputBindings`] that drive it.
pub struct Editor<T: TextDocument> {
    state: EditorState<T>,
    bindings: Box<dyn InputBindings<T>>,
}

impl<T: TextDocument> Editor<T> {
    /// Creates an editor at byte offset 0 driven by `bindings`.
    pub fn new(text: &T, bindings: Box<dyn InputBindings<T>>) -> Self {
        let mut editor = Self {
            state: EditorState::new(text),
            bindings,
        };
        editor.bindings.configure_state(&mut editor.state);
        editor
    }

    /// Returns the installed bindings.
    pub fn get_bindings(&self) -> &dyn InputBindings<T> {
        &*self.bindings
    }

    /// Returns the installed bindings for mutation.
    pub fn get_bindings_mut(&mut self) -> &mut dyn InputBindings<T> {
        &mut *self.bindings
    }

    /// Replaces the installed bindings.
    pub fn set_bindings(&mut self, bindings: Box<dyn InputBindings<T>>) {
        self.bindings = bindings;
        self.bindings.configure_state(&mut self.state);
    }

    /// Dispatches queued input to the bindings.
    pub fn on_input(&mut self, text: &mut T, queue: &mut InputQueue) -> InputResult {
        self.bindings.on_input(&mut self.state, text, queue)
    }

    /// Notifies the bindings that the editor gained focus.
    pub fn on_focus(&mut self, text: &T) {
        self.bindings.on_focus(&mut self.state, text);
    }

    /// Notifies the bindings that the editor lost focus.
    pub fn on_blur(&mut self, text: &T) {
        self.bindings.on_blur(&mut self.state, text);
    }

    /// Returns the cursor shape the bindings want rendered.
    pub fn get_cursor_shape(&self) -> CursorShape {
        self.bindings.get_cursor_shape(&self.state)
    }

    /// Returns the byte offset where the cursor should be drawn.
    pub fn get_cursor_index(&self, text: &T) -> usize {
        self.bindings.get_cursor_index(&self.state, text)
    }

    /// Returns the selected byte range to render.
    pub fn get_selected_range(&self, text: &T) -> std::ops::Range<usize> {
        self.bindings.get_selected_range(&self.state, text)
    }

    /// Read-only access to the active cursor.
    pub fn get_cursor(&self) -> &T::Cursor {
        &self.state.cursor
    }

    /// Read-only access to the selection anchor.
    pub fn get_anchor(&self) -> &T::Cursor {
        &self.state.anchor
    }

    /// Returns the side of a wrap boundary the cursor is on.
    pub fn get_wrap_bias(&self) -> Sign {
        self.state.get_wrap_bias()
    }

    /// Returns the selected substring.
    pub fn get_selection_text(&self, text: &T) -> String {
        self.state.get_selection_text(text)
    }

    /// Returns whether the document has been mutated since the last call, then clears the flag.
    pub fn take_dirty(&mut self) -> bool {
        self.state.take_dirty()
    }

    /// Closes the current undo group so the next edit starts a new entry.
    pub fn seal_undo_group(&mut self) {
        self.state.seal_undo_group();
    }

    /// Collapses the selection by moving the anchor to the cursor.
    pub fn collapse_selection(&mut self) {
        self.state.anchor = self.state.cursor.clone();
    }

    /// Selects the entire document.
    pub fn select_all(&mut self, text: &T) {
        self.state.select_all(text);
    }

    /// Places the cursor and anchor at the given byte offsets.
    pub fn set_selection(&mut self, text: &T, cursor: usize, anchor: usize) {
        self.state.cursor.set_index(text, cursor);
        self.state.anchor.set_index(text, anchor);
        self.state.update_preferred_col(text);
    }

    /// Sets whether the selection includes the grapheme under the cursor.
    pub fn set_inclusive_selection(&mut self, inclusive: bool) {
        self.state.inclusive_selection = inclusive;
    }

    /// Returns whether the selection includes the grapheme under the cursor.
    pub fn is_inclusive_selection(&self) -> bool {
        self.state.inclusive_selection
    }

    /// Replaces the entire document with `s`, moving the cursor to the end.
    pub fn set_content(&mut self, text: &mut T, s: &str) {
        self.state.seal_undo_group();
        self.state.set_content(text, s);
    }

    /// Moves the cursor one cell, collapsing any selection toward the leading edge.
    pub fn move_cursor(&mut self, text: &T, direction: Direction2D) {
        self.state.move_cursor(text, direction);
    }

    /// Moves the cursor one cell, leaving the anchor in place to grow the selection.
    pub fn extend_selection(&mut self, text: &T, direction: Direction2D) {
        self.state.extend_selection(text, direction);
    }

    /// Moves the cursor by one word and collapses the selection.
    pub fn move_cursor_word(&mut self, text: &T, sign: Sign) {
        self.state.move_cursor_word(text, sign);
    }

    /// Extends the selection by one word.
    pub fn extend_selection_word(&mut self, text: &T, sign: Sign) {
        self.state.extend_selection_word(text, sign);
    }

    /// Moves the cursor to the screen line edge and collapses the selection.
    pub fn move_cursor_line_edge(&mut self, text: &T, sign: Sign) {
        self.state.move_cursor_line_edge(text, sign);
    }

    /// Extends the selection to the screen line edge.
    pub fn extend_selection_line_edge(&mut self, text: &T, sign: Sign) {
        self.state.extend_selection_line_edge(text, sign);
    }

    /// Moves the cursor to the start or end of the document and collapses the selection.
    pub fn move_cursor_document_edge(&mut self, text: &T, sign: Sign) {
        self.state.move_cursor_document_edge(text, sign);
    }

    /// Extends the selection to the start or end of the document.
    pub fn extend_selection_document_edge(&mut self, text: &T, sign: Sign) {
        self.state.extend_selection_document_edge(text, sign);
    }

    /// Extends to the document edge, swapping cursor and anchor when reversing past the anchor.
    pub fn grow_extend_selection_document_edge(&mut self, text: &T, sign: Sign) {
        self.state.grow_extend_selection_document_edge(text, sign);
    }

    /// Inserts a single character, replacing the selection if any.
    pub fn insert_char(&mut self, text: &mut T, c: char) {
        self.state.insert_char(text, c);
    }

    /// Inserts `s`, replacing the selection if any.
    pub fn insert_str(&mut self, text: &mut T, s: &str) {
        self.state.insert_str(text, s);
    }

    /// Deletes one grapheme in `direction`, or the selection if non-empty.
    pub fn delete_char(&mut self, text: &mut T, direction: Sign) {
        self.state.delete_char(text, direction);
    }

    /// Deletes one word in `direction`, or the selection if non-empty.
    pub fn delete_word(&mut self, text: &mut T, direction: Sign) {
        self.state.delete_word(text, direction);
    }

    /// Swaps the graphemes either side of the cursor and advances past the pair.
    pub fn transpose_chars(&mut self, text: &mut T) {
        self.state.transpose_chars(text);
    }

    /// Copies the selection to the clipboard.
    pub fn copy(&mut self, text: &T) {
        self.state.copy(text);
    }

    /// Copies the selection to the clipboard and deletes it.
    pub fn cut(&mut self, text: &mut T) {
        self.state.cut(text);
    }

    /// Replaces the selection with the clipboard contents.
    pub fn paste(&mut self, text: &mut T) {
        self.state.paste(text);
    }

    /// Deletes from the cursor to the adjacent line boundary in `sign`.
    pub fn delete_to_line_edge(&mut self, text: &mut T, sign: Sign) {
        self.state.delete_to_line_edge(text, sign);
    }

    /// Deletes the byte `range`, adjusting for a trailing newline at end of file.
    pub fn delete_lines(&mut self, text: &mut T, range: std::ops::Range<usize>) {
        self.state.delete_lines(text, range);
    }

    /// Replaces the byte `range` with `replacement`, preserving a trailing newline.
    pub fn replace_lines(
        &mut self, text: &mut T, range: std::ops::Range<usize>,
        replacement: &str,
    ) {
        self.state.replace_lines(text, range, replacement);
    }

    /// Deletes the byte `range`.
    pub fn delete_range(&mut self, text: &mut T, range: std::ops::Range<usize>) {
        self.state.delete_range(text, range);
    }

    /// Replaces the byte `range` with `s`.
    pub fn replace_range(&mut self, text: &mut T, range: std::ops::Range<usize>, s: &str) {
        self.state.replace_range(text, range, s);
    }

    /// Expands both ends of the selection to whole-line boundaries.
    pub fn select_line_range(&mut self, text: &T) {
        self.state.select_line_range(text);
    }

    /// Moves the cursor to `index`, leaving the anchor unchanged.
    pub fn extend_selection_to(&mut self, text: &T, index: usize) {
        self.state.extend_selection_to(text, index);
    }

    /// Returns whether the document has been mutated since the dirty flag was last cleared.
    pub fn is_dirty(&self) -> bool {
        self.state.is_dirty()
    }

    /// Returns the visible row range.
    pub fn get_visible_range(&self, text: &T) -> std::ops::Range<i32> {
        self.state.get_visible_range(text)
    }
}
