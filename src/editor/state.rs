//! Cursor, selection, and undo history for a text editor.

use crate::prelude::*;
use crate::editor::Affinity;
use crate::editor::char_class::GetCharClass;

struct UndoEntry {
    pos: usize,
    deleted: String,
    inserted: String,
    cursor_before: usize,
    cursor_after: usize,
}

/// Cursor, selection, and undo history for an editor over a [`TextDocument`].
pub struct EditorState<T: TextDocument> {
    /// The active cursor position.
    pub cursor: T::Cursor,
    /// The selection anchor.
    pub anchor: T::Cursor,
    /// Whether selections include the grapheme under the cursor.
    pub inclusive_selection: bool,
    /// Side of a wrap boundary the cursor renders on.
    bias: Sign,
    /// The sticky desired screen column for vertical motion.
    pub preferred_col: usize,
    undo_stack: Vec<UndoEntry>,
    undo_index: usize,
    undo_saved_cursor: usize,
    undo_text_dirty: bool,
    dirty: bool,
}

fn affinity_for_motion(toward: Sign) -> Affinity {
    match toward {
        Sign::Positive => Affinity::End,
        Sign::Negative => Affinity::Start,
    }
}

/// Returns the [`Affinity`] that places the cursor on `side` of a wrap boundary.
fn affinity_for_bias(side: Sign) -> Affinity {
    match side {
        Sign::Positive => Affinity::Start,
        Sign::Negative => Affinity::End,
    }
}

impl<T: TextDocument> EditorState<T> {
    fn redo_inner(&mut self, text: &mut T, to_change_start: bool) {
        self.seal_undo_group();
        if self.undo_index < self.undo_stack.len() - 1 {
            self.undo_index += 1;
            let entry = &self.undo_stack[self.undo_index];
            let pos = entry.pos;
            let inserted = entry.inserted.clone();
            let deleted_len = entry.deleted.len();
            let cursor = if to_change_start {
                entry.cursor_before
            } else {
                entry.cursor_after
            };
            self.dirty = true;
            text.replace_range(pos..pos + deleted_len, &inserted);
            let idx = std::cmp::min(cursor, text.len());
            self.cursor.set_index(text, idx);
            self.anchor = self.cursor.clone();
            self.update_preferred_col(text);
        }
    }

    /// Moves the cursor to the start of the current or next line depending on `sign`.
    fn move_adjacent_line_start(&mut self, text: &T, sign: Sign) {
        match sign {
            Sign::Negative => {
                self.cursor.line_start(text);
            }
            Sign::Positive => {
                self.cursor.next_line_start(text);
            }
        }
    }
}

impl<T: TextDocument> EditorState<T> {
    /// Creates a new state with cursor and anchor at byte offset 0.
    pub fn new(text: &T) -> Self {
        let cursor = text.cursor(0);
        let anchor = text.cursor(0);
        Self {
            cursor,
            anchor,
            inclusive_selection: false,
            bias: Sign::Positive,
            preferred_col: 0,
            undo_stack: vec![UndoEntry {
                pos: 0,
                deleted: String::new(),
                inserted: String::new(),
                cursor_before: 0,
                cursor_after: 0,
            }],
            undo_index: 0,
            undo_saved_cursor: 0,
            undo_text_dirty: false,
            dirty: false,
        }
    }

    /// Returns a cursor positioned at byte offset `index`.
    pub fn cursor_at_index(&self, text: &T, index: usize) -> T::Cursor {
        let mut cursor = self.cursor.clone();
        cursor.set_index(text, index);
        cursor
    }

    /// Returns a cursor positioned at screen position `pos`.
    pub fn cursor_at_pos(&self, text: &T, pos: Vec2<i32>) -> T::Cursor {
        if pos.y < 0 {
            return self.cursor_at_index(text, 0);
        }
        let pos = Vec2::new(pos.x.max(0) as usize, pos.y as usize);
        self.cursor_at_index(text, text.pos_to_index(pos))
    }

    /// Returns the substring between two cursors.
    pub fn slice_cursors(&self, text: &T, start: &T::Cursor, end: &T::Cursor) -> String {
        text.slice(start.get_index()..end.get_index())
    }

    /// Replaces the range between two cursors with `replacement`.
    pub fn replace_text_cursors(
        &mut self, text: &mut T, start: &T::Cursor,
        end: &T::Cursor,
        replacement: &str,
    ) {
        self.replace_range(text, start.get_index()..end.get_index(), replacement);
    }

    /// Returns the side of a wrap boundary the cursor renders on.
    pub fn get_wrap_bias(&self) -> Sign {
        self.bias
    }

    /// Returns whether the selection includes the grapheme under the cursor.
    pub fn is_inclusive_selection(&self) -> bool {
        self.inclusive_selection
    }

    /// Applies `affinity` after a motion, updating the wrap bias and preferred column.
    pub fn update(&mut self, text: &T, affinity: Affinity) {
        if self.cursor.at_eof(text) {
            self.bias = Sign::Negative;
            return;
        }
        match affinity {
            Affinity::Start => self.bias = Sign::Positive,
            Affinity::End => self.bias = Sign::Negative,
            Affinity::Column => return,
        }
        self.preferred_col = self.cursor.get_virtual_pos(text, self.bias).x;
    }

    /// Returns the selection as `(low, high)` ordered cursor pair.
    pub fn get_selection(&self) -> (T::Cursor, T::Cursor) {
        if self.anchor <= self.cursor {
            (self.anchor.clone(), self.cursor.clone())
        } else {
            (self.cursor.clone(), self.anchor.clone())
        }
    }

    /// Returns whether the document has been mutated since the dirty flag was last cleared.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Returns the dirty flag and resets it to false.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::replace(&mut self.dirty, false)
    }

    /// Snapshots the cursor position for the next undo entry.
    pub fn save_cursor_before_edit(&mut self) {
        if !self.undo_text_dirty {
            self.undo_saved_cursor = self.cursor.get_index();
        }
    }

    /// Replaces the byte `range` with `replacement`, coalescing into the current undo entry when contiguous.
    pub fn replace_range(
        &mut self, text: &mut T, range: std::ops::Range<usize>,
        replacement: &str,
    ) {
        let std::ops::Range { start, end } = range;
        self.dirty = true;
        self.save_cursor_before_edit();
        let deleted = text.slice(start..end);
        text.replace_range(start..end, replacement);
        if self.undo_text_dirty {
            let entry = &mut self.undo_stack[self.undo_index];
            let ins_start = entry.pos;
            let ins_end = entry.pos + entry.inserted.len();
            if start == ins_end {
                entry.deleted.push_str(&deleted);
                entry.inserted.push_str(replacement);
                return;
            }
            if start >= ins_start && end <= ins_end {
                let rel_start = start - ins_start;
                let rel_end = end - ins_start;
                let mut new_inserted =
                    String::from(&entry.inserted[..rel_start]);
                new_inserted.push_str(replacement);
                new_inserted.push_str(&entry.inserted[rel_end..]);
                entry.inserted = new_inserted;
                return;
            }
            if end == ins_start {
                entry.pos = start;
                entry.deleted = deleted + &entry.deleted;
                entry.inserted = replacement.to_string() + &entry.inserted;
                return;
            }
            entry.cursor_after = self.cursor.get_index();
            self.undo_saved_cursor = self.cursor.get_index();
        }
        self.undo_stack.truncate(self.undo_index + 1);
        self.undo_stack.push(UndoEntry {
            pos: start,
            deleted,
            inserted: replacement.to_string(),
            cursor_before: self.undo_saved_cursor,
            cursor_after: 0,
        });
        self.undo_index = self.undo_stack.len() - 1;
        self.undo_text_dirty = true;
    }

    /// Closes the current undo group.
    pub fn seal_undo_group(&mut self) {
        if self.undo_text_dirty {
            self.undo_stack[self.undo_index].cursor_after = self.cursor.get_index();
            self.undo_text_dirty = false;
        }
    }

    /// Reverts the most recent edit group.
    pub fn undo(&mut self, text: &mut T) {
        self.seal_undo_group();
        if self.undo_index > 0 {
            let entry = &self.undo_stack[self.undo_index];
            let pos = entry.pos;
            let cursor = entry.cursor_before;
            let deleted = entry.deleted.clone();
            let inserted_len = entry.inserted.len();
            self.dirty = true;
            text.replace_range(pos..pos + inserted_len, &deleted);
            self.undo_index -= 1;
            let idx = std::cmp::min(cursor, text.len());
            self.cursor.set_index(text, idx);
            self.anchor = self.cursor.clone();
            self.update_preferred_col(text);
        }
    }

    /// Reapplies the next undone edit group, leaving the cursor at the end of the edit.
    pub fn redo(&mut self, text: &mut T) {
        self.redo_inner(text, false);
    }

    /// Reapplies the next undone edit group, leaving the cursor at the start of the edit.
    pub fn redo_to_change_start(&mut self, text: &mut T) {
        self.redo_inner(text, true);
    }

    /// Deletes the selection and collapses the cursor to the selection start.
    pub fn delete_selection(&mut self, text: &mut T) {
        let (start, end) = self.get_selection();
        self.replace_range(text, start.get_index()..end.get_index(), "");
        self.cursor.set_index(text, start.get_index());
        self.anchor = self.cursor.clone();
    }

    /// Deletes the byte `range`.
    pub fn delete_range(&mut self, text: &mut T, range: std::ops::Range<usize>) {
        self.replace_range(text, range, "");
    }

    /// Deletes the byte `range`, adjusting for a trailing newline at end of file.
    pub fn delete_lines(&mut self, text: &mut T, range: std::ops::Range<usize>) {
        let std::ops::Range { start, end } = range;
        let len = text.len();
        let clamped = std::cmp::min(end, len);
        let eat_backward = end > len
            && start > 0
            && self.cursor_at_index(text, start - 1).get_char(text) == '\n';

        if eat_backward {
            self.replace_range(text, start - 1..clamped, "");
            self.cursor.set_index(text, start - 1);
        } else {
            self.replace_range(text, start..clamped, "");
            self.cursor.set_index(text, start);
        }
        self.anchor = self.cursor.clone();
        self.update(text, Affinity::End);
    }

    /// Replaces the byte `range` with `replacement`, preserving a trailing newline.
    pub fn replace_lines(
        &mut self, text: &mut T, range: std::ops::Range<usize>,
        replacement: &str,
    ) {
        let std::ops::Range { start, end } = range;
        let len = text.len();
        let clamped = std::cmp::min(end, len);
        let has_trailing_nl = end <= len
            && clamped > start
            && self.cursor_at_index(text, clamped - 1).get_char(text) == '\n';

        if has_trailing_nl {
            self.replace_range(text, start..clamped - 1, replacement);
        } else {
            self.replace_range(text, start..clamped, replacement);
        }
        self.cursor.set_index(text, start);
        self.anchor = self.cursor.clone();
        self.update(text, Affinity::End);
    }

    /// Updates `preferred_col` to the cursor's current screen column.
    pub fn update_preferred_col(&mut self, text: &T) {
        let col = self.cursor.get_virtual_pos(text, self.bias).x;
        self.preferred_col = col;
    }

    /// Inserts a single character, replacing the selection if any.
    pub fn insert_char(&mut self, text: &mut T, c: char) {
        let mut buf = [0u8; 4];
        self.insert_str(text, c.encode_utf8(&mut buf));
    }

    /// Inserts `s`, replacing the selection if any.
    pub fn insert_str(&mut self, text: &mut T, s: &str) {
        let (start, end) = self.get_selection();
        self.replace_range(text, start.get_index()..end.get_index(), s);
        self.cursor.set_index(text, start.get_index() + s.len());
        self.anchor = self.cursor.clone();
        self.update(text, Affinity::End);
    }

    /// Swaps the graphemes either side of the cursor and advances past the pair.
    pub fn transpose_chars(&mut self, text: &mut T) {
        let mut before = self.cursor.clone();
        before.move_grapheme(text, Sign::Negative);
        if before.get_index() == self.cursor.get_index() {
            self.update(text, Affinity::Column);
            return;
        }
        let mut after = self.cursor.clone();
        after.move_grapheme(text, Sign::Positive);
        let left = text.slice(before.get_index()..self.cursor.get_index());
        let right = text.slice(self.cursor.get_index()..after.get_index());
        let swapped = format!("{}{}", right, left);
        self.replace_range(text, before.get_index()..after.get_index(), &swapped);
        self.cursor.set_index(text, after.get_index());
        self.anchor = self.cursor.clone();
        self.update(text, Affinity::End);
    }

    /// Deletes one grapheme in `direction`, or the selection if non-empty.
    pub fn delete_char(&mut self, text: &mut T, direction: Sign) {
        if self.anchor == self.cursor {
            let mut target = self.cursor.clone();
            target.move_grapheme(text, direction);
            let (start, end) = direction.order(self.cursor.get_index(), target.get_index());
            self.delete_range(text, start..end);
            self.cursor.set_index(text, start);
        } else {
            self.delete_selection(text);
        }
        self.anchor = self.cursor.clone();
        let affinity = affinity_for_bias(direction.flip());
        self.update(text, affinity);
    }

    /// Deletes one word in `direction`, or the selection if non-empty.
    pub fn delete_word(&mut self, text: &mut T, direction: Sign) {
        if self.anchor == self.cursor {
            let start = self.cursor.get_index();
            self.cursor.move_word(text, direction);
            let end = self.cursor.get_index();
            let (start, end) = direction.order(start, end);
            self.delete_range(text, start..end);
            self.cursor.set_index(text, start);
        } else {
            self.delete_selection(text);
        }
        self.anchor = self.cursor.clone();
        let affinity = affinity_for_bias(direction.flip());
        self.update(text, affinity);
    }

    /// Moves the cursor up or down one wrapped screen line toward `sign`.
    pub fn move_screen_line(&mut self, text: &T, sign: Sign) {
        let pos = self.cursor.get_virtual_pos(text, self.bias).map(|v| v as i32);
        let mut new_pos = sign.add(pos, Vec2::new(0, 1));
        if new_pos.y < 0 || new_pos.y == pos.y {
            return;
        }
        new_pos.x = self.preferred_col as i32;
        let new_cursor = self.cursor_at_pos(text, new_pos);
        if new_cursor.get_virtual_pos(text, self.bias).y as i32 == pos.y {
            return;
        }
        self.cursor = new_cursor;
    }

    /// Moves the cursor up or down one logical text line toward `sign`.
    pub fn move_text_line(&mut self, text: &T, sign: Sign) {
        let len = text.len();
        let target_line_start = match sign {
            Sign::Negative => {
                let mut probe = self.cursor.clone();
                probe.line_start(text);
                if probe.get_index() == 0 {
                    return;
                }
                probe.prev_line_start(text);
                probe.get_index()
            }
            Sign::Positive => {
                let mut probe = self.cursor.clone();
                probe.next_line_start(text);
                if probe == self.cursor {
                    return;
                }
                if probe.get_index() == len && self.cursor.get_index() == len {
                    return;
                }
                probe.get_index()
            }
        };
        let target_y = self
            .cursor_at_index(text, target_line_start)
            .get_virtual_pos(text, self.bias)
            .y as i32;
        let col = self.preferred_col as i32;
        self.cursor = self.cursor_at_pos(text, Vec2::new(col, target_y));
    }

    /// Moves the cursor to the start or end of the current wrapped screen line toward `sign`.
    pub fn move_screen_line_edge(&mut self, text: &T, sign: Sign) {
        let mut pos = self.cursor.get_virtual_pos(text, self.bias).map(|v| v as i32);
        pos.x = match sign {
            Sign::Positive => i32::MAX,
            Sign::Negative => 0,
        };
        self.cursor = self.cursor_at_pos(text, pos);
    }

    /// Returns the `(top, bottom)` inclusive-exclusive visible row range.
    pub fn get_visible_region(&self, text: &T) -> (i32, i32) {
        if let Some(measure) = tuie::get_focused_measure() {
            let offset = measure.visible_pos.y as i32 - measure.pos.y;
            (offset, offset + measure.visible_size.y as i32)
        } else {
            (0, text.get_visible_size().y as i32)
        }
    }

    /// Places the cursor and anchor at screen position `pos`.
    pub fn click(&mut self, text: &T, pos: Vec2<i32>) {
        self.cursor = self.cursor_at_pos(text, pos);
        self.bias = Sign::Positive;
        self.anchor = self.cursor.clone();
        self.update_preferred_col(text);
    }

    /// Extends the selection to screen position `pos`.
    pub fn drag(&mut self, text: &T, pos: Vec2<i32>) {
        let target = self.cursor_at_pos(text, pos);
        let mut bias =
            Sign::from(&target, &self.anchor).unwrap_or(self.bias);
        if target >= self.anchor {
            if self.anchor > self.cursor {
                self.anchor.prev_grapheme(text);
            }
            self.cursor = target;
            let before = self.cursor.get_index();
            self.cursor.next_grapheme(text);
            if self.cursor.get_index() != before {
                bias = Sign::Negative;
            } else if self.cursor.at_eof(text) {
                bias = Sign::Positive;
            }
        } else {
            if self.anchor <= self.cursor {
                self.anchor.next_grapheme(text);
            }
            self.cursor = target;
        }
        self.bias = bias;
        self.update_preferred_col(text);
    }

    /// Extends the selection to `pos` without snapping the anchor to a grapheme boundary.
    pub fn drag_ibeam(&mut self, text: &T, pos: Vec2<i32>) {
        self.cursor = self.cursor_at_pos(text, pos);
        let bias =
            Sign::from(&self.cursor, &self.anchor).unwrap_or(self.bias);
        self.bias = bias;
        self.update_preferred_col(text);
    }

    /// Selects the word at screen position `pos` by expanding through matching [`CharClass`].
    pub fn double_click(&mut self, text: &T, pos: Vec2<i32>) {
        if text.len() == 0 {
            return;
        }
        let mut start = self.cursor_at_pos(text, pos);
        if start.at_eof(text) {
            start.prev_grapheme(text);
        }
        let class = start.get_char(text).get_class();
        let mut end = start.clone();
        start.scan_while_class(text, Sign::Negative, class);
        end.scan_while_class(text, Sign::Positive, class);
        self.anchor = start;
        self.cursor = end;
        self.bias = Sign::Negative;
        self.update_preferred_col(text);
    }

    /// Extends a word-granularity selection to `pos`.
    pub fn double_click_drag(&mut self, text: &T, pos: Vec2<i32>) {
        if text.len() == 0 {
            return;
        }
        let target = self.cursor_at_pos(text, pos);
        let direction = Sign::from(&self.anchor, &target).unwrap_or(Sign::Positive);
        if direction.cmp(&self.anchor, &self.cursor).is_gt() {
            let mut anchor = self.anchor.clone();
            if direction == Sign::Positive {
                anchor.prev_grapheme(text);
            }
            let class = anchor.get_char(text).get_class();
            anchor.scan_while_class(text, direction.flip(), class);
            self.anchor = anchor;
        }
        let mut cursor = target;
        if cursor.at_eof(text) {
            cursor.prev_grapheme(text);
        }
        let class = cursor.get_char(text).get_class();
        cursor.scan_while_class(text, direction, class);
        self.cursor = cursor;
        self.bias = direction.flip();
    }

    /// Selects the full line at screen position `pos`.
    pub fn triple_click(&mut self, text: &T, pos: Vec2<i32>) {
        if text.len() == 0 {
            return;
        }
        let mut line = self.cursor_at_pos(text, pos);
        line.line_start(text);
        self.anchor = line.clone();
        line.next_line_start(text);
        self.cursor = line;
        self.update_preferred_col(text);
    }

    /// Extends a line-granularity selection to `pos`.
    pub fn triple_click_drag(&mut self, text: &T, pos: Vec2<i32>) {
        if text.len() == 0 {
            return;
        }
        let target = self.cursor_at_pos(text, pos);
        let direction = Sign::from(&self.anchor, &target).unwrap_or(Sign::Positive);
        if direction.cmp(&self.anchor, &self.cursor).is_gt() {
            let mut anchor = self.anchor.clone();
            match direction {
                Sign::Positive => {
                    anchor.prev_grapheme(text);
                    anchor.line_start(text);
                }
                Sign::Negative => {
                    anchor.next_line_start(text);
                }
            }
            self.anchor = anchor;
        }
        let mut cursor = target;
        match direction {
            Sign::Positive => {
                cursor.next_line_start(text);
            }
            Sign::Negative => {
                cursor.line_start(text);
            }
        }
        self.cursor = cursor;
        self.bias = direction;
    }

    /// Steps the cursor one cell in `direction`, collapsing onto the selection edge on a horizontal step.
    pub fn nav(&mut self, text: &T, direction: Direction2D) -> Affinity {
        if direction.axis() == Axis2D::X {
            if self.cursor == self.anchor {
                self.cursor.move_grapheme(text, direction.screen_sign());
            } else {
                self.cursor = direction
                    .screen_sign()
                    .max(self.cursor.clone(), self.anchor.clone());
            }
            affinity_for_motion(direction.screen_sign())
        } else {
            self.move_screen_line(text, direction.screen_sign());
            Affinity::Column
        }
    }

    /// Steps the cursor one cell in `direction` without collapsing the selection.
    pub fn nav_extend(&mut self, text: &T, direction: Direction2D) -> Affinity {
        if direction.axis() == Axis2D::X {
            self.cursor.move_grapheme(text, direction.screen_sign());
            affinity_for_motion(direction.screen_sign())
        } else {
            self.move_screen_line(text, direction.screen_sign());
            Affinity::Column
        }
    }

    /// Moves the cursor by one word in `sign`.
    pub fn word(&mut self, text: &T, sign: Sign) -> Affinity {
        self.cursor.move_word(text, sign);
        affinity_for_motion(sign)
    }

    /// Moves the cursor to the start or end of the current wrapped screen line.
    pub fn screen_line_edge(&mut self, text: &T, sign: Sign) -> Affinity {
        self.move_screen_line_edge(text, sign);
        affinity_for_motion(sign)
    }

    /// Moves the cursor to the start or end of the document.
    pub fn document_edge(&mut self, text: &T, sign: Sign) -> Affinity {
        self.cursor.move_document_edge(text, sign);
        affinity_for_motion(sign)
    }

    /// Returns the selected substring.
    pub fn get_selection_text(&self, text: &T) -> String {
        let (start, end) = self.get_selection();
        text.slice(start.get_index()..end.get_index())
    }

    /// Copies the selection to the clipboard.
    pub fn copy(&mut self, text: &T) {
        if self.anchor != self.cursor {
            tuie::clipboard::write(ClipboardItem::Text(self.get_selection_text(text)));
        }
    }

    /// Copies the selection to the clipboard and deletes it.
    pub fn cut(&mut self, text: &mut T) {
        if self.anchor != self.cursor {
            tuie::clipboard::write(ClipboardItem::Text(self.get_selection_text(text)));
            self.delete_selection(text);
            self.update(text, Affinity::End);
        } else {
            self.update(text, Affinity::Column);
        }
    }

    /// Replaces the selection with the clipboard contents.
    pub fn paste(&mut self, text: &mut T) {
        if let Some(content) = tuie::clipboard::read_string() {
            let (start, end) = self.get_selection();
            let start_idx = start.get_index();
            self.replace_range(text, start_idx..end.get_index(), &content);
            self.cursor.set_index(text, start_idx + content.len());
            self.anchor = self.cursor.clone();
            self.update(text, Affinity::End);
        } else {
            self.update(text, Affinity::Column);
        }
    }

    /// Deletes from the cursor to the adjacent line boundary in `sign`.
    pub fn delete_to_line_edge(&mut self, text: &mut T, sign: Sign) {
        self.move_adjacent_line_start(text, sign);
        self.delete_selection(text);
        self.update(text, Affinity::End);
    }

    /// Selects the entire document.
    pub fn select_all(&mut self, text: &T) {
        self.anchor.document_start();
        self.cursor.document_end(text);
        self.update_preferred_col(text);
    }

    /// Expands both ends of the selection to whole-line boundaries.
    pub fn select_line_range(&mut self, text: &T) {
        let mut anchor = self.anchor.clone();
        let mut cursor = self.cursor.clone();
        if anchor <= cursor {
            anchor.line_start(text);
            cursor.line_start(text);
            cursor.next_line_start(text);
            self.anchor = anchor;
            self.cursor = cursor;
        } else {
            cursor.line_start(text);
            anchor.line_start(text);
            anchor.next_line_start(text);
            self.anchor = anchor;
            self.cursor = cursor;
        }
    }

    /// Extends the cursor to the document edge in `sign`, swapping cursor and anchor when reversing past the anchor.
    pub fn grow_document_edge(&mut self, text: &T, sign: Sign) -> Affinity {
        if self.cursor.get_index() == sign.flip().bound(text.len())
            && self.anchor.get_index() != sign.bound(text.len())
        {
            std::mem::swap(&mut self.cursor, &mut self.anchor);
            self.cursor.move_document_edge(text, sign);
        } else {
            self.cursor.move_document_edge(text, sign);
        }
        affinity_for_motion(sign)
    }

    /// Moves the cursor to `index`, leaving the anchor unchanged.
    pub fn extend_selection_to(&mut self, text: &T, index: usize) {
        self.cursor.set_index(text, index);
    }

    /// Moves the cursor one cell, collapsing any selection toward the leading edge.
    pub fn move_cursor(&mut self, text: &T, direction: Direction2D) {
        let affinity = self.nav(text, direction);
        self.anchor = self.cursor.clone();
        self.update(text, affinity);
    }

    /// Moves the cursor one cell, leaving the anchor in place to grow the selection.
    pub fn extend_selection(&mut self, text: &T, direction: Direction2D) {
        let affinity = self.nav_extend(text, direction);
        self.update(text, affinity);
    }

    /// Moves the cursor by one word and collapses the selection.
    pub fn move_cursor_word(&mut self, text: &T, sign: Sign) {
        let affinity = self.word(text, sign);
        self.anchor = self.cursor.clone();
        self.update(text, affinity);
    }

    /// Extends the selection by one word.
    pub fn extend_selection_word(&mut self, text: &T, sign: Sign) {
        let affinity = self.word(text, sign);
        self.update(text, affinity);
    }

    /// Moves the cursor to the screen line edge and collapses the selection.
    pub fn move_cursor_line_edge(&mut self, text: &T, sign: Sign) {
        let affinity = self.screen_line_edge(text, sign);
        self.anchor = self.cursor.clone();
        self.update(text, affinity);
    }

    /// Extends the selection to the screen line edge.
    pub fn extend_selection_line_edge(&mut self, text: &T, sign: Sign) {
        let affinity = self.screen_line_edge(text, sign);
        self.update(text, affinity);
    }

    /// Moves the cursor to the start or end of the document and collapses the selection.
    pub fn move_cursor_document_edge(&mut self, text: &T, sign: Sign) {
        let affinity = self.document_edge(text, sign);
        self.anchor = self.cursor.clone();
        self.update(text, affinity);
    }

    /// Extends the selection to the start or end of the document.
    pub fn extend_selection_document_edge(&mut self, text: &T, sign: Sign) {
        let affinity = self.document_edge(text, sign);
        self.update(text, affinity);
    }

    /// Extends the selection to the document edge in `sign`, swapping cursor and anchor when reversing past the anchor.
    pub fn grow_extend_selection_document_edge(&mut self, text: &T, sign: Sign) {
        let affinity = self.grow_document_edge(text, sign);
        self.update(text, affinity);
    }

    /// Replaces the entire document content with `s`.
    pub fn replace_all(&mut self, text: &mut T, s: &str) {
        let len = text.len();
        self.replace_range(text, 0..len, s);
        let end = text.len();
        self.cursor.set_index(text, end);
        self.anchor = self.cursor.clone();
        self.update(text, Affinity::End);
    }
}
