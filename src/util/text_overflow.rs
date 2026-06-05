//! Text overflow and line-breaking iterator.

use crate::editor::char_class::{CharClass, GetCharClass};
use crate::prelude::*;
use unicode_segmentation::UnicodeSegmentation;

/// Double-ended cursor that yields `(byte_offset, grapheme)` pairs over a line.
pub trait LineCursor<'a>: DoubleEndedIterator<Item = (usize, &'a str)> {}
impl<'a, T: DoubleEndedIterator<Item = (usize, &'a str)>> LineCursor<'a> for T {}

/// Byte-at-a-time [`LineCursor`] for ASCII-only input.
pub struct AsciiCursor<'a> {
    s: &'a str,
    front: usize,
    back: usize,
}

impl<'a> AsciiCursor<'a> {
    /// Creates an `AsciiCursor` over the full byte range of `s`.
    pub fn new(s: &'a str) -> Self {
        Self {
            s,
            front: 0,
            back: s.len(),
        }
    }
}

impl<'a> Iterator for AsciiCursor<'a> {
    type Item = (usize, &'a str);
    fn next(&mut self) -> Option<Self::Item> {
        if self.front >= self.back {
            return None;
        }
        let i = self.front;
        let g = &self.s[i..i + 1];
        self.front += 1;
        Some((i, g))
    }
}

impl<'a> DoubleEndedIterator for AsciiCursor<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front >= self.back {
            return None;
        }
        self.back -= 1;
        let i = self.back;
        Some((i, &self.s[i..i + 1]))
    }
}

/// One visual line emitted by [`TextOverflowLineIterator`].
pub struct TextOverflowLineResult<'a> {
    /// Row index in the output area, starting at 0.
    pub y: usize,
    /// Cells of horizontal padding before the content, from alignment.
    pub pad_left: usize,
    /// Display width of `content` in cells, excluding `marker`.
    pub width: usize,
    /// Whether the source line continues with whitespace past the visible end.
    pub trailing_whitespace: bool,
    /// Truncation or word-break marker appended after `content`.
    pub marker: &'a str,
    /// Display width of `marker` in cells.
    pub marker_width: usize,
    /// Visible slice of the source line.
    pub content: &'a str,
    /// Byte offset of `content` within the original text.
    pub offset: usize,
}

/// Iterator yielding visual lines from a flat `&str` under a [`TextOverflow`] strategy.
pub struct TextOverflowLineIterator<'a> {
    config: TextOverflow,
    align: Align,
    max_size: Vec2<usize>,
    /// Byte offset into the source text of the next line to emit.
    offset: usize,
    /// 1-based row counter for the next line to emit.
    height: usize,
    truncated_marker: (&'static str, usize),
    word_break_marker: (&'static str, usize),
    lines: std::str::Split<'a, char>,
    line: Option<&'a str>,
    line_ascii: bool,
    tabstop: Option<u8>,
}

impl<'a> TextOverflowLineIterator<'a> {
    const NO_MARKER: (&'static str, usize) = ("", 0);

    /// Creates an iterator over `text` bounded by `max_size` cells.
    pub fn new(
        mut config: TextOverflow,
        max_size: Vec2<usize>,
        mut text: &'a str,
        align: Align,
        tabstop: Option<u8>,
    ) -> Self {
        let truncate_str = config.truncate.unwrap_or("");
        let mut truncated_marker_width =
            tuie::terminal_display_width(truncate_str);
        let mut word_break_marker_width =
            tuie::terminal_display_width(config.word_break);
        if max_size.x <= truncated_marker_width {
            config.truncate = Some("");
            truncated_marker_width = 0;
        }
        if max_size.x <= word_break_marker_width {
            config.word_break = "";
            word_break_marker_width = 0;
        }
        if max_size.x == 0 {
            text = "";
        }
        let mut lines = text.split('\n');
        let line = lines.next();
        let line_ascii = line.is_some_and(str::is_ascii);
        Self {
            config,
            max_size,
            height: 1,
            offset: 0,
            truncated_marker: (
                config.truncate.unwrap_or(""),
                truncated_marker_width,
            ),
            word_break_marker: (config.word_break, word_break_marker_width),
            lines,
            line,
            line_ascii,
            align,
            tabstop,
        }
    }

    fn result(
        &mut self,
        line: &'a str,
        width: usize,
        marker: (&'a str, usize),
        offset: usize,
        trailing_whitespace: bool,
    ) -> TextOverflowLineResult<'a> {
        let pad_left = match self.align {
            Align::Start => 0,
            Align::Center => (self.max_size.x.saturating_sub(width)) / 2,
            Align::End => self.max_size.x.saturating_sub(width + marker.1),
        };
        let result = TextOverflowLineResult {
            y: self.height - 1,
            trailing_whitespace,
            pad_left,
            width,
            content: line,
            marker: marker.0,
            marker_width: marker.1,
            offset,
        };
        self.height += 1;
        result
    }

    fn next_line(&mut self) -> Option<TextOverflowLineResult<'a>> {
        let right_align = self.align == Align::End && !self.config.wrap;

        let use_truncate_marker =
            self.height == self.max_size.y || !self.config.wrap;
        let use_word_break_marker = self.config.split && self.config.wrap;

        let marker = if use_truncate_marker {
            self.truncated_marker
        } else if use_word_break_marker {
            self.word_break_marker
        } else {
            Self::NO_MARKER
        };

        if self.line.is_none() {
            self.line = self.lines.next();
            self.line_ascii = self.line.is_some_and(str::is_ascii);
        }
        let line = self.line.take()?;

        if self.line_ascii {
            self.next_line_with(
                line,
                AsciiCursor::new(line),
                marker,
                use_truncate_marker,
                use_word_break_marker,
                right_align,
            )
        } else {
            self.next_line_with(
                line,
                line.grapheme_indices(true),
                marker,
                use_truncate_marker,
                use_word_break_marker,
                right_align,
            )
        }
    }

    fn next_line_with<C: LineCursor<'a>>(
        &mut self,
        line: &'a str,
        mut iter: C,
        marker: (&'a str, usize),
        use_truncate_marker: bool,
        use_word_break_marker: bool,
        right_align: bool,
    ) -> Option<TextOverflowLineResult<'a>> {
        let start = if right_align {
            line.len()
        } else {
            0
        };
        let mut class_idx = start;
        let mut class_width = 0;
        let mut split_idx = start;
        let mut split_width = 0;
        let mut width = 0;
        let mut total_width;
        let mut end;
        let mut current_class: Option<CharClass> = None;
        let allow_clip = self.config.wrap || self.config.truncate.is_some();
        loop {
            if let Some((i, grapheme)) = if right_align {
                iter.next_back()
            } else {
                iter.next()
            } {
                let grapheme_class = Some(grapheme.get_class());

                let grapheme_width = TextOverflow::grapheme_display_width(
                    grapheme, width, self.tabstop,
                );

                end = i;
                if self.config.split {
                    if grapheme_class != current_class {
                        split_idx = i;
                        split_width = width;
                        if current_class == Some(CharClass::Whitespace)
                            || grapheme_class == Some(CharClass::Whitespace)
                        {
                            class_idx = i;
                            class_width = width;
                        }
                        current_class = grapheme_class;
                    } else if grapheme_class == Some(CharClass::Whitespace) {
                        class_idx = i;
                        class_width = width;
                        split_idx = i;
                        split_width = width;
                    }
                }

                let needs_marker = use_truncate_marker
                    || (use_word_break_marker
                        && class_idx == start
                        && split_idx == start);
                let reserve = if needs_marker {
                    marker.1
                } else {
                    0
                };

                if allow_clip && width + grapheme_width + reserve > self.max_size.x {
                    total_width = width + grapheme_width;
                    break;
                }
                width += grapheme_width;
            } else {
                let offset = self.offset;
                self.offset += line.len() + 1;
                self.line = self.lines.next();
                self.line_ascii = self.line.is_some_and(str::is_ascii);
                return Some(self.result(
                    line,
                    width,
                    Self::NO_MARKER,
                    offset,
                    self.line.is_some(),
                ));
            }
        }

        if width == 0 {
            let offset = self.offset;
            self.offset += line.len() + 1;
            return Some(self.result("", width, marker, offset, false));
        }

        let mut next_class_idx = class_idx;
        let mut next_class_width = class_width;
        let mut next_split_idx = split_idx;
        let mut next_split_width = split_width;
        let mut next_class: Option<CharClass> = current_class;

        if total_width <= self.max_size.x {
            while let Some((i, grapheme)) = if right_align {
                iter.next_back()
            } else {
                iter.next()
            } {
                let grapheme_width = TextOverflow::grapheme_display_width(
                    grapheme, total_width, self.tabstop,
                );
                if self.config.split {
                    let grapheme_class = Some(grapheme.get_class());
                    if grapheme_class != next_class {
                        next_split_idx = i;
                        next_split_width = total_width;
                        if next_class == Some(CharClass::Whitespace)
                            || grapheme_class == Some(CharClass::Whitespace)
                        {
                            next_class_idx = i;
                            next_class_width = total_width;
                        }
                        next_class = grapheme_class;
                    } else if grapheme_class == Some(CharClass::Whitespace) {
                        next_class_idx = i;
                        next_class_width = total_width;
                        next_split_idx = i;
                        next_split_width = total_width;
                    }
                }
                total_width += grapheme_width;
                if total_width > self.max_size.x {
                    break;
                }
            }
        }

        if total_width > self.max_size.x {
            if self.align == Align::Center && !self.config.wrap {
                let overflow_columns = tuie::terminal_display_width(line)
                    .saturating_sub(self.max_size.x) / 2;
                let mut skipped_width = 0usize;
                let mut skip_end = 0usize;
                let mut shown_width = 0usize;
                let mut shown_end = line.len();
                for (i, g) in line.grapheme_indices(true) {
                    let grapheme_width = tuie::terminal_grapheme_width(g);
                    if skipped_width < overflow_columns {
                        skipped_width += grapheme_width;
                        skip_end = i + g.len();
                    } else if shown_width + grapheme_width > self.max_size.x {
                        shown_end = i;
                        break;
                    } else {
                        shown_width += grapheme_width;
                    }
                }
                let offset = self.offset + skip_end;
                self.offset += line.len() + 1;
                return Some(self.result(
                    &line[skip_end..shown_end],
                    shown_width,
                    Self::NO_MARKER,
                    offset,
                    false,
                ));
            }

            if class_idx == start && split_idx != start {
                class_idx = split_idx;
                class_width = split_width;
            }
            if use_word_break_marker
                && class_idx == start
                && next_class_idx != start
            {
                class_idx = next_class_idx;
                class_width = next_class_width;
            }
            if use_word_break_marker
                && class_idx == start
                && next_split_idx != start
            {
                class_idx = next_split_idx;
                class_width = next_split_width;
            }
            let mut end = if class_idx != start {
                class_idx
            } else {
                end
            };
            let mut width = if class_idx != start {
                class_width
            } else {
                width
            };
            let remaining = if right_align {
                &line[..end]
            } else {
                &line[end..]
            };
            let partial_line = if right_align {
                if let Some((i, _)) =
                    line[end..].grapheme_indices(true).nth(1)
                {
                    end += i;
                }
                &line[end..]
            } else {
                &line[..end]
            };
            let mut trimmed_line = partial_line;
            let mut trimmed_remaining = remaining;
            let mut trailing_whitespace = false;
            if self.config.trim {
                trimmed_line = partial_line.trim_end();
                width -= tuie::terminal_display_width(
                    &partial_line[trimmed_line.len()..],
                );
                trimmed_remaining = remaining.trim_start();
            } else if self.config.wrap && self.config.split {
                trimmed_line = partial_line.trim_end();
                if trimmed_line.is_empty() {
                    trimmed_line = partial_line;
                } else {
                    width -= tuie::terminal_display_width(
                        &partial_line[trimmed_line.len()..],
                    );
                    let mut trimmed_len =
                        partial_line.len() - trimmed_line.len();
                    if let Some(trailing) = partial_line
                        [trimmed_line.len()..]
                        .graphemes(true)
                        .next()
                    {
                        trailing_whitespace = true;
                        trimmed_len -= trailing.len();
                    }
                    trimmed_remaining = if right_align {
                        &line[..end - trimmed_len]
                    } else {
                        &line[end - trimmed_len..]
                    };
                }
            }
            let offset = self.offset;
            if !trimmed_remaining.is_empty() && self.config.wrap {
                self.offset += partial_line.len() + remaining.len()
                    - trimmed_remaining.len();
                self.line = Some(trimmed_remaining);
            } else {
                self.offset += line.len() + 1;
            }
            if use_truncate_marker {
                return Some(self.result(
                    trimmed_line,
                    width,
                    marker,
                    offset,
                    false,
                ));
            }
            if use_word_break_marker && class_idx == start {
                return Some(self.result(
                    trimmed_line,
                    width,
                    marker,
                    offset,
                    false,
                ));
            }
            return Some(self.result(
                trimmed_line,
                width,
                Self::NO_MARKER,
                offset,
                trailing_whitespace,
            ));
        }

        let offset = self.offset;
        self.offset += line.len() + 1;
        Some(self.result(
            line,
            total_width,
            Self::NO_MARKER,
            offset,
            false,
        ))
    }
}

impl<'a> Iterator for TextOverflowLineIterator<'a> {
    type Item = TextOverflowLineResult<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_line()
    }
}

/// Strategy for fitting text into a bounded area.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct TextOverflow {
    split: bool,
    wrap: bool,
    trim: bool,
    truncate: Option<&'static str>,
    word_break: &'static str,
}

impl TextOverflow {
    /// Single-line truncation with a trailing ellipsis at a word boundary.
    pub const ELLIPSIS: &'static TextOverflow =
        &TextOverflow::new().split(true).trim(true).truncate(Some("…"));

    /// Single-line clip without a visible marker.
    pub const TRUNCATE: &'static TextOverflow =
        &TextOverflow::new().truncate(Some(""));

    /// Full text rendered without clipping or wrapping.
    pub const VISIBLE: &'static TextOverflow = &TextOverflow::new();

    /// Word-boundary wrapping with a hyphen at mid-word breaks.
    pub const WORD_WRAP: &'static TextOverflow =
        &TextOverflow::new().split(true).wrap(true).trim(true).word_break("-");

    /// Grapheme-level wrapping at the right edge.
    pub const WRAP: &'static TextOverflow = &TextOverflow::new().wrap(true);

    /// Creates a strategy equal to [`TextOverflow::VISIBLE`].
    pub const fn new() -> Self {
        Self {
            split: false,
            wrap: false,
            trim: false,
            truncate: None,
            word_break: "",
        }
    }

    /// Sets whether to break at word-class boundaries rather than mid-grapheme.
    pub const fn split(mut self, split: bool) -> Self {
        self.split = split;
        self
    }

    /// Sets whether to wrap overflowing content onto a new line.
    pub const fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    /// Sets whether to drop whitespace at the wrap point.
    pub const fn trim(mut self, trim: bool) -> Self {
        self.trim = trim;
        self
    }

    /// Sets the marker appended when content is truncated, or `None` to allow overflow.
    pub const fn truncate(mut self, truncate: Option<&'static str>) -> Self {
        self.truncate = truncate;
        self
    }

    /// Sets the marker appended at a mid-word wrap point.
    pub const fn word_break(mut self, word_break: &'static str) -> Self {
        self.word_break = word_break;
        self
    }

    /// Returns whether to break at word-class boundaries rather than mid-grapheme.
    pub const fn get_split(&self) -> bool {
        self.split
    }

    /// Returns whether to wrap overflowing content onto a new line.
    pub const fn get_wrap(&self) -> bool {
        self.wrap
    }

    /// Returns whether to drop whitespace at the wrap point.
    pub const fn get_trim(&self) -> bool {
        self.trim
    }

    /// Returns the marker appended when content is truncated, or `None` when overflow is allowed.
    pub const fn get_truncate(&self) -> Option<&'static str> {
        self.truncate
    }

    /// Returns the marker appended at a mid-word wrap point.
    pub const fn get_word_break(&self) -> &'static str {
        self.word_break
    }

    /// Returns the display width of `grapheme` in cells at column `col`, expanding tabs to the
    /// next multiple of `tabstop` when set.
    pub fn grapheme_display_width(
        grapheme: &str,
        col: usize,
        tabstop: Option<u8>,
    ) -> usize {
        if let Some(tabstop) = tabstop {
            if grapheme.as_bytes().first() == Some(&b'\t') {
                let tabstop = tabstop as usize;
                return tabstop - (col % tabstop);
            }
        }
        tuie::terminal_grapheme_width(grapheme)
    }

    /// Returns an iterator over the visual lines of `text` bounded by `max_size` cells.
    pub fn iter_lines<'a>(
        &'a self,
        text: &'a str,
        max_size: Vec2<usize>,
        align: Align,
        tabstop: Option<u8>,
    ) -> TextOverflowLineIterator<'a> {
        TextOverflowLineIterator::new(*self, max_size, text, align, tabstop)
    }
}
