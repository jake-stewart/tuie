//! Vi-style modal input bindings for an [`EditorState`].

use crate::prelude::*;
use crate::editor::char_class::{CharClass, GetCharClass};
use crate::editor::default::on_input_shared;
use crate::util::flat_lookup::FlatLookup;
use chord_macro::chord;
use std::cell::RefCell;
use unicode_segmentation::UnicodeSegmentation;

/// Moves `cursor` to the next or previous occurrence of `word` in `text`.
fn find_word_occurrence<C: Cursor>(cursor: &mut C, text: &C::Text, word: &str, sign: Sign) -> bool {
    let len = text.len();
    let split = std::cmp::min(cursor.get_index() + 1, len);
    match sign {
        Sign::Positive => {
            let mut probe = cursor.clone();
            probe.set_index(text, split);
            probe.find_str_forward(text, word);
            if probe.matches(text, word) {
                *cursor = probe;
                return true;
            }
            let mut probe = cursor.clone();
            probe.document_start();
            probe.find_str_forward(text, word);
            if probe.matches(text, word) && probe < *cursor {
                *cursor = probe;
                true
            } else {
                false
            }
        }
        Sign::Negative => {
            let mut probe = cursor.clone();
            probe.find_str_backward(text, word);
            if probe.matches(text, word) && probe < *cursor {
                *cursor = probe;
                return true;
            }
            probe.document_end(text);
            probe.find_str_backward(text, word);
            if probe.matches(text, word) && probe.get_index() > split {
                *cursor = probe;
                true
            } else {
                false
            }
        }
    }
}

/// Moves `cursor` to the next occurrence of `ch` on the current line.
fn find_char_on_line<C: Cursor>(cursor: &mut C, text: &C::Text, ch: char, sign: Sign) -> bool {
    let mut probe = cursor.clone();
    probe.move_char(text, sign);
    loop {
        let cur = probe.get_char(text);
        if probe.at_line_end(text) {
            return false;
        }
        if cur == ch {
            *cursor = probe;
            return true;
        }
        probe.move_char(text, sign);
    }
}

/// Moves `cursor` to the bracket matching the next bracket found on the current line.
fn find_matching_bracket<C: Cursor>(cursor: &mut C, text: &C::Text) -> bool {
    let mut probe = cursor.clone();
    let line_end = probe.clone().line_end(text).get_index();
    let mut found_ch = None;
    loop {
        if probe.get_index() >= line_end {
            break;
        }
        let ch = probe.get_char(text);
        if matches!(ch, '(' | ')' | '[' | ']' | '{' | '}') {
            found_ch = Some(ch);
            break;
        }
        probe.next_char(text);
    }
    let ch = match found_ch {
        Some(ch) => ch,
        None => return false,
    };
    let (target, sign) = match ch {
        '(' => (')', Sign::Positive),
        '[' => (']', Sign::Positive),
        '{' => ('}', Sign::Positive),
        ')' => ('(', Sign::Negative),
        ']' => ('[', Sign::Negative),
        '}' => ('{', Sign::Negative),
        _ => return false,
    };
    if sign.is_negative() {
        probe.next_char(text);
    }
    let mut depth: i32 = 0;
    loop {
        let scan_ch = probe.get_char(text);
        if scan_ch == '\0' {
            break;
        }
        if scan_ch == ch {
            depth += 1;
        } else if scan_ch == target {
            depth -= 1;
            if depth == 0 {
                *cursor = probe;
                return true;
            }
        }
        if probe.clone() == *probe.move_char(text, sign) {
            break;
        }
    }
    false
}

/// Editing mode of a [`ViBindings`] instance.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ViMode {
    /// Normal command mode.
    Normal,
    /// Insert mode.
    Insert,
    /// Overtype mode.
    Replace,
    /// Character-wise visual selection mode.
    Visual,
    /// Line-wise visual selection mode.
    VisualLine,
    /// Operator-pending mode awaiting a motion or text object.
    Operator,
}

/// Pending operator applied to the next motion or text object.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ViOperator {
    /// Deletes the operated range.
    Delete,
    /// Yanks the operated range to the clipboard.
    Yank,
    /// Deletes the operated range and enters insert mode.
    Change,
    /// Shifts the operated lines by one tab stop in the given `Sign` direction.
    Indent(Sign),
    /// Converts the operated range to lowercase.
    Lowercase,
    /// Converts the operated range to uppercase.
    Uppercase,
}

struct SharedViState {
    last_command: Vec<InputEvent>,
    last_insert: Vec<InputEvent>,
    last_visual: Option<(ViOperator, usize, bool)>,
    last_find: Option<ViFindState>,
    insert_entry_pos: Option<usize>,
    replaying: bool,
    normal_maps: Vec<(Vec<Chord>, Vec<Chord>)>,
    operator_maps: Vec<(Vec<Chord>, Vec<Chord>)>,
    visual_maps: Vec<(Vec<Chord>, Vec<Chord>)>,
    insert_maps: Vec<(Vec<Chord>, Vec<Chord>)>,
    replace_maps: Vec<(Vec<Chord>, Vec<Chord>)>,
    cursor_shapes: [CursorShape; 6],
}

impl ViMode {
    const fn default_cursor_shape(self) -> CursorShape {
        match self {
            ViMode::Normal | ViMode::Visual | ViMode::VisualLine | ViMode::Operator => CursorShape::Block,
            ViMode::Insert => CursorShape::Beam,
            ViMode::Replace => CursorShape::Underline,
        }
    }

    const fn cursor_shape_idx(self) -> usize {
        match self {
            ViMode::Normal => 0,
            ViMode::Insert => 1,
            ViMode::Replace => 2,
            ViMode::Visual => 3,
            ViMode::VisualLine => 4,
            ViMode::Operator => 5,
        }
    }
}

thread_local! {
    static SHARED_VI_STATE: RefCell<SharedViState> = RefCell::new(SharedViState {
        last_command: Vec::new(),
        last_insert: Vec::new(),
        last_visual: None,
        last_find: None,
        insert_entry_pos: None,
        replaying: false,
        normal_maps: Vec::new(),
        operator_maps: Vec::new(),
        visual_maps: Vec::new(),
        insert_maps: Vec::new(),
        replace_maps: Vec::new(),
        cursor_shapes: [
            ViMode::Normal.default_cursor_shape(),
            ViMode::Insert.default_cursor_shape(),
            ViMode::Replace.default_cursor_shape(),
            ViMode::Visual.default_cursor_shape(),
            ViMode::VisualLine.default_cursor_shape(),
            ViMode::Operator.default_cursor_shape(),
        ],
    });
}

/// Registers a key mapping active in [`ViMode::Normal`].
pub fn map_normal(lhs: Vec<Chord>, rhs: Vec<Chord>) {
    SHARED_VI_STATE.with(|shared| shared.borrow_mut().normal_maps.push((lhs, rhs)));
}

/// Registers a key mapping active in [`ViMode::Operator`].
pub fn map_operator(lhs: Vec<Chord>, rhs: Vec<Chord>) {
    SHARED_VI_STATE.with(|shared| shared.borrow_mut().operator_maps.push((lhs, rhs)));
}

/// Registers a key mapping active in [`ViMode::Visual`] and [`ViMode::VisualLine`].
pub fn map_visual(lhs: Vec<Chord>, rhs: Vec<Chord>) {
    SHARED_VI_STATE.with(|shared| shared.borrow_mut().visual_maps.push((lhs, rhs)));
}

/// Registers a key mapping active in [`ViMode::Insert`].
pub fn map_insert(lhs: Vec<Chord>, rhs: Vec<Chord>) {
    SHARED_VI_STATE.with(|shared| shared.borrow_mut().insert_maps.push((lhs, rhs)));
}

/// Registers a key mapping active in [`ViMode::Replace`].
pub fn map_replace(lhs: Vec<Chord>, rhs: Vec<Chord>) {
    SHARED_VI_STATE.with(|shared| shared.borrow_mut().replace_maps.push((lhs, rhs)));
}

/// Sets the cursor shape used when the editor is in `mode`.
pub fn set_cursor_shape(mode: ViMode, shape: CursorShape) {
    SHARED_VI_STATE.with(|shared| {
        shared.borrow_mut().cursor_shapes[mode.cursor_shape_idx()] = shape;
    });
}

/// Returns the cursor shape configured for `mode`.
pub fn get_cursor_shape(mode: ViMode) -> CursorShape {
    SHARED_VI_STATE.with(|shared| shared.borrow().cursor_shapes[mode.cursor_shape_idx()])
}

#[derive(Clone, Copy)]
enum ViFindKind {
    F,
    T,
}

#[derive(Clone, Copy)]
struct ViFindState {
    kind: ViFindKind,
    sign: Sign,
    ch: char,
}

/// How an operator includes the range a motion covers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Inclusivity {
    /// Range stops one short of the motion endpoint.
    Exclusive,
    /// Range includes the grapheme at the motion endpoint.
    Inclusive,
    /// Range expands to cover whole lines.
    Linewise,
}

/// Vi-style modal input bindings for an [`EditorState`].
pub struct ViBindings<T: TextDocument> {
    mode: ViMode,
    replace_stack: String,
    marks: FlatLookup<u8, usize>,
    recording_insert: Vec<InputEvent>,
    pending_insert: Option<(usize, Option<Sign>)>,
    _marker: std::marker::PhantomData<T>,
}

/// Returns `text` guaranteed to end with a newline.
fn normalize_line_paste(text: &str) -> String {
    if text.ends_with('\n') {
        text.to_string()
    } else {
        format!("{}\n", text)
    }
}

impl<T: TextDocument> ViBindings<T> {
    const TABSTOP: usize = 8;

    fn is_mouse_click_or_drag(event: &InputEvent) -> bool {
        matches!(event.chord.trigger,
            Trigger::MouseDown(MouseButton::Left) | Trigger::MouseDrag(MouseButton::Left)
        )
    }

    fn read_count(queue: &mut InputQueue) -> usize {
        let mut count: usize = 0;
        while let Some(ev) = queue.peek() {
            let chord!(Char(c)) = ev.chord else { break };
            if !c.is_ascii_digit() {
                break;
            }
            let digit = (c as u8 - b'0') as usize;
            if digit == 0 && count == 0 {
                break;
            }
            count = count * 10 + digit;
            queue.next();
        }
        count
    }

    fn is_normal_command_repeatable(queue: &[InputEvent]) -> bool {
        for ev in queue {
            match ev.chord {
                chord!(Char(c)) if c.is_ascii_digit() => continue,
                chord!(u) | chord!(Ctrl + r) | chord!('.') => return false,
                _ => return true,
            }
        }
        true
    }

    fn check_mappings(
        queue: &[InputEvent],
        maps: &[(Vec<Chord>, Vec<Chord>)],
    ) -> Result<Option<Vec<Chord>>, ()> {
        let mut has_prefix = false;
        for (lhs, rhs) in maps {
            if lhs.len() == queue.len()
                && lhs.iter().zip(queue).all(|(l, e)| *l == e.chord)
            {
                return Ok(Some(rhs.clone()));
            }
            if lhs.len() > queue.len()
                && lhs[..queue.len()].iter().zip(queue).all(|(l, e)| *l == e.chord)
            {
                has_prefix = true;
            }
        }
        if has_prefix {
            Ok(None)
        } else {
            Err(())
        }
    }

    fn replaying(&self) -> bool {
        SHARED_VI_STATE.with(|shared| shared.borrow().replaying)
    }

    fn set_mark(&mut self, ch: u8, pos: usize) {
        self.marks.insert(ch, pos);
    }

    fn get_mark(&self, ch: u8) -> Option<usize> {
        self.marks.get(&ch).copied()
    }
}

impl<T: TextDocument + 'static> ViBindings<T> {
    fn line_pos(&self, state: &EditorState<T>, text: &T, line: usize) -> T::Cursor {
        let mut c = state.cursor_at_index(text, 0);
        for _ in 1..line {
            c.next_line_start(text);
        }
        c
    }

    fn scan_n_lines(text: &T, c: &mut T::Cursor, count: usize) {
        for _ in 0..count {
            if c.clone() == *c.linewise_end(text) {
                break;
            }
        }
    }

    fn apply_operator(&mut self, state: &mut EditorState<T>, text: &mut T, op: ViOperator) {
        if state.cursor > state.anchor {
            std::mem::swap(&mut state.cursor, &mut state.anchor);
        }
        match op {
            ViOperator::Delete | ViOperator::Change => {
                state.copy(text);
                state.delete_selection(text);
                if matches!(op, ViOperator::Change) {
                    self.set_mode(state, ViMode::Insert);
                } else {
                    self.clamp_cursor_to_line_end(state, text);
                }
                state.update_preferred_col(text);
            }
            ViOperator::Yank => {
                state.copy(text);
                state.anchor = state.cursor.clone();
                state.update_preferred_col(text);
            }
            ViOperator::Indent(sign) => {
                let from = state.cursor.clone();
                let to = state.anchor.clone();
                state.anchor = state.cursor.clone();
                self.indent_lines(state, text, sign, from, to);
            }
            ViOperator::Lowercase | ViOperator::Uppercase => {
                let from = state.cursor.clone();
                let to = state.anchor.clone();
                let content = state.slice_cursors(text, &from, &to);
                let replaced = if matches!(op, ViOperator::Lowercase) {
                    content.to_lowercase()
                } else {
                    content.to_uppercase()
                };
                state.replace_text_cursors(text, &from, &to, &replaced);
                state.anchor = state.cursor.clone();
                state.update_preferred_col(text);
            }
        }
    }

    fn apply_operator_line_range(&mut self, state: &mut EditorState<T>, text: &mut T, op: ViOperator) {
        if state.cursor > state.anchor {
            std::mem::swap(&mut state.cursor, &mut state.anchor);
        }
        if let ViOperator::Indent(sign) = op {
            let from = state.cursor.clone();
            let to = state.anchor.clone();
            self.indent_lines(state, text, sign, from, to);
            return;
        }
        if matches!(op, ViOperator::Lowercase | ViOperator::Uppercase) {
            self.apply_operator(state, text, op);
            return;
        }
        let clamped_end =
            std::cmp::min(state.anchor.get_index(), text.len());
        let clipboard_text = normalize_line_paste(
            &text.slice(state.cursor.get_index()..clamped_end),
        );
        tuie::clipboard::write(ClipboardItem::Text(clipboard_text));
        let from = state.cursor.clone();
        let to = state.anchor.clone();
        match op {
            ViOperator::Yank => {
                state.anchor = state.cursor.clone();
                state.update_preferred_col(text);
            }
            ViOperator::Delete => {
                state.delete_lines(text, from.get_index()..to.get_index());
                self.clamp_cursor_to_line_end(state, text);
                state.update_preferred_col(text);
            }
            ViOperator::Change => {
                state.replace_lines(text, from.get_index()..to.get_index(), "");
                state.update_preferred_col(text);
                self.set_mode(state, ViMode::Insert);
            }
            _ => {}
        }
    }

    /// Inserts `content` before or after the cursor according to `sign`.
    fn paste_str_at(&mut self, state: &mut EditorState<T>, text: &mut T, content: &str, sign: Sign) {
        if content.contains('\n') {
            let paste_text = normalize_line_paste(content);
            match sign {
                Sign::Positive => {
                    let len = text.len();
                    let mut probe = state.cursor.clone();
                    let line_end = probe.line_end(text).get_index();
                    probe.next_line_start(text);
                    let next_line_start = probe.get_index();
                    let at_eof = next_line_start == len && line_end == len;
                    let insertion = if at_eof {
                        let trimmed = paste_text.strip_suffix('\n').unwrap_or(&paste_text);
                        format!("\n{}", trimmed)
                    } else {
                        paste_text
                    };
                    state.replace_range(text, next_line_start..next_line_start, &insertion);
                    let offset = if at_eof {
                        1
                    } else {
                        0
                    };
                    state.cursor.set_index(text, next_line_start + offset);
                }
                Sign::Negative => {
                    let mut probe = state.cursor.clone();
                    let line_start = probe.line_start(text).get_index();
                    state.replace_range(text, line_start..line_start, &paste_text);
                    state.cursor.set_index(text, line_start);
                }
            }
        } else {
            let start = match sign {
                Sign::Positive => {
                    let mut probe = state.cursor.clone();
                    probe.next_grapheme(text);
                    probe.get_index()
                }
                Sign::Negative => state.cursor.get_index(),
            };
            state.replace_range(text, start..start, content);
            state.cursor.set_index(text, start + content.len());
            state.cursor.prev_grapheme(text);
        }
        state.anchor = state.cursor.clone();
        state.update(text, Affinity::End);
    }

    /// Inserts the clipboard contents before or after the cursor according to `sign`.
    fn paste_at(&mut self, state: &mut EditorState<T>, text: &mut T, sign: Sign) {
        if let Some(content) = tuie::clipboard::read_string() {
            self.paste_str_at(state, text, &content, sign);
        } else {
            state.update(text, Affinity::Column);
        }
    }

    fn apply_operator_line(&mut self, state: &mut EditorState<T>, text: &mut T, op: ViOperator, count: usize) {
        state.save_cursor_before_edit();
        let mut c = state.cursor.clone();
        c.line_start(text);
        state.cursor = c.clone();
        Self::scan_n_lines(text, &mut c, count);
        state.anchor = c;
        self.apply_operator_line_range(state, text, op);
    }

    fn on_empty_line(&self, state: &EditorState<T>, text: &T) -> bool {
        let ch = state.cursor.get_char(text);
        if ch == '\0' {
            let mut c = state.cursor.clone();
            c.line_start(text);
            return c.get_index() == state.cursor.get_index();
        }
        if ch == '\n' {
            let mut c = state.cursor.clone();
            c.prev_char(text);
            return c.get_index() == state.cursor.get_index() || c.get_char(text) == '\n';
        }
        false
    }

    /// Extends the far selection endpoint by one grapheme to make the range exclusive.
    fn extend_selection_inclusive(&mut self, state: &mut EditorState<T>, text: &T) {
        if state.cursor >= state.anchor {
            state.cursor.next_grapheme(text);
        } else {
            state.anchor.next_grapheme(text);
        }
    }

    fn clamp_cursor_to_line_end(&mut self, state: &mut EditorState<T>, text: &T) {
        let len = text.len();
        if len == 0 {
            return;
        }
        if state.cursor.get_index() >= len {
            let mut c = state.cursor.clone();
            c.prev_grapheme(text);
            if c.get_char(text) != '\n' {
                state.cursor = c;
                state.anchor = state.cursor.clone();
            }
        } else if state.cursor.get_char(text) == '\n' {
            let mut c = state.cursor.clone();
            c.prev_grapheme(text);
            if c < state.cursor && c.get_char(text) != '\n' {
                state.cursor = c;
                state.anchor = state.cursor.clone();
            }
        }
    }

    fn step_char(text: &T, c: &mut T::Cursor, sign: Sign) -> bool {
        c.clone() != *c.move_char(text, sign)
    }

    fn text_object_range(&mut self, state: &mut EditorState<T>, text: &mut T, ch: char, inner: bool) -> bool {
        match ch {
            'w' => self.text_object_word(state, text, !inner),
            'W' => self.text_object_big_word(state, text, !inner),
            '(' | ')' | 'b' => self.text_object_pair(state, text, '(', ')', inner),
            '[' | ']' => self.text_object_pair(state, text, '[', ']', inner),
            '{' | '}' | 'B' => self.text_object_pair(state, text, '{', '}', inner),
            '<' | '>' => self.text_object_pair(state, text, '<', '>', inner),
            '"' => self.text_object_quote(state, text, '"', inner),
            '\'' => self.text_object_quote(state, text, '\'', inner),
            '`' => self.text_object_quote(state, text, '`', inner),
            'p' => self.text_object_paragraph(state, text, inner),
            's' => self.text_object_sentence(state, text, !inner),
            _ => false,
        }
    }

    fn text_object_word(&mut self, state: &mut EditorState<T>, text: &mut T, outer: bool) -> bool {
        let sign = Sign::from(&state.anchor, &state.cursor)
            .unwrap_or(Sign::Positive);

        let class = if state.cursor == state.anchor {
            let mut c = state.cursor.clone();
            let cursor_char = c.get_char(text);
            if c.at_eof(text) {
                return false;
            }
            loop {
                if !Self::step_char(text, &mut c, Sign::Negative) {
                    break;
                }
                if c.get_char(text) == '\n' || c.get_char(text).get_class() != cursor_char.get_class() {
                    c.next_char(text);
                    break;
                }
            }
            state.anchor = c;
            cursor_char.get_class()
        } else {
            let mut c = state.cursor.clone();
            c.move_char(text, sign);
            if c == state.cursor {
                return false;
            }
            let next_char = c.get_char(text);
            if c.at_line_end(text) {
                return false;
            }
            state.cursor = c;
            next_char.get_class()
        };

        let mut c = state.cursor.clone();
        match sign {
            Sign::Positive => {
                while !c.at_line_end(text)
                    && c.get_char(text).get_class() == class
                {
                    c.next_char(text);
                }
            }
            Sign::Negative => loop {
                if !Self::step_char(text, &mut c, Sign::Negative) {
                    break;
                }
                if c.get_char(text) == '\n' || c.get_char(text).get_class() != class {
                    c.next_char(text);
                    break;
                }
            },
        }

        if outer {
            match sign {
                Sign::Positive => {
                    if c.at_line_end(text) {
                        state.cursor = c;
                        return true;
                    }
                    let next_class = c.get_char(text).get_class();
                    if class == CharClass::Whitespace
                        || next_class == CharClass::Whitespace
                    {
                        while !c.at_line_end(text)
                            && c.get_char(text).get_class() == next_class
                        {
                            c.next_char(text);
                        }
                    }
                }
                Sign::Negative => {
                    let mut peek = c.clone();
                    if Self::step_char(text, &mut peek, Sign::Negative) {
                        let prev_class = peek.get_char(text).get_class();
                        if class == CharClass::Whitespace
                            || prev_class == CharClass::Whitespace
                        {
                            c = peek;
                            loop {
                                if !Self::step_char(text, &mut c, Sign::Negative) {
                                    break;
                                }
                                if c.get_char(text) == '\n'
                                    || c.get_char(text).get_class() != prev_class
                                {
                                    c.next_char(text);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
        state.cursor = c;
        true
    }

    fn text_object_big_word(&mut self, state: &mut EditorState<T>, text: &mut T, outer: bool) -> bool {
        let sign = Sign::from(&state.anchor, &state.cursor)
            .unwrap_or(Sign::Positive);

        let is_whitespace = if state.cursor == state.anchor {
            let mut c = state.cursor.clone();
            let cursor_char = c.get_char(text);
            if c.at_eof(text) {
                return false;
            }
            loop {
                if !Self::step_char(text, &mut c, Sign::Negative) {
                    break;
                }
                if c.get_char(text) == '\n'
                    || c.get_char(text).is_whitespace() != cursor_char.is_whitespace()
                {
                    c.next_char(text);
                    break;
                }
            }
            state.anchor = c;
            cursor_char.is_whitespace()
        } else {
            let mut c = state.cursor.clone();
            c.move_char(text, sign);
            if c == state.cursor {
                return false;
            }
            let next_char = c.get_char(text);
            if c.at_line_end(text) {
                return false;
            }
            state.cursor = c;
            next_char.is_whitespace()
        };

        let mut c = state.cursor.clone();
        match sign {
            Sign::Positive => {
                while !c.at_line_end(text)
                    && c.get_char(text).is_whitespace() == is_whitespace
                {
                    c.next_char(text);
                }
            }
            Sign::Negative => loop {
                if !Self::step_char(text, &mut c, Sign::Negative) {
                    break;
                }
                if c.get_char(text) == '\n' || c.get_char(text).is_whitespace() != is_whitespace
                {
                    c.next_char(text);
                    break;
                }
            },
        }

        if outer {
            if c.at_line_end(text) {
                state.cursor = c;
                return true;
            }
            match sign {
                Sign::Positive => {
                    while !c.at_line_end(text)
                        && c.get_char(text).is_whitespace() != is_whitespace
                    {
                        c.next_char(text);
                    }
                }
                Sign::Negative => loop {
                    if !Self::step_char(text, &mut c, Sign::Negative) {
                        break;
                    }
                    if c.get_char(text) == '\n'
                        || c.get_char(text).is_whitespace() == is_whitespace
                    {
                        c.next_char(text);
                        break;
                    }
                },
            }
        }
        state.cursor = c;
        true
    }

    fn find_unmatched_open(&mut self, state: &mut EditorState<T>, text: &mut T, open: char, close: char) -> bool {
        let mut depth = 0i32;
        loop {
            if state.cursor.clone() == *state.cursor.prev_char(text) {
                return false;
            }
            let ch = state.cursor.get_char(text);
            if ch == close {
                depth += 1;
            } else if ch == open {
                if depth == 0 {
                    return true;
                }
                depth -= 1;
            }
        }
    }

    fn find_unmatched_close(&mut self, state: &mut EditorState<T>, text: &mut T, open: char, close: char) -> bool {
        let mut depth = 0i32;
        loop {
            let ch = state.cursor.get_char(text);
            if ch == '\0' {
                return false;
            }
            if ch == close {
                if depth == 0 {
                    return true;
                }
                depth -= 1;
            } else if ch == open {
                depth += 1;
            }
            state.cursor.next_char(text);
        }
    }

    fn text_object_pair(
        &mut self, state: &mut EditorState<T>, text: &mut T, open: char,
        close: char,
        inner: bool,
    ) -> bool {
        let lo = std::cmp::min(&state.anchor, &state.cursor).clone();
        let saved = state.cursor.clone();

        if state.cursor == state.anchor
            && state.cursor.get_char(text) == open
        {
            state.cursor.next_char(text);
        } else if state.anchor < state.cursor {
            state.cursor = state.anchor.clone();
        }

        let mut found = None;
        if self.find_unmatched_open(state, text, open, close) {
            let open_cursor = state.cursor.clone();
            state.cursor.next_char(text);
            if self.find_unmatched_close(state, text, open, close) {
                found = Some((open_cursor, state.cursor.clone()));
            }
        }

        if found.is_none() && saved == state.anchor {
            state.cursor = saved.clone();
            let mut end = state.cursor.clone();
            end.line_end(text);
            state.cursor.find_char(text, Sign::Positive, open);
            if state.cursor.get_char(text) == open && state.cursor < end {
                let open_cursor = state.cursor.clone();
                state.cursor.next_char(text);
                if self.find_unmatched_close(state, text, open, close) {
                    found = Some((open_cursor, state.cursor.clone()));
                }
            }
        }

        let Some((mut open_cursor, mut close_cursor)) = found else {
            state.cursor = saved;
            return false;
        };

        if saved != state.anchor {
            let same = if inner {
                let mut after_open = open_cursor.clone();
                after_open.next_char(text);
                after_open == lo
            } else {
                open_cursor == lo
            };
            if same {
                state.cursor = open_cursor.clone();
                if !self.find_unmatched_open(state, text, open, close) {
                    state.cursor = saved;
                    return false;
                }
                open_cursor = state.cursor.clone();
                state.cursor.next_char(text);
                if !self.find_unmatched_close(state, text, open, close) {
                    state.cursor = saved;
                    return false;
                }
                close_cursor = state.cursor.clone();
            }
        }

        if inner {
            state.anchor = open_cursor;
            state.anchor.next_char(text);
            state.cursor = close_cursor;
        } else {
            state.anchor = open_cursor;
            state.cursor = close_cursor;
            state.cursor.next_char(text);
        }
        true
    }

    fn text_object_quote(&mut self, state: &mut EditorState<T>, text: &mut T, quote: char, inner: bool) -> bool {
        let mut c = state.cursor.clone();
        c.line_start(text);
        let line_start = c.clone();
        let line_end = c.clone().line_end(text).clone();

        let mut quote_positions: Vec<T::Cursor> = Vec::new();
        while c < line_end {
            let ch = c.get_char(text);
            if ch == '\0' {
                break;
            }
            if ch == quote {
                let mut probe = c.clone();
                let mut backslash_count = 0usize;
                loop {
                    if probe.clone() == *probe.prev_char(text) || probe.get_char(text) != '\\' {
                        break;
                    }
                    backslash_count += 1;
                }
                if backslash_count % 2 == 0 {
                    quote_positions.push(c.clone());
                }
            }
            c.next_char(text);
        }

        let selection_lo = if state.anchor < state.cursor {
            state.anchor.clone()
        } else {
            state.cursor.clone()
        };

        let mut found = None;
        for pair in quote_positions.chunks_exact(2) {
            if state.cursor >= pair[0] && state.cursor <= pair[1] {
                found = Some((pair[0].clone(), pair[1].clone()));
                break;
            }
        }

        if found.is_none() && state.cursor == state.anchor {
            for pair in quote_positions.chunks_exact(2) {
                if pair[0] >= state.cursor {
                    found = Some((pair[0].clone(), pair[1].clone()));
                    break;
                }
            }
        }

        let Some((open_pos, close_pos)) = found else {
            return false;
        };

        if state.cursor != state.anchor && inner {
            let mut after_open = open_pos.clone();
            after_open.next_char(text);
            if after_open == selection_lo {
                state.anchor = open_pos;
                state.cursor = close_pos;
                state.cursor.next_char(text);
                return true;
            }
        }

        if inner {
            state.anchor = open_pos;
            state.anchor.next_char(text);
            state.cursor = close_pos;
        } else {
            let mut trailing = close_pos.clone();
            trailing.next_char(text);
            if trailing < line_end && Self::is_white(trailing.get_char(text)) {
                while trailing < line_end && Self::is_white(trailing.get_char(text)) {
                    trailing.next_char(text);
                }
                state.anchor = open_pos;
                state.cursor = trailing;
            } else {
                let mut leading = open_pos.clone();
                loop {
                    if leading.clone() == *leading.prev_char(text) || leading < line_start {
                        break;
                    }
                    if !Self::is_white(leading.get_char(text)) {
                        leading.next_char(text);
                        break;
                    }
                }
                state.anchor = leading;
                state.cursor = trailing;
            }
        }
        true
    }

    fn text_object_paragraph(&mut self, state: &mut EditorState<T>, text: &mut T, inner: bool) -> bool {
        let len = text.len();
        if len == 0 {
            return false;
        }
        let mut c = state.cursor.clone();
        if c.get_index() >= len {
            c.prev_char(text);
        }
        c.line_start(text);

        if state.cursor != state.anchor {
            let sign = Sign::from(&state.anchor, &state.cursor)
                .unwrap_or(Sign::Positive);
            let mut prev_is_white: Option<bool> = None;

            for _ in 0..2 {
                let saved = c.clone();
                if !Self::step_line(text, &mut c, len, sign) {
                    break;
                }

                let cur_is_white = Self::is_blank_line(text, &c);
                if prev_is_white == Some(cur_is_white) {
                    c = saved;
                    break;
                }

                loop {
                    let saved = c.clone();
                    if !Self::step_line(text, &mut c, len, sign) {
                        break;
                    }
                    if Self::is_blank_line(text, &c) != cur_is_white {
                        c = saved;
                        break;
                    }
                }

                if inner {
                    break;
                }

                prev_is_white = Some(cur_is_white);
            }

            if sign.is_positive() {
                c.next_line_start(text);
            }
            state.cursor = c;
            return true;
        }

        let started_on_blank_line = Self::is_blank_line(text, &c);

        let mut start_c = c.clone();
        loop {
            let prev = start_c.clone();
            if prev == *start_c.prev_line_start(text) {
                break;
            }
            if Self::is_blank_line(text, &start_c) != started_on_blank_line {
                start_c = prev;
                break;
            }
        }

        if inner {
            let mut e = c.clone();
            while e.clone() != *e.next_line_start(text) {
                if Self::is_blank_line(text, &e) != started_on_blank_line {
                    break;
                }
            }
            state.anchor = start_c;
            state.cursor = e;
            return true;
        }

        if started_on_blank_line {
            let mut e = c.clone();
            while e.clone() != *e.next_line_start(text) {
                if !Self::is_blank_line(text, &e) {
                    break;
                }
            }
            let blank_end = e.get_index();
            if blank_end >= len {
                state.anchor = start_c;
                state.cursor = e;
                return true;
            }
            while e.clone() != *e.next_line_start(text) {
                if Self::is_blank_line(text, &e) {
                    break;
                }
            }
            state.anchor = start_c;
            state.cursor = e;
        } else {
            let mut e = c.clone();
            while e.clone() != *e.next_line_start(text) {
                if Self::is_blank_line(text, &e) {
                    break;
                }
            }
            let mut trailing = e.clone();
            while trailing.get_index() < len && Self::is_blank_line(text, &trailing) {
                trailing.next_line_start(text);
            }
            if trailing != e {
                state.anchor = start_c;
                state.cursor = trailing;
            } else {
                let mut start = start_c;
                loop {
                    let prev = start.clone();
                    if prev == *start.prev_line_start(text) {
                        break;
                    }
                    if !Self::is_blank_line(text, &start) {
                        start = prev;
                        break;
                    }
                }
                state.anchor = start;
                state.cursor = e;
            }
        }
        true
    }

    fn is_white(ch: char) -> bool {
        ch == ' ' || ch == '\t'
    }

    fn line_is_empty(text: &T, c: &T::Cursor) -> bool {
        let mut start = c.clone();
        start.line_start(text);
        start.at_line_end(text)
    }

    fn advance_char(text: &T, c: &mut T::Cursor) -> bool {
        let old = c.clone();
        c.next_char(text);
        *c != old
    }

    fn retreat_char(text: &T, c: &mut T::Cursor) -> bool {
        let old = c.clone();
        c.prev_char(text);
        *c != old
    }

    fn move_to_sentence_boundary(text: &T, pos: &mut T::Cursor, sign: Sign) -> bool {
        let saved_pos = pos.clone();
        let mut ran_off_end = false;

        if pos.at_line_end(text) {
            loop {
                let moved = match sign {
                    Sign::Positive => Self::advance_char(text, pos),
                    Sign::Negative => Self::retreat_char(text, pos),
                };
                if !moved {
                    break;
                }
                if !pos.at_line_end(text) {
                    break;
                }
            }
            if sign == Sign::Positive {
                while Self::is_white(pos.get_char(text)) {
                    if !Self::advance_char(text, pos) {
                        break;
                    }
                }
                if *pos == saved_pos {
                    if sign == Sign::Positive {
                        Self::advance_char(text, pos);
                    } else {
                        Self::retreat_char(text, pos);
                    }
                }
                return true;
            }
        } else if sign == Sign::Negative {
            Self::retreat_char(text, pos);
        }

        let mut found_dot = false;
        loop {
            let ch = pos.get_char(text);
            if ch == '\n' {
                if Self::line_is_empty(text, &*pos) && sign == Sign::Positive {
                    break;
                }
                if !Self::retreat_char(text, pos) {
                    break;
                }
                continue;
            }
            if !Self::is_white(ch)
                && !matches!(ch, '.' | '!' | '?' | ')' | ']' | '"' | '\'')
            {
                break;
            }
            let mut probe = pos.clone();
            if !Self::retreat_char(text, &mut probe) {
                break;
            }
            if Self::line_is_empty(text, &probe) && sign == Sign::Positive {
                break;
            }
            if found_dot {
                break;
            }
            if matches!(ch, '.' | '!' | '?') {
                found_dot = true;
            }
            if matches!(ch, ')' | ']' | '"' | '\'') {
                let probe_ch = probe.get_char(text);
                if !matches!(probe_ch, '.' | '!' | '?' | ')' | ']' | '"' | '\'') {
                    break;
                }
            }
            Self::retreat_char(text, pos);
        }

        let mut start_line = pos.clone();
        start_line.line_start(text);

        loop {
            let ch = pos.get_char(text);
            if ch == '\0' {
                break;
            }
            if ch == '\n' {
                if Self::line_is_empty(text, &*pos) {
                    if sign == Sign::Negative {
                        let mut cur_line = pos.clone();
                        cur_line.line_start(text);
                        if cur_line != start_line {
                            Self::advance_char(text, pos);
                        }
                    }
                    break;
                }
                let moved = match sign {
                    Sign::Positive => Self::advance_char(text, pos),
                    Sign::Negative => Self::retreat_char(text, pos),
                };
                if !moved {
                    ran_off_end = true;
                    break;
                }
                continue;
            }
            if matches!(ch, '.' | '!' | '?') {
                let mut probe = pos.clone();
                loop {
                    if !Self::advance_char(text, &mut probe) {
                        break;
                    }
                    if !matches!(probe.get_char(text), ')' | ']' | '"' | '\'') {
                        break;
                    }
                }
                let probe_ch = probe.get_char(text);
                if probe.at_line_end(text) || Self::is_white(probe_ch) {
                    *pos = probe;
                    if pos.at_line_end(text) {
                        Self::advance_char(text, pos);
                    }
                    break;
                }
            }
            let moved = match sign {
                Sign::Positive => Self::advance_char(text, pos),
                Sign::Negative => Self::retreat_char(text, pos),
            };
            if !moved {
                ran_off_end = true;
                break;
            }
        }

        if !ran_off_end {
            while Self::is_white(pos.get_char(text)) {
                if !Self::advance_char(text, pos) {
                    break;
                }
            }
        }

        if *pos == saved_pos {
            let moved = match sign {
                Sign::Positive => Self::advance_char(text, pos),
                Sign::Negative => Self::retreat_char(text, pos),
            };
            if !moved {
                return false;
            }
            return Self::move_to_sentence_boundary(text, pos, sign);
        }

        true
    }

    fn retreat_to_blank_run_start(text: &T, pos: &mut T::Cursor) {
        loop {
            if !Self::retreat_char(text, pos) {
                break;
            }
            if !Self::is_white(pos.get_char(text)) {
                Self::advance_char(text, pos);
                break;
            }
        }
    }

    fn skip_sentences_forward(
        text: &T,
        pos: &mut T::Cursor,
        mut count: usize,
        mut at_sentence_start: bool,
    ) {
        while count > 0 {
            count -= 1;
            Self::move_to_sentence_boundary(text, pos, Sign::Positive);
            if at_sentence_start {
                Self::retreat_to_blank_run_start(text, pos);
            }
            if count == 0 || at_sentence_start {
                Self::retreat_char(text, pos);
                if pos.get_char(text) == '\n' {
                    Self::retreat_char(text, pos);
                }
            }
            at_sentence_start = !at_sentence_start;
        }
    }

    fn text_object_sentence(&mut self, state: &mut EditorState<T>, text: &mut T, outer: bool) -> bool {
        if text.len() == 0 {
            return false;
        }

        if state.anchor != state.cursor {
            let start_pos = state.cursor.clone();
            Self::move_to_sentence_boundary(text, &mut state.cursor, Sign::Positive);

            let mut pos = start_pos.clone();
            let count = if outer { 2 } else { 1 };

            if start_pos > state.anchor {
                Self::advance_char(text, &mut pos);
                let mut at_sentence_start = true;
                if pos != state.cursor {
                    at_sentence_start = false;
                    while pos < state.cursor {
                        if !Self::is_white(pos.get_char(text)) && !pos.at_line_end(text) {
                            at_sentence_start = true;
                            break;
                        }
                        Self::advance_char(text, &mut pos);
                    }
                    if at_sentence_start {
                        Self::move_to_sentence_boundary(
                            text,
                            &mut state.cursor,
                            Sign::Negative,
                        );
                    } else {
                        state.cursor = start_pos.clone();
                    }
                }

                let mut end = state.cursor.clone();
                Self::skip_sentences_forward(
                    text,
                    &mut end,
                    count,
                    at_sentence_start,
                );
                if !Self::advance_char(text, &mut end) {
                    end.document_end(text);
                }
                state.cursor = end;
            } else {
                let mut at_sentence_start = true;
                Self::retreat_char(text, &mut pos);
                while pos < state.cursor {
                    if !Self::is_white(pos.get_char(text)) && !pos.at_line_end(text) {
                        at_sentence_start = false;
                        break;
                    }
                    Self::advance_char(text, &mut pos);
                }
                if !at_sentence_start {
                    Self::move_to_sentence_boundary(
                        text,
                        &mut state.cursor,
                        Sign::Negative,
                    );
                    if state.cursor == start_pos {
                        at_sentence_start = true;
                    } else {
                        Self::move_to_sentence_boundary(
                            text,
                            &mut state.cursor,
                            Sign::Positive,
                        );
                    }
                } else {
                    state.cursor = start_pos.clone();
                }
                for _ in 0..count {
                    if at_sentence_start {
                        Self::retreat_to_blank_run_start(text, &mut state.cursor);
                    }
                    let ch = state.cursor.get_char(text);
                    if !at_sentence_start
                        || (!outer
                            && !Self::is_white(ch)
                            && !state.cursor.at_line_end(text))
                    {
                        Self::move_to_sentence_boundary(
                            text,
                            &mut state.cursor,
                            Sign::Negative,
                        );
                    }
                    at_sentence_start = !at_sentence_start;
                }
            }
            return true;
        }

        let mut start_pos = state.cursor.clone();
        let mut pos = start_pos.clone();

        let mut next_sent = state.cursor.clone();
        Self::move_to_sentence_boundary(text, &mut next_sent, Sign::Positive);

        let start_blank;
        while Self::is_white(pos.get_char(text)) {
            if !Self::advance_char(text, &mut pos) {
                break;
            }
        }
        if pos == next_sent {
            start_blank = true;
            Self::retreat_to_blank_run_start(text, &mut start_pos);
        } else {
            start_blank = false;
            let mut backward = next_sent.clone();
            Self::move_to_sentence_boundary(text, &mut backward, Sign::Negative);
            start_pos = backward;
        }

        let sentence_count = if outer {
            2
        } else if start_blank {
            0
        } else {
            1
        };

        let mut end_pos = start_pos.clone();
        if sentence_count > 0 {
            Self::skip_sentences_forward(text, &mut end_pos, sentence_count, true);
        } else {
            Self::retreat_char(text, &mut end_pos);
        }

        if outer {
            if start_blank {
                Self::retreat_to_blank_run_start(text, &mut end_pos);
                if Self::is_white(end_pos.get_char(text)) {
                    Self::retreat_char(text, &mut end_pos);
                }
            } else if !Self::is_white(end_pos.get_char(text)) {
                Self::retreat_to_blank_run_start(text, &mut start_pos);
            }
        }

        if !Self::advance_char(text, &mut end_pos) {
            end_pos.document_end(text);
        }

        state.anchor = start_pos;
        state.cursor = end_pos;
        true
    }

    fn find_number_at_or_after_cursor(&mut self, state: &mut EditorState<T>, text: &mut T) -> bool {
        let mut c = state.cursor.clone();
        if state.cursor < *c.clone().line_end(text)
            && c.get_char(text).is_ascii_digit()
        {
            let mut start = state.cursor.clone();
            loop {
                if c.clone() == *c.prev_char(text) || !c.get_char(text).is_ascii_digit() {
                    break;
                }
                start = c.clone();
            }
            if c < start && c.get_char(text) == '-' {
                start = c.clone();
            }
            c = state.cursor.clone();
            c.next_char(text);
            while c.get_char(text).is_ascii_digit() {
                c.next_char(text);
            }
            state.anchor = start;
            state.cursor = c;
            return true;
        }
        let line_end = c.clone().line_end(text).clone();
        c = state.cursor.clone();
        while c < line_end {
            let ch = c.get_char(text);
            if ch.is_ascii_digit() {
                let start = c.clone();
                while c.get_char(text).is_ascii_digit() {
                    c.next_char(text);
                }
                state.anchor = start;
                state.cursor = c;
                return true;
            }
            if ch == '-' {
                let minus = c.clone();
                c.next_char(text);
                if c < line_end && c.get_char(text).is_ascii_digit() {
                    while c.get_char(text).is_ascii_digit() {
                        c.next_char(text);
                    }
                    state.anchor = minus;
                    state.cursor = c;
                    return true;
                }
                continue;
            }
            c.next_char(text);
        }
        false
    }

    fn increment_number(&mut self, state: &mut EditorState<T>, text: &mut T, delta: i64) {
        if self.find_number_at_or_after_cursor(state, text) {
            let num_str = state.slice_cursors(text,
                &state.anchor.clone(),
                &state.cursor.clone(),
            );
            if let Ok(n) = num_str.parse::<i64>() {
                let new_val = n + delta;
                let new_str = new_val.to_string();
                let anchor = state.anchor.clone();
                let cursor = state.cursor.clone();
                state.replace_text_cursors(text, &anchor, &cursor, &new_str);
                state.cursor = state.anchor.clone();
                state
                    .cursor
                    .seek_chars(text, new_str.chars().count() as i64 - 1);
                state.anchor = state.cursor.clone();
                state.update_preferred_col(text);
            }
        }
    }

    fn join_lines(&mut self, state: &mut EditorState<T>, text: &mut T, from: usize, to: usize) {
        let len = text.len();
        let orig_end = std::cmp::min(to, len);
        let end = if orig_end > from {
            let c = state.cursor_at_index(text, orig_end - 1);
            if c.get_char(text) == '\n' {
                let mut c = state.cursor_at_index(text, from);
                c.find_char(text, Sign::Positive, '\n');
                if c.get_char(text) == '\n' && c.get_index() < orig_end - 1 {
                    orig_end - 1
                } else {
                    state.cursor_at_index(text, orig_end).line_end(text).get_index()
                }
            } else {
                orig_end
            }
        } else {
            orig_end
        };

        let mut c = state.cursor_at_index(text, from);
        c.find_char(text, Sign::Positive, '\n');
        if c.get_char(text) != '\n' || c.get_index() >= end {
            return;
        }

        let mut result = String::with_capacity(end - from);
        let mut seg_cursor = state.cursor_at_index(text, from);

        let mut join_pos = None;

        loop {
            let seg_start = seg_cursor.get_index();
            let mut nl = seg_cursor.clone();
            nl.find_char(text, Sign::Positive, '\n');
            let is_last = nl.get_char(text) != '\n' || nl.get_index() >= end;
            let seg_end_idx = if is_last { end } else { nl.get_index() };

            let mut trim_end = if seg_end_idx == nl.get_index() {
                nl.clone()
            } else {
                state.cursor_at_index(text, seg_end_idx)
            };
            loop {
                if !Self::step_char(text, &mut trim_end, Sign::Negative)
                    || trim_end.get_index() < seg_start
                {
                    trim_end = seg_cursor.clone();
                    break;
                }
                if !Self::is_white(trim_end.get_char(text)) {
                    trim_end.next_char(text);
                    break;
                }
            }

            let seg_has_content = trim_end.get_index() > seg_start;
            if !result.is_empty() && seg_has_content {
                result.push(' ');
            }

            if join_pos.is_none() {
                join_pos = Some(from + result.len());
            }

            if seg_has_content {
                for chunk in text.chunks(seg_start..trim_end.get_index())
                {
                    result.push_str(&chunk);
                }
            }

            if is_last {
                break;
            }

            nl.next_char(text);
            while Self::is_white(nl.get_char(text)) {
                nl.next_char(text);
            }
            seg_cursor = nl;
        }

        state.replace_range(text, from..end, &result);
        state.cursor =
            state.cursor_at_index(text, join_pos.unwrap_or(from));
        state.anchor = state.cursor.clone();
        state.update_preferred_col(text);
    }

    fn toggle_case(&mut self, state: &mut EditorState<T>, text: &mut T) {
        let mut c = state.cursor.clone();
        let ch = c.get_char(text);
        c.next_grapheme(text);
        if c == state.cursor || ch == '\n' {
            return;
        }
        let next = c.get_index();
        drop(c);
        let grapheme = text.slice(state.cursor.get_index()..next);
        let mut toggled = String::with_capacity(grapheme.len());
        for c in grapheme.chars() {
            if c.is_uppercase() {
                toggled.extend(c.to_lowercase());
            } else {
                toggled.extend(c.to_uppercase());
            }
        }
        state
            .replace_range(text, state.cursor.get_index()..next, &toggled);
        state.cursor.seek_chars(text, toggled.chars().count() as i64);
        state.anchor = state.cursor.clone();
        state.update_preferred_col(text);
    }

    fn join_line_at_cursor(&mut self, state: &mut EditorState<T>, text: &mut T) {
        let mut c = state.cursor.clone();
        c.line_end(text);
        if c.get_index() >= text.len() {
            return;
        }
        c.next_char(text);
        let end = c.line_end(text).get_index();
        let cursor_index = state.cursor.get_index();
        self.join_lines(state, text, cursor_index, end);
    }

    fn execute_find_char(&mut self, state: &mut EditorState<T>, text: &mut T, find: ViFindState, count: usize) -> bool {
        let mut found = false;
        for _ in 0..count {
            if find_char_on_line(&mut state.cursor, text, find.ch, find.sign) {
                if matches!(find.kind, ViFindKind::T) {
                    state.cursor.move_grapheme(text, find.sign.flip());
                }
                found = true;
            } else {
                break;
            }
        }
        found
    }

    fn parse_find_char_keys(
        &self,
        event: &InputEvent,
        queue: &mut InputQueue,
    ) -> Option<ViFindState> {
        let (kind, sign) = match &event.chord {
            chord!(f) => (ViFindKind::F, Sign::Positive),
            chord!(F) => (ViFindKind::F, Sign::Negative),
            chord!(t) => (ViFindKind::T, Sign::Positive),
            chord!(T) => (ViFindKind::T, Sign::Negative),
            _ => return None,
        };
        let ev2 = queue.next()?;
        if let chord!(Char(ch)) = ev2.chord {
            let find = ViFindState { kind, sign, ch };
            SHARED_VI_STATE.with(|shared| shared.borrow_mut().last_find = Some(find));
            return Some(find);
        }
        None
    }

    fn resolve_find_repeat(&self, event: &InputEvent) -> Option<ViFindState> {
        let mut find = SHARED_VI_STATE.with(|shared| shared.borrow().last_find)?;
        if matches!(event.chord, chord!(',')) {
            find.sign = find.sign.flip();
        }
        Some(find)
    }

    fn parse_mark_target(
        &self, state: &EditorState<T>, text: &T,
        event: &InputEvent,
        queue: &mut InputQueue,
    ) -> Option<(usize, bool)> {
        let linewise = matches!(event.chord, chord!('\''));
        let ev2 = queue.next()?;
        if let chord!(Char(c)) = ev2.chord {
            if c.is_ascii_alphabetic() || c == '^' {
                if let Some(target) =
                    self.resolve_mark_target(state, text, c as u8, linewise)
                {
                    return Some((target, linewise));
                }
            }
        }
        None
    }

    fn motion_char(&mut self, state: &mut EditorState<T>, text: &mut T, count: usize, sign: Sign) {
        for _ in 0..count {
            state.cursor.move_within_line(text, sign);
        }
    }

    fn motion_line(&mut self, state: &mut EditorState<T>, text: &mut T, count: usize, sign: Sign) -> bool {
        let start = state.cursor.clone();
        for _ in 0..count {
            state.move_text_line(text, sign);
        }
        state.cursor != start
    }

    fn motion_screen_line(&mut self, state: &mut EditorState<T>, text: &mut T, count: usize, sign: Sign) {
        for _ in 0..count {
            state.move_screen_line(text, sign);
        }
    }

    fn skip_big_word(&mut self, state: &mut EditorState<T>, text: &mut T, sign: Sign) -> bool {
        let start = state.cursor.clone();
        let mut c = state.cursor.clone();
        loop {
            if sign.is_positive() {
                if c.get_char(text) == '\0' || c.get_char(text).get_class() == CharClass::Whitespace
                {
                    break;
                }
            }
            if c.clone() == *c.move_grapheme(text, sign) {
                break;
            }
            if sign.is_negative() {
                if c.get_char(text).get_class() == CharClass::Whitespace {
                    c.next_grapheme(text);
                    break;
                }
            }
        }
        state.cursor = c;
        state.cursor != start
    }

    fn motion_forward_word(&mut self, state: &mut EditorState<T>, text: &mut T, count: usize, big: bool) {
        for _ in 0..count {
            if big {
                self.skip_big_word(state, text, Sign::Positive);
            } else {
                state.cursor.skip_current_word_class(text, Sign::Positive);
            }
            state.cursor.skip_whitespace(text, Sign::Positive);
        }
    }

    fn motion_backward_word(&mut self, state: &mut EditorState<T>, text: &mut T, count: usize, big: bool) {
        for _ in 0..count {
            if big {
                state.cursor.skip_whitespace(text, Sign::Negative);
                self.skip_big_word(state, text, Sign::Negative);
            } else {
                state.cursor.move_word(text, Sign::Negative);
            }
        }
    }

    fn motion_word_end(&mut self, state: &mut EditorState<T>, text: &mut T, count: usize, sign: Sign, big: bool) {
        for _ in 0..count {
            if sign.is_positive() {
                if state.cursor.at_eof(text) {
                    return;
                }
                state.cursor.move_grapheme(text, Sign::Positive);
                state.cursor.skip_whitespace(text, Sign::Positive);
                let moved = if big {
                    self.skip_big_word(state, text, Sign::Positive)
                } else {
                    state.cursor.skip_current_word_class(text, Sign::Positive)
                };
                if moved {
                    state.cursor.prev_grapheme(text);
                }
            } else {
                if big {
                    self.skip_big_word(state, text, Sign::Negative);
                } else {
                    state.cursor.skip_current_word_class(text, Sign::Negative);
                }
                if state.cursor.skip_whitespace(text, Sign::Negative) {
                    state.cursor.prev_grapheme(text);
                }
            }
        }
    }

    fn motion_screen_top(&mut self, state: &mut EditorState<T>, text: &mut T, count: usize) {
        let (top, _) = state.get_visible_region(text);
        let scrolloff = if top == 0 { 0 } else { tuie::config::get().scrolloff as i32 };
        let y = top + scrolloff + (count - 1) as i32;
        state.cursor = state.cursor_at_pos(text, Vec2::new(0, y));
    }

    fn motion_screen_middle(&mut self, state: &mut EditorState<T>, text: &mut T) {
        let (top, bottom) = state.get_visible_region(text);
        state.cursor =
            state.cursor_at_pos(text, Vec2::new(0, (top + bottom) / 2));
    }

    fn motion_screen_bottom(&mut self, state: &mut EditorState<T>, text: &mut T, count: usize) {
        let (_, bottom) = state.get_visible_region(text);
        let at_end = bottom >= text.get_visible_size().y as i32;
        let scrolloff = if at_end { 0 } else { tuie::config::get().scrolloff as i32 };
        let y = bottom - count as i32 - scrolloff;
        state.cursor = state.cursor_at_pos(text, Vec2::new(0, y));
    }

    fn motion_first_non_blank(&mut self, state: &mut EditorState<T>, text: &mut T) {
        let mut c = state.cursor.clone();
        c.line_start(text);
        while Self::is_white(c.get_char(text)) {
            c.next_char(text);
        }
        state.cursor = c;
    }

    fn motion_paragraph(&mut self, state: &mut EditorState<T>, text: &mut T, count: usize, sign: Sign) {
        for _ in 0..count {
            self.paragraph_boundary(state, text, sign);
        }
    }

    fn motion_sentence(&mut self, state: &mut EditorState<T>, text: &mut T, count: usize, sign: Sign) {
        for _ in 0..count {
            self.sentence_boundary(state, text, sign);
        }
    }

    fn motion_word_search(&mut self, state: &mut EditorState<T>, text: &mut T, count: usize, sign: Sign) {
        if let Some(range) = state.cursor.get_word_under_cursor(text) {
            let word = text.slice(range);
            for _ in 0..count {
                if !find_word_occurrence(&mut state.cursor, text, &word, sign) {
                    break;
                }
            }
        }
    }

    fn step_line(
        text: &T, c: &mut T::Cursor,
        text_len: usize,
        sign: Sign,
    ) -> bool {
        match sign {
            Sign::Positive => {
                let prev = c.clone();
                c.next_line_start(text);
                if c.get_index() >= text_len {
                    *c = prev;
                    false
                } else {
                    true
                }
            }
            Sign::Negative => {
                if c.get_index() == 0 {
                    false
                } else {
                    c.prev_grapheme(text);
                    c.line_start(text);
                    true
                }
            }
        }
    }

    fn is_blank_line(text: &T, c: &T::Cursor) -> bool {
        let mut c = c.clone();
        loop {
            let ch = c.get_char(text);
            if c.at_line_end(text) {
                return true;
            }
            if !Self::is_white(ch) {
                return false;
            }
            c.next_char(text);
        }
    }

    fn paragraph_boundary(&mut self, state: &mut EditorState<T>, text: &mut T, sign: Sign) {
        let text_len = text.len();
        let mut c = state.cursor.clone();
        c.line_start(text);
        while Self::step_line(text, &mut c, text_len, sign) {
            if Self::is_blank_line(text, &c) {
                state.cursor = c;
                return;
            }
        }
        state.cursor.move_document_edge(text, sign);
    }

    fn sentence_boundary(&mut self, state: &mut EditorState<T>, text: &mut T, sign: Sign) {
        let mut pos = state.cursor.clone();
        Self::move_to_sentence_boundary(text, &mut pos, sign);
        state.cursor = pos;
    }

    fn indent_lines(
        &mut self, state: &mut EditorState<T>, text: &mut T, sign: Sign,
        from: T::Cursor,
        to: T::Cursor,
    ) {
        let mut result = String::new();
        let mut line_cursor = from.clone();

        loop {
            let mut nl = line_cursor.clone();
            nl.find_char(text, Sign::Positive, '\n');
            let is_last = nl.get_char(text) != '\n' || nl >= to;
            let line_end = if is_last { to.clone() } else { nl.clone() };

            if line_cursor > from {
                result.push('\n');
            }

            if line_cursor < line_end {
                let mut space_count: usize = 0;
                let mut prefix = String::new();

                if sign.is_positive() {
                    prefix.push('\t');
                }

                let mut c = line_cursor.clone();
                while c < line_end {
                    let ch = c.get_char(text);
                    if ch == '\t' {
                        for _ in 0..space_count {
                            prefix.push(' ');
                        }
                        space_count = 0;
                        prefix.push('\t');
                    } else if ch == ' ' {
                        space_count += 1;
                        if space_count == Self::TABSTOP {
                            prefix.push('\t');
                            space_count = 0;
                        }
                    } else {
                        break;
                    }
                    c.next_char(text);
                }

                for _ in 0..space_count {
                    prefix.push(' ');
                }

                if sign.is_negative() {
                    if prefix.starts_with('\t') {
                        prefix.replace_range(..1, "");
                    } else {
                        let spaces =
                            prefix.len() - prefix.trim_start_matches(' ').len();
                        let remove = std::cmp::min(spaces, Self::TABSTOP);
                        prefix.replace_range(..remove, "");
                    }
                }

                result.push_str(&prefix);

                while c < line_end {
                    result.push(c.get_char(text));
                    c.next_char(text);
                }
            }

            if is_last {
                break;
            }
            nl.next_char(text);
            line_cursor = nl;
        }

        let mut pos_cursor = from.clone();
        if from < to && pos_cursor.get_char(text) == '\n' {
            pos_cursor.next_char(text);
        }
        state.cursor = pos_cursor;
        state.replace_text_cursors(text, &from, &to, &result);
        state.anchor = state.cursor.clone();
        state.update_preferred_col(text);
    }

    fn action_replace_char(&mut self, state: &mut EditorState<T>, text: &mut T, count: usize, c: char) {
        let mut probe = state.cursor.clone();
        for _ in 0..count {
            if probe.get_index() >= text.len() || probe.get_char(text) == '\n' {
                return;
            }
            probe.next_grapheme(text);
        }
        for _ in 0..count {
            state.delete_char(text, Sign::Positive);
            state.insert_char(text, c);
        }
        state.cursor.prev_grapheme(text);
        state.anchor = state.cursor.clone();
        state.update_preferred_col(text);
    }

    fn action_delete_char(&mut self, state: &mut EditorState<T>, text: &mut T, count: usize, sign: Sign) {
        for _ in 0..count {
            self.delete_char_no_newline(state, text, sign);
        }
    }

    fn action_substitute(&mut self, state: &mut EditorState<T>, text: &mut T) {
        state.delete_char(text, Sign::Positive);
        self.set_mode(state, ViMode::Insert);
    }

    fn action_toggle_case(&mut self, state: &mut EditorState<T>, text: &mut T, count: usize) {
        for _ in 0..count {
            self.toggle_case(state, text);
        }
    }

    fn action_resume_insert(&mut self, state: &mut EditorState<T>, text: &mut T) {
        if let Some(pos) = self.get_mark(b'^') {
            let clamped = std::cmp::min(pos, text.len());
            state.cursor.set_index(text, clamped);
            state.anchor = state.cursor.clone();
            state.update_preferred_col(text);
        }
        self.set_mode(state, ViMode::Insert);
    }

    fn replay_pending_insert_count(&mut self, state: &mut EditorState<T>, text: &mut T, recorded: &[InputEvent]) {
        let Some((count, reentry)) = self.pending_insert.take() else {
            return;
        };
        let was_replaying = self.replaying();
        if !was_replaying {
            SHARED_VI_STATE.with(|s| s.borrow_mut().replaying = true);
        }
        for _ in 1..count {
            if let Some(sign) = reentry {
                self.action_open_line(state, text, sign);
            }
            for ev in recorded {
                let mut q = InputQueue::new(std::slice::from_ref(ev), false);
                self.on_input(state, text, &mut q);
            }
        }
        if !was_replaying {
            SHARED_VI_STATE.with(|s| s.borrow_mut().replaying = false);
        }
    }

    fn dispatch_insert_entry(&mut self, state: &mut EditorState<T>, text: &mut T, chord: &Chord) -> Option<Sign> {
        match chord {
            chord!(i) => { self.action_enter_insert(state, text, Sign::Negative); None }
            chord!(a) => { self.action_enter_insert(state, text, Sign::Positive); None }
            chord!(I) => { self.action_enter_insert_line_start(state, text); None }
            chord!(A) => { self.action_enter_insert_line_end(state, text, Sign::Positive); None }
            chord!(o) => { self.action_open_line(state, text, Sign::Positive); Some(Sign::Positive) }
            chord!(O) => { self.action_open_line(state, text, Sign::Negative); Some(Sign::Negative) }
            _ => unreachable!(),
        }
    }

    fn action_enter_insert(&mut self, state: &mut EditorState<T>, text: &mut T, sign: Sign) {
        if sign == Sign::Positive
            && !state.cursor.at_line_end(text)
        {
            state.move_cursor(text, Direction2D::Right);
        }
        self.set_mode(state, ViMode::Insert);
    }

    fn action_enter_insert_line_end(&mut self, state: &mut EditorState<T>, text: &mut T, sign: Sign) {
        state.cursor.move_line_edge(text, sign);
        state.anchor = state.cursor.clone();
        state.update_preferred_col(text);
        self.set_mode(state, ViMode::Insert);
    }

    fn action_enter_insert_line_start(&mut self, state: &mut EditorState<T>, text: &mut T) {
        self.motion_first_non_blank(state, text);
        state.anchor = state.cursor.clone();
        state.update_preferred_col(text);
        self.set_mode(state, ViMode::Insert);
    }

    fn action_open_line(&mut self, state: &mut EditorState<T>, text: &mut T, sign: Sign) {
        match sign {
            Sign::Positive => {
                state.cursor.move_line_edge(text, Sign::Positive);
                state.anchor = state.cursor.clone();
                state.insert_char(text, '\n');
            }
            Sign::Negative => {
                state.cursor.move_line_edge(text, Sign::Negative);
                state.anchor = state.cursor.clone();
                state.insert_char(text, '\n');
                state.cursor.prev_grapheme(text);
                state.anchor = state.cursor.clone();
                state.update_preferred_col(text);
            }
        }
        self.set_mode(state, ViMode::Insert);
    }

    fn action_find_repeat(&mut self, state: &mut EditorState<T>, text: &mut T, event: &InputEvent, count: usize) {
        if let Some(find) = self.resolve_find_repeat(event) {
            self.execute_find_char(state, text, find, count);
            state.anchor = state.cursor.clone();
            state.update_preferred_col(text);
        }
    }

    fn action_cursor_advance(&mut self, state: &mut EditorState<T>, text: &mut T, sign: Sign) {
        state.cursor.move_grapheme(text, sign);
        if sign == Sign::Positive
            && state.cursor.get_index() < text.len()
            && state.cursor.get_char(text) == '\n'
        {
            state.cursor.next_grapheme(text);
        }
        state.anchor = state.cursor.clone();
        state.update_preferred_col(text);
    }

    fn resolve_motion(
        &mut self, state: &mut EditorState<T>, text: &mut T, event: &InputEvent,
        count: usize,
        queue: &mut InputQueue,
    ) -> Option<Result<(Inclusivity, Affinity), ()>> {
        use Inclusivity::*;
        use Affinity::*;
        let result = match &event.chord {
            chord!(h) => {
                self.motion_char(state, text, count, Sign::Negative);
                Ok((Exclusive, Start))
            }
            chord!(l) => {
                self.motion_char(state, text, count, Sign::Positive);
                Ok((Exclusive, Start))
            }
            chord!(j) => {
                if !self.motion_line(state, text, count, Sign::Positive) {
                    return Some(Err(()));
                }
                Ok((Linewise, Column))
            }
            chord!(k) => {
                if !self.motion_line(state, text, count, Sign::Negative) {
                    return Some(Err(()));
                }
                Ok((Linewise, Column))
            }
            chord!(w) => {
                self.motion_forward_word(state, text, count, false);
                Ok((Exclusive, Start))
            }
            chord!(W) => {
                self.motion_forward_word(state, text, count, true);
                Ok((Exclusive, Start))
            }
            chord!(b) => {
                self.motion_backward_word(state, text, count, false);
                Ok((Exclusive, Start))
            }
            chord!(B) => {
                self.motion_backward_word(state, text, count, true);
                Ok((Exclusive, Start))
            }
            chord!(e) => {
                self.motion_word_end(state, text, count, Sign::Positive, false);
                Ok((Inclusive, Start))
            }
            chord!(E) => {
                self.motion_word_end(state, text, count, Sign::Positive, true);
                Ok((Inclusive, Start))
            }
            chord!(H) => {
                self.motion_screen_top(state, text, count);
                Ok((Linewise, Column))
            }
            chord!(M) => {
                self.motion_screen_middle(state, text);
                Ok((Linewise, Column))
            }
            chord!(L) => {
                self.motion_screen_bottom(state, text, count);
                Ok((Linewise, Column))
            }
            chord!(G) => {
                if count > 1 {
                    state.cursor = self.line_pos(state, text, count);
                } else {
                    let len = text.len();
                    let mut c = state.cursor_at_index(text, len);
                    if c.clone() != *c.find_char(text, Sign::Negative, '\n') {
                        c.next_char(text);
                    } else {
                        c.line_start(text);
                    }
                    state.cursor = c;
                }
                Ok((Linewise, Column))
            }
            chord!(0) => {
                state.cursor.move_line_edge(text, Sign::Negative);
                Ok((Exclusive, Start))
            }
            chord!('^') => {
                self.motion_first_non_blank(state, text);
                Ok((Exclusive, Start))
            }
            chord!('_') => {
                if count > 1 {
                    self.motion_line(state, text, count - 1, Sign::Positive);
                }
                self.motion_first_non_blank(state, text);
                Ok((Linewise, Column))
            }
            chord!('$') => {
                state.cursor.move_line_edge(text, Sign::Positive);
                Ok((Inclusive, End))
            }
            chord!('+') | chord!(Enter) => {
                self.motion_line(state, text, count, Sign::Positive);
                self.motion_first_non_blank(state, text);
                Ok((Linewise, Column))
            }
            chord!('-') => {
                self.motion_line(state, text, count, Sign::Negative);
                self.motion_first_non_blank(state, text);
                Ok((Linewise, Column))
            }
            chord!('{') => {
                self.motion_paragraph(state, text, count, Sign::Negative);
                Ok((Linewise, Column))
            }
            chord!('}') => {
                self.motion_paragraph(state, text, count, Sign::Positive);
                Ok((Linewise, Column))
            }
            chord!('(') => {
                self.motion_sentence(state, text, count, Sign::Negative);
                Ok((Exclusive, Start))
            }
            chord!(')') => {
                self.motion_sentence(state, text, count, Sign::Positive);
                Ok((Exclusive, Start))
            }
            chord!('%') => {
                find_matching_bracket(&mut state.cursor, text);
                Ok((Inclusive, Start))
            }
            chord!('*') => {
                self.motion_word_search(state, text, count, Sign::Positive);
                Ok((Exclusive, Start))
            }
            chord!('#') => {
                self.motion_word_search(state, text, count, Sign::Negative);
                Ok((Exclusive, Start))
            }
            chord!(f) | chord!(F) | chord!(t) | chord!(T) => {
                let find = self.parse_find_char_keys(event, queue)?;
                self.execute_find_char(state, text, find, count);
                Ok((Inclusive, Start))
            }
            chord!(';') | chord!(',') => {
                if let Some(find) = self.resolve_find_repeat(event) {
                    self.execute_find_char(state, text, find, count);
                    Ok((Inclusive, Start))
                } else {
                    Err(())
                }
            }
            chord!(g) => {
                let ev2 = queue.next()?;
                match ev2.chord {
                    chord!(g) => {
                        state.cursor = self.line_pos(state, text, count);
                        Ok((Linewise, Column))
                    }
                    chord!(j) => {
                        self.motion_screen_line(state, text, count, Sign::Positive);
                        Ok((Linewise, Column))
                    }
                    chord!(k) => {
                        self.motion_screen_line(state, text, count, Sign::Negative);
                        Ok((Linewise, Column))
                    }
                    chord!(0) => {
                        state.move_screen_line_edge(text, Sign::Negative);
                        Ok((Exclusive, Start))
                    }
                    chord!('$') => {
                        state.move_screen_line_edge(text, Sign::Positive);
                        Ok((Inclusive, End))
                    }
                    chord!(e) => {
                        self.motion_word_end(state, text, count, Sign::Negative, false);
                        Ok((Inclusive, Start))
                    }
                    chord!(E) => {
                        self.motion_word_end(state, text, count, Sign::Negative, true);
                        Ok((Inclusive, Start))
                    }
                    _ => Err(()),
                }
            }
            chord!('`') | chord!('\'') => {
                let (target, linewise) =
                    self.parse_mark_target(state, text, event, queue)?;
                state.cursor.set_index(text, target);
                Ok(if linewise { (Linewise, Column) } else { (Exclusive, Start) })
            }
            chord!(Arrow(direction)) => match direction.axis() {
                Axis2D::X => {
                    self.motion_char(state, text, count, direction.screen_sign());
                    Ok((Exclusive, Start))
                }
                Axis2D::Y => {
                    if !self.motion_line(state, text, count, direction.screen_sign()) {
                        return Some(Err(()));
                    }
                    Ok((Linewise, Column))
                }
            },
            _ => Err(()),
        };
        Some(result)
    }

    fn leave_visual(&mut self, state: &mut EditorState<T>, text: &T) {
        let len = text.len();
        if state.cursor.get_index() > len {
            state.cursor.set_index(text, len);
        }
        if state.anchor.get_index() > len {
            state.anchor.set_index(text, len);
        }
        self.set_mode(state, ViMode::Normal);
    }

    fn escape_to_normal(&mut self, state: &mut EditorState<T>, text: &T) {
        self.set_mode(state, ViMode::Normal);
        if !self.on_empty_line(state, text) {
            let mut c = state.cursor.clone();
            c.prev_grapheme(text);
            if c.get_index() < state.cursor.get_index() && c.get_char(text) != '\n' {
                state.cursor = c;
            }
        }
        state.anchor = state.cursor.clone();
        state.update_preferred_col(text);
    }

    fn operator_to_eol(&mut self, state: &mut EditorState<T>, text: &mut T, op: ViOperator) {
        state.anchor = state.cursor.clone();
        state.cursor.move_line_edge(text, Sign::Positive);
        std::mem::swap(&mut state.cursor, &mut state.anchor);
        self.apply_operator(state, text, op);
    }

    fn delete_char_no_newline(&mut self, state: &mut EditorState<T>, text: &mut T, sign: Sign) {
        let mut c = state.cursor.clone();
        c.move_grapheme(text, sign);
        if c == state.cursor {
            return;
        }
        if (sign.is_negative() && c.get_char(text) == '\n')
            || (sign.is_positive() && state.cursor.get_char(text) == '\n')
        {
            return;
        }
        state.anchor = c;
        self.apply_operator(state, text, ViOperator::Delete);
    }

    fn visual_selection_op(&mut self, state: &mut EditorState<T>, text: &mut T, op: ViOperator, linewise: bool) {
        if state.cursor > state.anchor {
            std::mem::swap(&mut state.cursor, &mut state.anchor);
        }
        if linewise {
            state.cursor.line_start(text);
            state.anchor.linewise_end(text);
            if !self.replaying() && !matches!(op, ViOperator::Yank) {
                let mut c = state.cursor.clone();
                let end = std::cmp::min(
                    state.anchor.get_index(),
                    text.len(),
                );
                let mut lines = 1usize;
                loop {
                    let prev = c.get_index();
                    c.next_line_start(text);
                    if c.get_index() == prev || c.get_index() >= end {
                        break;
                    }
                    lines += 1;
                }
                if matches!(op, ViOperator::Change) {
                    self.recording_insert.clear();
                }
                SHARED_VI_STATE.with(|shared| {
                    let shared = &mut *shared.borrow_mut();
                    if !matches!(op, ViOperator::Change) {
                        shared.last_insert.clear();
                    }
                    shared.last_visual = Some((op, lines, true));
                });
            }
            self.apply_operator_line_range(state, text, op);
            self.leave_visual(state, text);
            self.clamp_cursor_to_line_end(state, text);
        } else {
            state.anchor.next_grapheme(text);
            if !self.replaying() && !matches!(op, ViOperator::Yank) {
                let extent =
                    state.anchor.get_index() - state.cursor.get_index();
                if matches!(op, ViOperator::Change) {
                    self.recording_insert.clear();
                }
                SHARED_VI_STATE.with(|shared| {
                    let shared = &mut *shared.borrow_mut();
                    if !matches!(op, ViOperator::Change) {
                        shared.last_insert.clear();
                    }
                    shared.last_visual = Some((op, extent, false));
                });
            }
            self.leave_visual(state, text);
            self.apply_operator(state, text, op);
        }
    }

    fn expand_to_line_range(&mut self, state: &mut EditorState<T>, text: &mut T) {
        if state.cursor > state.anchor {
            std::mem::swap(&mut state.cursor, &mut state.anchor);
        }
        state.cursor.line_start(text);
        state.anchor.linewise_end(text);
    }

    fn save_last(&self, command: Vec<InputEvent>) {
        SHARED_VI_STATE.with(|shared| {
            let shared = &mut *shared.borrow_mut();
            shared.last_command = command;
            shared.last_insert.clear();
            shared.last_visual = None;
        });
    }

    fn finish_insert(&mut self, state: &mut EditorState<T>, text: &mut T) {
        let recorded = self.recording_insert.clone();

        if !self.replaying() {
            SHARED_VI_STATE.with(|shared| {
                shared.borrow_mut().last_insert = recorded.clone();
            });
        }

        self.replay_pending_insert_count(state, text, &recorded);
        self.escape_to_normal(state, text);
    }

    fn visual_op(&self, event: &InputEvent) -> ViOperator {
        match &event.chord {
            chord!(d) | chord!(x) | chord!(X) | chord!(D) => ViOperator::Delete,
            chord!(y) | chord!(Y) => ViOperator::Yank,
            chord!(c) | chord!(s) | chord!(C) | chord!(S) => ViOperator::Change,
            chord!('>') => ViOperator::Indent(Sign::Positive),
            chord!('<') => ViOperator::Indent(Sign::Negative),
            chord!(u) => ViOperator::Lowercase,
            chord!(U) => ViOperator::Uppercase,
            _ => unreachable!(),
        }
    }

    fn resolve_mark_target(&self, state: &EditorState<T>, text: &T, ch: u8, linewise: bool) -> Option<usize> {
        let mut target =
            std::cmp::min(self.get_mark(ch)?, text.len());
        if linewise {
            target = state.cursor_at_index(text, target).line_start(text).get_index();
        }
        Some(target)
    }

    fn parse_normal_queue(&mut self, state: &mut EditorState<T>, text: &mut T, queue: &mut InputQueue) -> bool {
        state.anchor = state.cursor.clone();
        self.parse_normal_queue_inner(state, text, queue)
    }

    fn parse_normal_queue_inner(
        &mut self, state: &mut EditorState<T>, text: &mut T, queue: &mut InputQueue,
    ) -> bool {
        let count = std::cmp::max(Self::read_count(queue), 1);
        let Some(event) = queue.next() else { return false; };

        match &event.chord {
            chord!(u) | chord!(Ctrl + r) => {
                state.seal_undo_group();
                let undo = matches!(event.chord, chord!(u));
                for _ in 0..count {
                    if undo {
                        state.undo(text);
                    } else {
                        state.redo_to_change_start(text);
                    }
                }
                self.clamp_cursor_to_line_end(state, text);
                return true;
            }
            chord!('.') => {
                state.seal_undo_group();
                for _ in 0..count {
                    self.replay_dot(state, text);
                }
                self.clamp_cursor_to_line_end(state, text);
                return true;
            }
            _ => {}
        }

        state.seal_undo_group();

        match &event.chord {
            chord!(r) => {
                let Some(ev2) = queue.next() else { return false; };
                if let chord!(Char(c)) = ev2.chord {
                    self.action_replace_char(state, text, count, c);
                }
                return true;
            }
            chord!(m) => {
                let Some(ev2) = queue.next() else { return false; };
                if let chord!(Char(c)) = ev2.chord {
                    if c.is_ascii_alphabetic() {
                        self.set_mark(c as u8, state.cursor.get_index());
                    }
                }
                return true;
            }
            chord!(g)
                if queue.peek().map_or(false, |e| {
                    matches!(e.chord, chord!(u) | chord!(U))
                }) =>
            {
                let Some(ev2) = queue.next() else { return false; };
                let op = if matches!(ev2.chord, chord!(u)) {
                    ViOperator::Lowercase
                } else {
                    ViOperator::Uppercase
                };
                return self.parse_operator(state, text, op, count, queue);
            }
            chord!(g)
                if queue.peek().map_or(false, |e| matches!(e.chord, chord!(i))) =>
            {
                let _ = queue.next();
                self.action_resume_insert(state, text);
                return true;
            }
            chord!(d) => {
                return self.parse_operator(state, text, ViOperator::Delete, count, queue)
            }
            chord!(y) => {
                return self.parse_operator(state, text, ViOperator::Yank, count, queue)
            }
            chord!(c) => {
                return self.parse_operator(state, text, ViOperator::Change, count, queue)
            }
            chord!('>') => {
                return self.parse_operator(state, text,
                    ViOperator::Indent(Sign::Positive),
                    count,
                    queue,
                )
            }
            chord!('<') => {
                return self.parse_operator(state, text,
                    ViOperator::Indent(Sign::Negative),
                    count,
                    queue,
                )
            }
            _ => {}
        }

        match self.resolve_motion(state, text, &event, count, queue) {
            Some(Ok((_, affinity))) => {
                self.clamp_cursor_to_line_end(state, text);
                state.anchor = state.cursor.clone();
                state.update(text, affinity);
                return true;
            }
            None => return false,
            Some(Err(())) => {}
        }
        if Self::is_mouse_click_or_drag(&event) {
            return self.handle_normal_mouse(state, text, &event);
        }
        match &event.chord {
            chord!(D) => self.operator_to_eol(state, text, ViOperator::Delete),
            chord!(C) => self.operator_to_eol(state, text, ViOperator::Change),
            chord!(Y) => self.operator_to_eol(state, text, ViOperator::Yank),
            chord!(x) => self.action_delete_char(state, text, count, Sign::Positive),
            chord!(X) => self.action_delete_char(state, text, count, Sign::Negative),
            chord!(s) => self.action_substitute(state, text),
            chord!(S) => self.apply_operator_line(state, text, ViOperator::Change, 1),
            chord!(J) => self.join_line_at_cursor(state, text),
            chord!('~') => self.action_toggle_case(state, text, count),
            chord!(Ctrl + a) => self.increment_number(state, text, count as i64),
            chord!(Ctrl + x) => self.increment_number(state, text, -(count as i64)),
            chord!(i) | chord!(a) | chord!(I) | chord!(A) | chord!(o) | chord!(O) => {
                let reentry = self.dispatch_insert_entry(state, text, &event.chord);
                self.pending_insert = (count > 1).then_some((count, reentry));
            }
            chord!(R) => self.set_mode(state, ViMode::Replace),
            chord!(v) => self.set_mode(state, ViMode::Visual),
            chord!(V) => self.set_mode(state, ViMode::VisualLine),
            chord!(';') | chord!(',') => self.action_find_repeat(state, text, &event, count),
            chord!(p) => self.paste_at(state, text, Sign::Positive),
            chord!(P) => self.paste_at(state, text, Sign::Negative),
            chord!(Backspace) => self.action_cursor_advance(state, text, Sign::Negative),
            chord!(Space) => self.action_cursor_advance(state, text, Sign::Positive),
            chord!(Esc) => tuie::dirty_paint(),
            _ => return true,
        }
        if matches!(self.mode, ViMode::Normal | ViMode::Operator) {
            self.clamp_cursor_to_line_end(state, text);
        }
        true
    }

    fn parse_operator(
        &mut self, state: &mut EditorState<T>, text: &mut T, op: ViOperator,
        op_count: usize,
        queue: &mut InputQueue,
    ) -> bool {
        let motion_count = Self::read_count(queue);

        let remaining = queue.get_remaining();
        match Self::check_mode_mappings(ViMode::Operator, remaining) {
            Ok(Some(action)) => {
                for _ in 0..remaining.len() {
                    queue.next();
                }
                let expanded: Vec<InputEvent> =
                    action.into_iter().map(InputEvent::from_chord).collect();
                let mut eq = InputQueue::new(&expanded, false);
                return self.parse_operator_inner(state, text,
                    op,
                    op_count,
                    motion_count,
                    &mut eq,
                );
            }
            Ok(None) => return false,
            Err(()) => {}
        }

        self.parse_operator_inner(state, text, op, op_count, motion_count, queue)
    }

    fn parse_operator_inner(
        &mut self, state: &mut EditorState<T>, text: &mut T, op: ViOperator,
        op_count: usize,
        motion_count: usize,
        queue: &mut InputQueue,
    ) -> bool {
        let Some(event) = queue.next() else { return false; };
        let count = std::cmp::max(op_count, 1) * std::cmp::max(motion_count, 1);
        let is_repeat = match op {
            ViOperator::Delete => matches!(event.chord, chord!(d)),
            ViOperator::Yank => matches!(event.chord, chord!(y)),
            ViOperator::Change => matches!(event.chord, chord!(c)),
            ViOperator::Indent(Sign::Positive) => {
                matches!(event.chord, chord!('>'))
            }
            ViOperator::Indent(Sign::Negative) => {
                matches!(event.chord, chord!('<'))
            }
            ViOperator::Lowercase => matches!(event.chord, chord!(u)),
            ViOperator::Uppercase => matches!(event.chord, chord!(U)),
        };
        if is_repeat {
            self.apply_operator_line(state, text, op, count);
            self.complete_op(state, text);
            return true;
        }

        if matches!(event.chord, chord!(i) | chord!(a)) {
            let inner = matches!(event.chord, chord!(i));
            let Some(ev2) = queue.next() else { return false; };
            if let chord!(Char(c)) = ev2.chord {
                if self.text_object_range(state, text, c, inner) {
                    if c == 'p' {
                        let len = text.len();
                        let at_eof_blank = state.cursor.get_index() == len
                            && state.anchor.get_index() > 0
                            && {
                                let mut prev = state.anchor.clone();
                                prev.prev_char(text);
                                prev.get_char(text) == '\n'
                            }
                            && {
                                let mut sc = state.anchor.clone();
                                while sc < state.cursor
                                    && matches!(sc.get_char(text), '\n' | ' ' | '\t')
                                {
                                    sc.next_char(text);
                                }
                                sc >= state.cursor
                            };
                        if at_eof_blank {
                            if matches!(op, ViOperator::Change) {
                                self.apply_operator(state, text, op);
                            } else {
                                state.anchor.prev_char(text);
                                self.apply_operator_line_range(state, text, op);
                            }
                        } else {
                            self.apply_operator_line_range(state, text, op);
                        }
                    } else {
                        self.apply_operator(state, text, op);
                    }
                    self.complete_op(state, text);
                    return true;
                }
            }
            return true;
        }

        if matches!(op, ViOperator::Change)
            && matches!(event.chord, chord!(w) | chord!(W))
        {
            let on_word = state
                .cursor
                .get_char_class_at(text, Sign::Positive)
                .map_or(false, |c| c != CharClass::Whitespace);
            if on_word {
                let saved = state.cursor.clone();
                let big = matches!(event.chord, chord!(W));
                for _ in 0..count {
                    if big {
                        self.skip_big_word(state, text, Sign::Positive);
                    } else {
                        state.cursor.skip_current_word_class(text, Sign::Positive);
                    }
                }
                state.anchor = state.cursor.clone();
                state.cursor = saved;
                self.apply_operator(state, text, ViOperator::Change);
                self.complete_op(state, text);
                return true;
            }
        }

        let start = state.cursor.clone();
        match self.resolve_motion(state, text, &event, count, queue) {
            Some(Ok((mt, _))) => {
                if matches!(mt, Inclusivity::Inclusive) {
                    state.cursor.next_grapheme(text);
                }
                state.anchor = start;
                if let ViOperator::Indent(sign) = op {
                    self.expand_to_line_range(state, text);
                    let from = state.cursor.clone();
                    let to = state.anchor.clone();
                    self.indent_lines(state, text, sign, from, to);
                } else if matches!(mt, Inclusivity::Linewise) {
                    self.expand_to_line_range(state, text);
                    self.apply_operator_line_range(state, text, op);
                } else {
                    self.apply_operator(state, text, op);
                }
                self.complete_op(state, text);
                true
            }
            None => false,
            Some(Err(())) => true,
        }
    }

    fn complete_op(&mut self, state: &mut EditorState<T>, text: &mut T) {
        if self.mode != ViMode::Insert {
            self.clamp_cursor_to_line_end(state, text);
        }
    }

    fn exit_visual_to_normal(&mut self, state: &mut EditorState<T>, text: &T) {
        self.leave_visual(state, text);
        state.anchor = state.cursor.clone();
        self.clamp_cursor_to_line_end(state, text);
    }

    fn visual_action_toggle_visual(&mut self, state: &mut EditorState<T>, text: &mut T, linewise: bool) {
        if linewise {
            self.set_mode(state, ViMode::Visual);
        } else {
            self.exit_visual_to_normal(state, text);
            self.clamp_cursor_to_line_end(state, text);
        }
        state.update_preferred_col(text);
    }

    fn visual_action_toggle_visual_line(&mut self, state: &mut EditorState<T>, text: &mut T, linewise: bool) {
        if linewise {
            self.leave_visual(state, text);
            state
                .move_screen_line_edge(text, Sign::Negative);
            state.anchor = state.cursor.clone();
        } else {
            self.set_mode(state, ViMode::VisualLine);
        }
        state.update_preferred_col(text);
    }

    fn visual_action_swap_anchor(&mut self, state: &mut EditorState<T>, text: &mut T) {
        std::mem::swap(&mut state.anchor, &mut state.cursor);
        state.update_preferred_col(text);
    }

    fn visual_action_join(&mut self, state: &mut EditorState<T>, text: &mut T) {
        self.extend_selection_inclusive(state, text);
        let (from, to) = state.get_selection();
        self.leave_visual(state, text);
        self.join_lines(state, text, from.get_index(), to.get_index());
    }

    fn visual_action_paste(&mut self, state: &mut EditorState<T>, text: &mut T) {
        self.extend_selection_inclusive(state, text);
        self.leave_visual(state, text);
        state.delete_selection(text);
        state.paste(text);
        self.clamp_cursor_to_line_end(state, text);
    }

    fn visual_action_click(&mut self, state: &mut EditorState<T>, text: &mut T, event: &InputEvent) {
        self.leave_visual(state, text);
        state.click(text, event.mouse_pos);
        self.clamp_cursor_to_line_end(state, text);
    }

    fn visual_text_object_linewise(&mut self, state: &mut EditorState<T>, text: &mut T, c: char, inner: bool) -> bool {
        let saved_anchor = state.anchor.clone();
        let saved_cursor = state.cursor.clone();
        let sign =
            Sign::from(&saved_anchor, &saved_cursor).unwrap_or(Sign::Positive);
        state.select_line_range(text);
        let (sel_start, sel_end) = if state.anchor <= state.cursor {
            (state.anchor.clone(), state.cursor.clone())
        } else {
            (state.cursor.clone(), state.anchor.clone())
        };
        state.anchor = saved_anchor.clone();
        state.cursor = saved_cursor.clone();
        state.anchor = state.cursor.clone();
        if self.text_object_range(state, text, c, inner) {
            let (mut obj_anchor, mut obj_cursor) =
                (state.anchor.clone(), state.cursor.clone());
            state.anchor = saved_anchor.clone();
            state.cursor = saved_cursor.clone();
            if c == 'p' {
                let obj_lo = if obj_anchor < obj_cursor {
                    &obj_anchor
                } else {
                    &obj_cursor
                };
                let obj_hi = if obj_anchor < obj_cursor {
                    &obj_cursor
                } else {
                    &obj_anchor
                };
                if *obj_lo >= sel_start && *obj_hi <= sel_end {
                    state.cursor = if sign.is_positive() {
                        sel_end.clone()
                    } else {
                        let mut prev_cursor = sel_start.clone();
                        prev_cursor.prev_char(text);
                        prev_cursor
                    };
                    state.anchor = state.cursor.clone();
                    if self.text_object_range(state, text, c, inner) {
                        obj_anchor = state.anchor.clone();
                        obj_cursor = state.cursor.clone();
                    }
                    state.anchor = saved_anchor.clone();
                    state.cursor = saved_cursor.clone();
                }
                let (obj_lo, obj_hi) = if obj_anchor <= obj_cursor {
                    (obj_anchor, obj_cursor)
                } else {
                    (obj_cursor, obj_anchor)
                };
                let new_start = if sel_start < obj_lo {
                    sel_start
                } else {
                    obj_lo
                };
                let mut new_end =
                    if sel_end > obj_hi { sel_end } else { obj_hi };
                if new_end > new_start {
                    new_end.prev_char(text);
                }
                if sign.is_positive() {
                    state.anchor = new_start;
                    state.cursor = new_end;
                } else {
                    state.anchor = new_end;
                    state.cursor = new_start;
                }
            } else {
                self.set_mode(state, ViMode::Visual);
                state.anchor = obj_anchor;
                state.cursor = obj_cursor;
            }
            state.update_preferred_col(text);
            return true;
        }
        false
    }

    fn visual_text_object_charwise(&mut self, state: &mut EditorState<T>, text: &mut T, c: char, inner: bool) -> bool {
        let saved_anchor = state.anchor.clone();
        let saved_cursor = state.cursor.clone();
        let sign =
            Sign::from(&saved_anchor, &saved_cursor).unwrap_or(Sign::Positive);
        let has_selection = saved_anchor != saved_cursor;
        let has_internal_extension = matches!(c, 'w' | 'W' | 's');

        if matches!(c, 'w' | 'W') && !has_selection {
            let ch = state.cursor.get_char(text);
            if ch != '\0' && ch.is_whitespace() {
                match sign {
                    Sign::Positive => {
                        while state.cursor.get_char(text) != '\0'
                            && state.cursor.get_char(text).is_whitespace()
                        {
                            state.cursor.next_char(text);
                        }
                    }
                    Sign::Negative => loop {
                        if state.cursor.clone()
                            == *state.cursor.prev_char(text)
                        {
                            break;
                        }
                        if !state.cursor.get_char(text).is_whitespace() {
                            state.cursor.next_char(text);
                            break;
                        }
                    },
                }
            }
        }

        if !(has_internal_extension && has_selection) {
            state.anchor = state.cursor.clone();
        }
        if !self.text_object_range(state, text, c, inner) {
            state.anchor = saved_anchor;
            state.cursor = saved_cursor;
            return false;
        }

        let mut obj_anchor = state.anchor.clone();
        let mut obj_cursor = state.cursor.clone();

        state.anchor = saved_anchor.clone();
        state.cursor = saved_cursor.clone();

        if !(has_internal_extension && has_selection) {
            let (sel_start, sel_end) = {
                let (s, e) = state.get_selection();
                (s.get_index(), e.get_index())
            };
            if obj_anchor.get_index() >= sel_start && obj_cursor.get_index() <= sel_end
            {
                let mut skip_cursor = if sign.is_positive() {
                    let mut sc = obj_cursor.clone();
                    sc.next_grapheme(text);
                    sc
                } else {
                    let mut sc = obj_anchor.clone();
                    sc.prev_grapheme(text);
                    sc
                };
                if sign.is_negative() && skip_cursor.get_index() > 0 {
                    while skip_cursor.get_index() > 0
                        && matches!(
                            skip_cursor.get_char(text),
                            ' ' | '\t' | '"' | '\'' | ')' | ']'
                        )
                    {
                        skip_cursor.prev_char(text);
                    }
                    if skip_cursor.get_index() > 0
                        && matches!(skip_cursor.get_char(text), '.' | '!' | '?')
                    {
                        skip_cursor.prev_char(text);
                    }
                }
                state.cursor = skip_cursor;
                state.anchor = state.cursor.clone();
                if self.text_object_range(state, text, c, inner) {
                    obj_anchor = state.anchor.clone();
                    obj_cursor = state.cursor.clone();
                }
                state.anchor = saved_anchor.clone();
                state.cursor = saved_cursor.clone();
            }
        }

        if c == 'p' {
            self.set_mode(state, ViMode::VisualLine);
        }

        let (cur_lo, cur_hi) = if state.anchor <= state.cursor {
            (state.anchor.clone(), state.cursor.clone())
        } else {
            (state.cursor.clone(), state.anchor.clone())
        };
        let (obj_lo, mut obj_hi) = if obj_anchor <= obj_cursor {
            (obj_anchor, obj_cursor)
        } else {
            (obj_cursor, obj_anchor)
        };
        if obj_hi > obj_lo {
            obj_hi.prev_grapheme(text);
        }
        let new_start = if cur_lo < obj_lo { cur_lo } else { obj_lo };
        let new_end = if cur_hi > obj_hi { cur_hi } else { obj_hi };
        if state.anchor <= state.cursor {
            state.anchor = new_start;
            state.cursor = new_end;
        } else {
            state.anchor = new_end;
            state.cursor = new_start;
        }
        state.update_preferred_col(text);
        true
    }

    fn parse_visual_queue(&mut self, state: &mut EditorState<T>, text: &mut T, queue: &mut InputQueue) -> bool {
        let linewise = self.mode == ViMode::VisualLine;
        let Some(event) = queue.peek() else { return false; };

        state.seal_undo_group();

        let click = event.get_click_cycle(3);
        let action_taken = match &event.chord {
            chord!(Esc) => { self.exit_visual_to_normal(state, text); true }
            chord!(v) => { self.visual_action_toggle_visual(state, text, linewise); true }
            chord!(V) => { self.visual_action_toggle_visual_line(state, text, linewise); true }
            chord!(o) => { self.visual_action_swap_anchor(state, text); true }
            chord!(J) => { self.visual_action_join(state, text); true }
            chord!(d)
            | chord!(x)
            | chord!(X)
            | chord!(D)
            | chord!(y)
            | chord!(Y)
            | chord!(c)
            | chord!(s)
            | chord!(C)
            | chord!(S)
            | chord!('>')
            | chord!('<')
            | chord!(u)
            | chord!(U) => {
                let op = self.visual_op(&event);
                let op_linewise =
                    linewise || matches!(op, ViOperator::Indent(_));
                self.visual_selection_op(state, text, op, op_linewise);
                true
            }
            chord!(p) => { self.visual_action_paste(state, text); true }
            chord!(LeftClick) if click == 1 => { self.visual_action_click(state, text, &event); true }
            chord!(LeftClick) if click == 2 => { state.double_click(text, event.mouse_pos); true }
            chord!(LeftClick) if click == 3 => {
                self.set_mode(state, ViMode::VisualLine);
                state.click(text, event.mouse_pos);
                true
            }
            chord!(LeftDrag) if click == 1 => { state.drag(text, event.mouse_pos); true }
            chord!(LeftDrag) if click == 2 => { state.double_click_drag(text, event.mouse_pos); true }
            chord!(LeftDrag) if click == 3 => { self.triple_click_drag_visual_line(state, text, &event); true }
            _ => false,
        };
        if action_taken {
            queue.next();
            return true;
        }

        let count = std::cmp::max(Self::read_count(queue), 1);
        let Some(event) = queue.next() else { return false; };
        let old_anchor = state.anchor.clone();
        let old_cursor = state.cursor.clone();

        match &event.chord {
            chord!(i) | chord!(a) => {
                let inner = matches!(event.chord, chord!(i));
                let Some(ev2) = queue.next() else { return false; };
                if let chord!(Char(c)) = ev2.chord {
                    let found = if linewise {
                        self.visual_text_object_linewise(state, text, c, inner)
                    } else {
                        self.visual_text_object_charwise(state, text, c, inner)
                    };
                    if found {
                        return true;
                    }
                }
                state.anchor = old_anchor;
                state.cursor = old_cursor;
                return true;
            }
            _ => {}
        }

        let handled = match self.resolve_motion(state, text, &event, count, queue) {
            Some(Ok((mt, _))) => Some(mt),
            None => {
                state.anchor = old_anchor;
                state.cursor = old_cursor;
                return false;
            }
            Some(Err(())) => None,
        };
        let dest = state.cursor.clone();
        state.anchor = old_anchor;
        state.cursor = old_cursor;
        if let Some(mt) = handled {
            state.cursor = dest;
            if !matches!(mt, Inclusivity::Linewise) {
                state.update_preferred_col(text);
            }
        }
        true
    }

    fn triple_click_drag_visual_line(&mut self, state: &mut EditorState<T>, text: &mut T, event: &InputEvent) {
        self.set_mode(state, ViMode::VisualLine);
        state.triple_click_drag(text, event.mouse_pos);
        if state.cursor > state.anchor {
            state.cursor.prev_grapheme(text);
        } else {
            state.anchor.prev_grapheme(text);
        }
    }

    fn handle_normal_mouse(&mut self, state: &mut EditorState<T>, text: &mut T, event: &InputEvent) -> bool {
        let click = event.get_click_cycle(3);
        match &event.chord {
            chord!(LeftClick) if click == 1 => state.click(text, event.mouse_pos),
            chord!(LeftClick) if click == 2 => state.double_click(text, event.mouse_pos),
            chord!(LeftClick) if click == 3 => {
                self.set_mode(state, ViMode::VisualLine);
                state.click(text, event.mouse_pos);
            }
            chord!(LeftDrag) if click == 1 => state.drag(text, event.mouse_pos),
            chord!(LeftDrag) if click == 2 => state.double_click_drag(text, event.mouse_pos),
            chord!(LeftDrag) if click == 3 => self.triple_click_drag_visual_line(state, text, event),
            _ => return false,
        }
        if state.anchor != state.cursor {
            if self.mode != ViMode::VisualLine {
                self.set_mode(state, ViMode::Visual);
            }
        } else {
            self.clamp_cursor_to_line_end(state, text);
        }
        true
    }

    fn replay_dot(&mut self, state: &mut EditorState<T>, text: &mut T) {
        let (visual, commands, inserts) = SHARED_VI_STATE.with(|shared| {
            let shared = shared.borrow();
            (
                shared.last_visual,
                shared.last_command.clone(),
                shared.last_insert.clone(),
            )
        });
        SHARED_VI_STATE.with(|shared| shared.borrow_mut().replaying = true);
        if let Some((op, extent, linewise)) = visual {
            if linewise {
                let mut c = state.cursor.clone();
                c.line_start(text);
                state.cursor = c.clone();
                Self::scan_n_lines(text, &mut c, extent);
                state.anchor = c;
                self.apply_operator(state, text, op);
            } else {
                state.anchor = state.cursor.clone();
                for _ in 0..extent {
                    state.anchor.next_char(text);
                }
                self.apply_operator(state, text, op);
            }
        } else if !commands.is_empty() {
            let mut q = InputQueue::new(&commands, false);
            self.parse_normal_queue_inner(state, text, &mut q);
        }
        for ev in &inserts {
            let mut q = InputQueue::new(std::slice::from_ref(ev), false);
            self.on_input(state, text, &mut q);
        }
        self.replay_pending_insert_count(state, text, &inserts);
        if self.mode != ViMode::Normal {
            self.escape_to_normal(state, text);
        }
        SHARED_VI_STATE.with(|shared| shared.borrow_mut().replaying = false);
    }

    fn record_insert_key(&mut self, event: &InputEvent) {
        if !self.replaying() && !matches!(event.chord, chord!(Esc)) {
            self.recording_insert.push(event.clone());
        }
    }

    fn vi_kill_backwards(&mut self, state: &mut EditorState<T>, text: &mut T) {
        let c = state.cursor.get_index();
        let mut probe = state.cursor.clone();
        probe.line_start(text);
        let ls = probe.get_index();
        loop {
            let ch = probe.get_char(text);
            if ch == '\n' || ch.get_class() != CharClass::Whitespace {
                break;
            }
            let prev = probe.clone();
            probe.next_grapheme(text);
            if probe == prev {
                break;
            }
        }
        let ind = probe.get_index();
        let entry = SHARED_VI_STATE
            .with(|s| s.borrow().insert_entry_pos)
            .filter(|&e| e >= ls && e < c);
        let start = if let Some(e) = entry {
            e
        } else if c > ind {
            ind
        } else if c > ls {
            ls
        } else if c > 0 {
            let mut p = state.cursor.clone();
            p.prev_grapheme(text);
            p.get_index()
        } else {
            c
        };
        if start < c {
            state.delete_range(text, start..c);
            state.cursor.set_index(text, start);
            state.anchor = state.cursor.clone();
            state.update_preferred_col(text);
        }
    }

    fn on_input_insert_mode(&mut self, state: &mut EditorState<T>, text: &mut T, event: &InputEvent) -> bool {
        self.record_insert_key(event);
        match &event.chord {
            chord!(Esc) => self.finish_insert(state, text),
            chord!(Ctrl + h) => state.delete_char(text, Sign::Negative),
            chord!(Ctrl + w) => state.delete_word(text, Sign::Negative),
            chord!(Ctrl + u) => self.vi_kill_backwards(state, text),
            _ => return on_input_shared(state, text, event),
        }
        true
    }

    fn on_input_replace_mode(&mut self, state: &mut EditorState<T>, text: &mut T, event: &InputEvent) -> bool {
        self.record_insert_key(event);
        match &event.chord {
            chord!(Esc) => self.finish_insert(state, text),
            chord!(Backspace) => {
                if let Some(grapheme) =
                    self.replace_stack.graphemes(true).next_back()
                {
                    let grapheme = grapheme.to_string();
                    self.replace_stack
                        .truncate(self.replace_stack.len() - grapheme.len());
                    let end = state.cursor.get_index();
                    state.cursor.prev_grapheme(text);
                    let replacement =
                        if grapheme == "\0" { "" } else { &grapheme };
                    state.replace_range(text,
                        state.cursor.get_index()..end,
                        replacement,
                    );
                    state.anchor = state.cursor.clone();
                    state.update_preferred_col(text);
                } else {
                    state.move_cursor(text, Direction2D::Left);
                }
            }
            chord!(Char(c)) => {
                let at_end = state.cursor.get_index() >= text.len()
                    || state.cursor.get_char(text) == '\n';
                let next = state.cursor.clone().next_grapheme(text).get_index();
                if at_end {
                    self.replace_stack.push('\0');
                    state.insert_char(text, *c);
                } else {
                    self.replace_stack.push_str(
                        &text.slice(state.cursor.get_index()..next),
                    );
                    let ch = c.to_string();
                    state.replace_range(text,
                        state.cursor.get_index()..next,
                        &ch,
                    );
                    state.cursor.next_char(text);
                    state.anchor = state.cursor.clone();
                    state.update_preferred_col(text);
                }
            }
            _ => return on_input_shared(state, text, event),
        }
        true
    }

    fn emit_mapped_rhs(&mut self, state: &mut EditorState<T>, text: &mut T, rhs: Vec<Chord>) -> bool {
        let events: Vec<InputEvent> =
            rhs.into_iter().map(InputEvent::from_chord).collect();
        let mut q = InputQueue::new_flushing(&events, false);
        while q.peek().is_some() {
            if !self.on_input_inner(state, text, &mut q) {
                break;
            }
        }
        true
    }

    fn check_mode_mappings(
        mode: ViMode,
        queue: &[InputEvent],
    ) -> Result<Option<Vec<Chord>>, ()> {
        SHARED_VI_STATE.with(|shared| {
            let shared = shared.borrow();
            let maps = match mode {
                ViMode::Normal => &shared.normal_maps,
                ViMode::Operator => &shared.operator_maps,
                ViMode::Visual | ViMode::VisualLine => &shared.visual_maps,
                ViMode::Insert => &shared.insert_maps,
                ViMode::Replace => &shared.replace_maps,
            };
            if maps.is_empty() {
                return Err(());
            }
            Self::check_mappings(queue, maps)
        })
    }

    fn on_input_inner(&mut self, state: &mut EditorState<T>, text: &mut T, queue: &mut InputQueue) -> bool {
        if let Some(event) = queue.peek() {
            if let Trigger::Paste(paste) = &event.chord.trigger {
                queue.next();
                state.seal_undo_group();
                match self.mode {
                    ViMode::Normal | ViMode::Operator => {
                        self.paste_str_at(state, text, &paste, Sign::Positive);
                    }
                    ViMode::Insert | ViMode::Replace => {
                        self.record_insert_key(event);
                        state.insert_str(text, &paste);
                    }
                    ViMode::Visual | ViMode::VisualLine => {
                        state.delete_selection(text);
                        state.insert_str(text, &paste);
                    }
                }
                return true;
            }
        }
        match self.mode {
            ViMode::Normal | ViMode::Operator => {
                if !queue.is_flushing() {
                    match Self::check_mode_mappings(ViMode::Normal, queue.get_all()) {
                        Ok(Some(action)) => {
                            while queue.next().is_some() {}
                            self.set_mode(state, ViMode::Normal);
                            return self.emit_mapped_rhs(state, text, action);
                        }
                        Ok(None) => {
                            self.set_mode(state, ViMode::Operator);
                            return false;
                        }
                        Err(()) => {}
                    }
                }

                if !self.parse_normal_queue(state, text, queue) {
                    self.set_mode(state, ViMode::Operator);
                    return false;
                }
                if self.mode == ViMode::Operator {
                    self.set_mode(state, ViMode::Normal);
                }
                tuie::dirty_paint();
                let consumed_events = &queue.get_all()[..queue.get_consumed()];
                if !self.replaying()
                    && Self::is_normal_command_repeatable(consumed_events)
                {
                    self.recording_insert.clear();
                    if self.mode == ViMode::Insert
                        || self.mode == ViMode::Replace
                        || state.is_dirty()
                    {
                        self.save_last(consumed_events.to_vec());
                    }
                }
                true
            }
            ViMode::Visual | ViMode::VisualLine => {
                if !queue.is_flushing() {
                    match Self::check_mode_mappings(self.mode, queue.get_all()) {
                        Ok(Some(action)) => {
                            while queue.next().is_some() {}
                            return self.emit_mapped_rhs(state, text, action);
                        }
                        Ok(None) => {
                            return false;
                        }
                        Err(()) => {}
                    }
                }

                let handled = self.parse_visual_queue(state, text, queue);
                if handled {
                    tuie::dirty_paint();
                }
                handled
            }
            ViMode::Insert | ViMode::Replace => {
                if !queue.is_flushing() {
                    match Self::check_mode_mappings(self.mode, queue.get_all()) {
                        Ok(Some(action)) => {
                            while queue.next().is_some() {}
                            return self.emit_mapped_rhs(state, text, action);
                        }
                        Ok(None) => {
                            return false;
                        }
                        Err(()) => {}
                    }
                }

                let Some(event) = queue.next() else { return false; };
                if self.mode == ViMode::Insert {
                    self.on_input_insert_mode(state, text, &event)
                } else {
                    self.on_input_replace_mode(state, text, &event)
                }
            }
        }
    }
}

impl<T: TextDocument + 'static> InputBindings<T> for ViBindings<T> {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn configure_state(&self, state: &mut EditorState<T>) {
        state.inclusive_selection = false;
        let entry = state.cursor.get_index();
        SHARED_VI_STATE.with(|shared| {
            shared.borrow_mut().insert_entry_pos = Some(entry);
        });
    }

    fn on_blur(&mut self, state: &mut EditorState<T>, text: &T) {
        match self.mode {
            ViMode::Visual | ViMode::VisualLine => {
                self.exit_visual_to_normal(state, text);
            }
            ViMode::Replace => {
                self.escape_to_normal(state, text);
            }
            ViMode::Operator => {
                self.mode = ViMode::Normal;
            }
            _ => {}
        }
    }

    fn get_cursor_shape(&self, _state: &EditorState<T>) -> CursorShape {
        get_cursor_shape(self.mode)
    }

    fn get_cursor_index(&self, state: &EditorState<T>, _text: &T) -> usize {
        state.cursor.get_index()
    }

    fn get_selected_range(&self, state: &EditorState<T>, text: &T) -> std::ops::Range<usize> {
        match self.mode {
            ViMode::VisualLine => {
                let (lo, hi) = state.get_selection();
                let mut start = lo;
                let mut end = hi;
                start.line_start(text);
                end.linewise_end(text);
                let end_idx = std::cmp::min(end.get_index(), text.len() + 1);
                start.get_index()..end_idx
            }
            ViMode::Visual => {
                let (start, mut end) = state.get_selection();
                end.next_grapheme(text);
                start.get_index()..end.get_index()
            }
            _ => {
                let (start, end) = state.get_selection();
                start.get_index()..end.get_index()
            }
        }
    }

    fn on_input(
        &mut self, state: &mut EditorState<T>,
        text: &mut T,
        queue: &mut InputQueue,
    ) -> InputResult {
        if self.on_input_inner(state, text, queue) {
            InputResult::Handled
        } else {
            InputResult::Pending
        }
    }
}

impl<T: TextDocument + 'static> ViBindings<T> {
    /// Builds vi bindings starting in [`ViMode::Insert`].
    pub fn new() -> Box<dyn InputBindings<T>> {
        Box::new(Self {
            mode: ViMode::Insert,
            replace_stack: String::new(),
            marks: FlatLookup::new(),
            recording_insert: Vec::new(),
            pending_insert: None,
            _marker: std::marker::PhantomData,
        })
    }

    /// Switches the active editing mode.
    pub fn set_mode(&mut self, state: &EditorState<T>, mode: ViMode) {
        if self.mode != mode {
            if self.mode == ViMode::Replace {
                self.replace_stack.clear();
            }
            let was_insert = self.mode == ViMode::Insert;
            let now_insert = mode == ViMode::Insert;
            let pos = state.cursor.get_index();
            self.mode = mode;
            if now_insert && !was_insert {
                SHARED_VI_STATE.with(|shared| {
                    shared.borrow_mut().insert_entry_pos = Some(pos);
                });
            } else if was_insert && !now_insert {
                self.set_mark(b'^', pos);
                SHARED_VI_STATE.with(|shared| {
                    shared.borrow_mut().insert_entry_pos = None;
                });
            }
            tuie::dirty_paint();
        }
    }

    /// Returns the current editing mode.
    pub fn get_mode(&self) -> ViMode {
        self.mode
    }
}
