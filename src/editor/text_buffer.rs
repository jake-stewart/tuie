//! Traits for editor backing stores.

use crate::prelude::*;
use crate::editor::char_class::{CharClass, GetCharClass};

/// Random-access byte storage for editable text.
pub trait TextBuffer {
    /// Returns the total length in bytes.
    fn len(&self) -> usize;
    /// Returns whether `index` is a `char` boundary.
    fn is_char_boundary(&self, index: usize) -> bool;
    /// Returns the substring `range` as a new [`String`].
    fn slice(&self, range: std::ops::Range<usize>) -> String;

    /// Replaces the byte `range` with `replacement`.
    fn replace_range(&mut self, range: std::ops::Range<usize>, replacement: &str);

    /// Iterates contiguous chunks covering `range`.
    fn chunks(
        &self,
        range: std::ops::Range<usize>,
    ) -> Box<dyn Iterator<Item = &str> + '_>;

    /// Returns the logical line/column position of `index`, ignoring soft-wrap.
    fn index_to_physical_pos(&self, index: usize) -> Vec2<usize>;
}

/// Position within a [`TextContent`] with movement primitives.
pub trait Cursor: Sized + Clone + Eq + Ord {
    /// Underlying text type this cursor walks.
    type Text: TextContent + ?Sized;

    /// Returns the byte offset of this cursor.
    fn get_index(&self) -> usize;

    /// Moves the cursor to byte offset `index`.
    fn set_index(&mut self, text: &Self::Text, index: usize);

    /// Returns the char at the current position, or `'\0'` at end of file.
    fn get_char(&self, text: &Self::Text) -> char;

    /// Returns whether the text at the current position starts with `needle`.
    fn matches(&self, text: &Self::Text, needle: &str) -> bool;

    /// Returns the wrapped screen position of the cursor.
    fn get_virtual_pos(&self, text: &Self::Text, wrap_bias: Sign) -> Vec2<usize>;

    /// Returns the logical line/column position of the cursor.
    fn get_physical_pos(&self, text: &Self::Text) -> Vec2<usize>;

    /// Advances one char forward.
    fn next_char(&mut self, text: &Self::Text) -> &mut Self;
    /// Moves one char backward.
    fn prev_char(&mut self, text: &Self::Text) -> &mut Self;

    /// Advances one grapheme cluster forward.
    fn next_grapheme(&mut self, text: &Self::Text) -> &mut Self;
    /// Moves one grapheme cluster backward.
    fn prev_grapheme(&mut self, text: &Self::Text) -> &mut Self;

    /// Moves to the start of the next line, or to the end of the document if there is none.
    fn next_line_start(&mut self, text: &Self::Text) -> &mut Self;
    /// Moves to the start of the previous line, or stays at the document start.
    fn prev_line_start(&mut self, text: &Self::Text) -> &mut Self;

    /// Finds `ch` forward from the current position. Moves the cursor if found.
    fn find_char_forward(&mut self, text: &Self::Text, ch: char) -> &mut Self;
    /// Finds `ch` backward from the current position. Moves the cursor if found.
    fn find_char_backward(&mut self, text: &Self::Text, ch: char) -> &mut Self;

    /// Finds `needle` forward from the current position. Moves the cursor if found.
    fn find_str_forward(&mut self, text: &Self::Text, needle: &str) -> &mut Self;
    /// Finds `needle` backward from the current position. Moves the cursor if found.
    fn find_str_backward(&mut self, text: &Self::Text, needle: &str) -> &mut Self;

    /// Moves to the start of the current line.
    fn line_start(&mut self, text: &Self::Text) -> &mut Self;

    /// Moves to the `\n` ending the current line, or to the end of the document.
    fn line_end(&mut self, text: &Self::Text) -> &mut Self;

    /// Moves to the exclusive end position for linewise operations.
    fn linewise_end(&mut self, text: &Self::Text) -> &mut Self;

    /// Moves to byte offset 0.
    fn document_start(&mut self) -> &mut Self;
    /// Moves to the end of the document.
    fn document_end(&mut self, text: &Self::Text) -> &mut Self;
}

/// Convenience methods provided to every [`Cursor`] via a blanket impl.
pub trait CursorMethods: Cursor {
    /// Returns whether the cursor is at a line end or end of file.
    fn at_line_end(&self, text: &Self::Text) -> bool {
        matches!(self.get_char(text), '\n' | '\0')
    }

    /// Returns whether the cursor is past the last character.
    fn at_eof(&self, text: &Self::Text) -> bool {
        self.get_char(text) == '\0'
    }

    /// Moves one char in the given direction.
    fn move_char(&mut self, text: &Self::Text, sign: Sign) -> &mut Self {
        match sign {
            Sign::Positive => self.next_char(text),
            Sign::Negative => self.prev_char(text),
        }
    }

    /// Moves one grapheme cluster in the given direction.
    fn move_grapheme(&mut self, text: &Self::Text, sign: Sign) -> &mut Self {
        match sign {
            Sign::Positive => self.next_grapheme(text),
            Sign::Negative => self.prev_grapheme(text),
        }
    }

    /// Moves to the start of the adjacent line in `sign`.
    fn move_line_start(&mut self, text: &Self::Text, sign: Sign) -> &mut Self {
        match sign {
            Sign::Positive => self.next_line_start(text),
            Sign::Negative => self.prev_line_start(text),
        }
    }

    /// Finds `ch` in `sign` from the current position.
    fn find_char(&mut self, text: &Self::Text, sign: Sign, ch: char) -> &mut Self {
        match sign {
            Sign::Positive => self.find_char_forward(text, ch),
            Sign::Negative => self.find_char_backward(text, ch),
        }
    }

    /// Finds `needle` in `sign` from the current position.
    fn find_str(&mut self, text: &Self::Text, sign: Sign, needle: &str) -> &mut Self {
        match sign {
            Sign::Positive => self.find_str_forward(text, needle),
            Sign::Negative => self.find_str_backward(text, needle),
        }
    }

    /// Moves to the document start or end depending on `sign`.
    fn move_document_edge(&mut self, text: &Self::Text, sign: Sign) -> &mut Self {
        match sign {
            Sign::Positive => self.document_end(text),
            Sign::Negative => self.document_start(),
        }
    }

    /// Moves to the line start or end depending on `sign`.
    fn move_line_edge(&mut self, text: &Self::Text, sign: Sign) -> &mut Self {
        match sign {
            Sign::Positive => self.line_end(text),
            Sign::Negative => self.line_start(text),
        }
    }

    /// Advances the cursor while the grapheme under it matches `class`.
    fn scan_while_class(&mut self, text: &Self::Text, sign: Sign, class: CharClass) {
        match sign {
            Sign::Positive => {
                while self.get_char(text) != '\0' && self.get_char(text).get_class() == class {
                    self.move_grapheme(text, sign);
                }
            }
            Sign::Negative => loop {
                let prev = self.clone();
                self.move_grapheme(text, sign);
                if *self == prev {
                    break;
                }
                if self.get_char(text).get_class() != class {
                    *self = prev;
                    break;
                }
            },
        }
    }

    /// Advances the cursor by `count` characters, moving backward for negative values.
    fn seek_chars(&mut self, text: &Self::Text, count: i64) -> &mut Self {
        let sign = if count >= 0 {
            Sign::Positive
        } else {
            Sign::Negative
        };
        for _ in 0..count.unsigned_abs() {
            self.move_char(text, sign);
        }
        self
    }

    /// Returns the [`CharClass`] of the grapheme on `sign`-side of the cursor, or `None` at the document edge.
    fn get_char_class_at(&self, text: &Self::Text, sign: Sign) -> Option<CharClass> {
        let ch = if sign == Sign::Negative {
            let mut probe = self.clone();
            if probe.clone() == *probe.prev_grapheme(text) {
                return None;
            }
            probe.get_char(text)
        } else {
            self.get_char(text)
        };
        if ch == '\0' {
            None
        } else {
            Some(ch.get_class())
        }
    }

    /// Advances the cursor over a run of `class`, returning whether it moved.
    fn skip_class(&mut self, text: &Self::Text, sign: Sign, class: CharClass) -> bool {
        let start = self.clone();
        self.scan_while_class(text, sign, class);
        *self != start
    }

    /// Advances the cursor over whitespace, returning whether it moved.
    fn skip_whitespace(&mut self, text: &Self::Text, sign: Sign) -> bool {
        self.skip_class(text, sign, CharClass::Whitespace)
    }

    /// Advances the cursor over a run of word or symbol characters on the `sign` side.
    fn skip_current_word_class(&mut self, text: &Self::Text, sign: Sign) -> bool {
        match self.get_char_class_at(text, sign) {
            Some(class @ (CharClass::Word | CharClass::Symbol)) => {
                self.skip_class(text, sign, class)
            }
            _ => false,
        }
    }

    /// Moves the cursor one grapheme in `sign`, returning whether it moved.
    fn cursor_step(&mut self, text: &Self::Text, sign: Sign) -> bool {
        self.clone() != *self.move_grapheme(text, sign)
    }

    /// Moves the cursor one word in `sign`.
    fn move_word(&mut self, text: &Self::Text, sign: Sign) {
        self.skip_whitespace(text, sign);
        self.skip_current_word_class(text, sign);
    }

    /// Moves the cursor one grapheme in `sign` without crossing a newline.
    fn move_within_line(&mut self, text: &Self::Text, sign: Sign) {
        match sign {
            Sign::Negative => {
                if self.get_index() > 0 {
                    let mut probe = self.clone();
                    probe.prev_grapheme(text);
                    if probe.get_char(text) != '\n' {
                        *self = probe;
                    }
                }
            }
            Sign::Positive => {
                if self.get_index() < text.len() && self.get_char(text) != '\n' {
                    self.next_grapheme(text);
                }
            }
        }
    }

    /// Returns the byte range of the word under the cursor.
    fn get_word_under_cursor(&self, text: &Self::Text) -> Option<std::ops::Range<usize>> {
        let mut cursor = self.clone();
        let ch = cursor.get_char(text);
        if ch == '\0' {
            return None;
        }
        let class = ch.get_class();
        if class == CharClass::Whitespace {
            return None;
        }
        loop {
            let saved = cursor.clone();
            cursor.prev_char(text);
            if cursor == saved || cursor.get_char(text).get_class() != class {
                break;
            }
        }
        let start = if cursor.get_char(text).get_class() == class {
            cursor.get_index()
        } else {
            cursor.next_char(text).get_index()
        };
        loop {
            if cursor.get_char(text) == '\0' || cursor.get_char(text).get_class() != class {
                break;
            }
            cursor.next_char(text);
        }
        Some(start..cursor.get_index())
    }
}

impl<C: Cursor> CursorMethods for C {}

/// Maps between byte offsets and screen positions.
pub trait TextLayout {
    /// Returns the wrapped screen position of `index`, disambiguated by `wrap_bias`.
    fn index_to_virtual_pos(&self, index: usize, wrap_bias: Sign) -> Vec2<usize>;
    /// Returns the byte offset closest to `pos`.
    fn pos_to_index(&self, pos: Vec2<usize>) -> usize;
    /// Returns the widget's content rect size in cells.
    fn get_visible_size(&self) -> Vec2<usize>;
}

/// Combined [`TextBuffer`] and [`TextLayout`].
pub trait TextContent: TextBuffer + TextLayout {}
impl<T: TextBuffer + TextLayout + ?Sized> TextContent for T {}

/// [`TextContent`] that produces its own [`Cursor`] type.
pub trait TextDocument: TextContent + 'static {
    /// The cursor type for this document.
    type Cursor: Cursor<Text = Self>;
    /// Returns a cursor at byte offset `index`.
    fn cursor(&self, index: usize) -> Self::Cursor;
}
