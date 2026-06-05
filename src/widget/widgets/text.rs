//! Styled text widget with configurable overflow, alignment, and tab expansion.

use crate::prelude::*;
use crate::util::text_overflow::{TextOverflow, TextOverflowLineIterator};
use unicode_segmentation::UnicodeSegmentation;

struct TabIterator<'a> {
    tabstop: u8,
    col: u8,
    offset: usize,
    pending: Option<TabIteratorResult<'a>>,
    iter: Option<std::str::Split<'a, char>>,
}

struct TabIteratorResult<'a> {
    width: u64,
    content: &'a str,
    offset: usize,
    leftpad: u8,
}

impl<'a> TabIterator<'a> {
    fn new(col: u8, tabstop: Option<u8>, text: &'a str) -> Self {
        if let Some(tabstop) = tabstop {
            let mut iter = text.split('\t');
            let content = iter.next().unwrap();
            let width = tuie::terminal_display_width(content) as u64;
            Self {
                col: col.wrapping_add(width as u8),
                tabstop,
                offset: content.len() + 1,
                pending: Some(TabIteratorResult {
                    content,
                    width,
                    leftpad: 0,
                    offset: 0,
                }),
                iter: Some(iter),
            }
        } else {
            let width = tuie::terminal_display_width(text) as u64;
            Self {
                col: col.wrapping_add(width as u8),
                tabstop: 0,
                offset: 0,
                pending: Some(TabIteratorResult {
                    content: text,
                    width,
                    leftpad: 0,
                    offset: 0,
                }),
                iter: None,
            }
        }
    }
}

impl<'a> Iterator for TabIterator<'a> {
    type Item = TabIteratorResult<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        let Some(iter) = &mut self.iter else {
            return self.pending.take();
        };
        let upcoming = iter.next().map(|content| {
            let tab_size = self.tabstop - self.col % self.tabstop;
            let width = tuie::terminal_display_width(content) as u64;
            self.col = self.col
                .wrapping_add(width as u8)
                .wrapping_add(tab_size);
            let offset = self.offset;
            self.offset += content.len() + 1;
            TabIteratorResult {
                leftpad: tab_size,
                content,
                width,
                offset,
            }
        });
        std::mem::replace(&mut self.pending, upcoming)
    }
}

/// Click event reporting the byte range of the clicked grapheme.
pub struct TextClickEvent(pub std::ops::Range<usize>);

/// Styled text widget with configurable overflow, alignment, and tab expansion.
pub struct Text {
    layout: Layout,
    content_size: Vec2<u16>,
    overflow: &'static TextOverflow,
    align: Align,
    content: StyledString,
}

impl Text {
    fn flow_size(&self, allocated: Vec2<u16>) -> Vec2<u16> {
        if !self.overflow.get_wrap() {
            if self.overflow.get_truncate().is_some() {
                return Vec2::new(
                    self.content_size.x.min(allocated.x),
                    self.content_size.y.min(allocated.y),
                );
            }
            return self.content_size;
        }
        let content_width = allocated.x;
        if content_width >= self.content_size.x {
            return self.content_size;
        }
        if content_width == 0 {
            return Vec2::of(0);
        }
        if let Some(out) = self.layout.flow_lookup_by_main(Axis2D::X, content_width) {
            return out;
        }
        let mut num_lines: u16 = 0;
        let mut max_width: u16 = 0;
        for line in self.overflow.iter_lines(
            self.content.as_str(),
            Vec2::new(content_width as usize, usize::MAX),
            self.align,
            self.tabstop(),
        ) {
            num_lines = num_lines.saturating_add(1);
            let line_width = (line.width + line.marker_width).min(u16::MAX as usize) as u16;
            if line_width > max_width {
                max_width = line_width;
            }
        }
        Vec2::new(max_width, num_lines)
    }

    fn tabstop(&self) -> Option<u8> {
        let config = tuie::config::get();
        if config.expand_tabs {
            Some(config.tabstop)
        } else {
            None
        }
    }
}

impl Widget for Text {
    fn get_layout(&self) -> &Layout {
        &self.layout
    }

    fn get_layout_mut(&mut self) -> &mut Layout {
        &mut self.layout
    }

    fn get_name(&self) -> &'static str {
        "Text"
    }

    fn measure_constraints(&mut self) -> Constraints {
        let mut lines = 0u16;
        let mut width = 0u16;
        let tabstop = self.tabstop();
        for line in self.content.as_str().split('\n') {
            lines = lines.saturating_add(1);
            let line_width = TabIterator::new(0, tabstop, line)
                .fold(0u64, |acc, part| acc + part.width + part.leftpad as u64);
            width = std::cmp::max(width, line_width.min(u16::MAX as u64) as u16);
        }
        self.content_size = Vec2::new(width, lines);

        let min_size = if self.overflow.get_wrap() || self.overflow.get_truncate().is_some() {
            Vec2::new(0, self.content_size.y)
        } else {
            self.content_size
        };
        Constraints {
            min_size,
            max_size: Vec2::of(u16::MAX),
            preferred_size: self.content_size,
        }
    }

    fn layout_flow(&mut self, allocated: Vec2<u16>) -> Vec2<u16> {
        self.flow_size(allocated)
    }

    fn layout_measure(&self, allocated: Vec2<u16>) -> Vec2<u16> {
        self.flow_size(allocated)
    }

    fn render(&self, mut ctx: RenderContext) {
        let content = &self.content;
        let size = self.layout.rect.size;

        let visible_y_start =
            (ctx.pos.y as i32 - ctx.anchor.y).max(0) as usize;
        let visible_y_end =
            ((ctx.pos.y + ctx.physical_size.y) as i32 - ctx.anchor.y)
                as usize;

        let text = content.as_str();
        let max_size = size.map(|a| a as usize);
        let tabstop = self.tabstop();

        let skip_offset = if !self.overflow.get_wrap() && visible_y_start > 0 {
            let mut text_offset = 0;
            for _ in 0..visible_y_start {
                match text[text_offset..].find('\n') {
                    Some(i) => text_offset += i + 1,
                    None => {
                        text_offset = text.len();
                        break;
                    }
                }
            }
            text_offset
        } else {
            0
        };

        let (iter, skip_rows) = if skip_offset > 0 {
            let iter = TextOverflowLineIterator::new(
                *self.overflow,
                max_size,
                &text[skip_offset..],
                self.align,
                tabstop,
            );
            (iter, visible_y_start)
        } else {
            (self.overflow.iter_lines(text, max_size, self.align, tabstop), 0)
        };

        let mut runs = content
            .iter_chunks(skip_offset..)
            .map({
                let mut end = skip_offset;
                move |(chunk, style)| {
                    end += chunk.len();
                    (end, style)
                }
            })
            .peekable();
        let mut run_at = move |pos: usize| {
            while runs.next_if(|&(end, _)| end <= pos).is_some() {}
            runs.peek().map_or_else(|| (usize::MAX, content.style_at(pos)), |&run| run)
        };
        for line in iter {
            let y = line.y + skip_rows;
            let offset = line.offset + skip_offset;
            if y < visible_y_start {
                continue;
            }
            if y >= visible_y_end {
                break;
            }

            ctx.move_to(Vec2::new(line.pad_left as i32, y as i32));
            if !self.overflow.get_wrap() && self.align == Align::End {
                ctx.set_style(run_at(offset).1);
                ctx.write(line.marker);
            }
            let line_end = offset + line.content.len();
            let mut pos = offset;
            let mut col = 0;
            while pos < line_end {
                let (run_end, style) = run_at(pos);
                let chunk_end = run_end.min(line_end);
                ctx.set_style(style);
                let mut tab_iter = TabIterator::new(col, tabstop, &text[pos..chunk_end]);
                while let Some(part) = tab_iter.next() {
                    for _ in 0..part.leftpad {
                        ctx.write(" ");
                    }
                    ctx.write(part.content);
                }
                col = tab_iter.col;
                pos = chunk_end;
            }
            if self.overflow.get_wrap() || self.align != Align::End {
                if !line.marker.is_empty() {
                    ctx.set_style(Style::new());
                }
                ctx.write(line.marker);
            }
            if line.trailing_whitespace {
                ctx.set_style(run_at(line_end).1);
                ctx.write(" ");
            }
        }

        let eof_style = run_at(content.len()).1;
        if eof_style != Style::new() {
            ctx.set_style(eof_style);
            ctx.write(" ");
        }
    }
}

impl TextBuffer for Text {
    fn len(&self) -> usize {
        self.content.len()
    }

    fn is_char_boundary(&self, index: usize) -> bool {
        self.content.as_str().is_char_boundary(index)
    }

    fn slice(&self, range: std::ops::Range<usize>) -> String {
        self.content.as_str()[range].to_string()
    }

    fn replace_range(&mut self, range: std::ops::Range<usize>, replacement: &str) {
        self.content.replace_range(range, replacement);
        self.dirty_layout();
    }

    fn chunks(
        &self,
        range: std::ops::Range<usize>,
    ) -> Box<dyn Iterator<Item = &str> + '_> {
        Box::new(std::iter::once(&self.content.as_str()[range]))
    }

    fn index_to_physical_pos(&self, index: usize) -> Vec2<usize> {
        Text::index_to_physical_pos(self, index)
    }
}

impl TextDocument for Text {
    type Cursor = TextCursor;
    fn cursor(&self, index: usize) -> TextCursor {
        TextCursor { index }
    }
}

impl TextLayout for Text {
    fn index_to_virtual_pos(&self, index: usize, wrap_bias: Sign) -> Vec2<usize> {
        Text::index_to_virtual_pos(self, index, wrap_bias)
    }

    fn pos_to_index(&self, pos: Vec2<usize>) -> usize {
        Text::pos_to_index(self, pos)
    }

    fn get_visible_size(&self) -> Vec2<usize> {
        self.layout.rect.size.map(|v| v as usize)
    }
}

impl Text {
    /// Creates an empty [`Text`] with [`TextOverflow::VISIBLE`] and start alignment.
    pub fn new() -> Box<Self> {
        Box::new(Self {
            layout: Layout::new(),
            content_size: Vec2::of(0),
            overflow: TextOverflow::VISIBLE,
            align: Align::Start,
            content: StyledString::new(),
        })
    }

    /// Builder form of [`Text::set_content`].
    pub fn content<T: Into<StyledString>>(
        mut self: Box<Self>,
        content: T,
    ) -> Box<Self> {
        self.set_content(content);
        self
    }

    /// Sets the styled content.
    pub fn set_content<T: Into<StyledString>>(&mut self, content: T) {
        self.content = content.into();
        self.dirty_layout();
    }

    /// Appends a styled span to the existing content.
    pub fn push<'a>(&mut self, s: impl Into<StyledStr<'a>>) {
        self.content.push_span(s.into());
        self.dirty_layout();
    }

    /// Removes all content.
    pub fn clear_content(&mut self) {
        self.content.clear();
        self.dirty_layout();
    }

    /// Maps a byte index to a cell position, using `wrap_bias` to resolve wrap boundaries.
    pub fn index_to_virtual_pos(
        &self,
        index: usize,
        wrap_bias: Sign,
    ) -> Vec2<usize> {
        let content = &self.content;
        let size = self.layout.rect.size;
        if index >= content.len() {
            for line in self.overflow.iter_lines(
                content.as_str(),
                size.map(|a| a as usize),
                self.align,
                self.tabstop(),
            ) {
                if line.offset + line.content.len() == content.len()
                {
                    let width =
                        TabIterator::new(0, self.tabstop(), line.content)
                            .fold(0, |acc, part| {
                                acc + part.width + part.leftpad as u64
                            });
                    return Vec2::new(line.pad_left + width as usize, line.y);
                }
            }
        }
        for line in self.overflow.iter_lines(
            content.as_str(),
            size.map(|a| a as usize),
            self.align,
            self.tabstop(),
        ) {
            if index <= line.offset + line.content.len() {
                let mut w = line.pad_left;
                for part in TabIterator::new(0, self.tabstop(), line.content) {
                    w += part.leftpad as usize;
                    let mut offset = line.offset + part.offset;
                    if offset + part.content.len() < index {
                        w += part.width as usize;
                        continue;
                    }
                    for grapheme in part.content.graphemes(true) {
                        offset += grapheme.len();
                        if offset > index {
                            break;
                        }
                        w += tuie::terminal_grapheme_width(grapheme);
                    }
                    break;
                }
                if (w as u16) < size.x || line.trailing_whitespace {
                    if index == line.offset + line.content.len()
                        && !line.marker.is_empty()
                    {
                        return Vec2::new(0, line.y + 1);
                    }
                    return Vec2::new(w, line.y);
                }
                if wrap_bias == Sign::Negative
                    || content.as_str()[line.offset + line.content.len()..]
                        .chars()
                        .next()
                        == Some('\n')
                {
                    return Vec2::new(w, line.y);
                }
                return Vec2::new(0, line.y + 1);
            }
        }
        Vec2::new(0, 0)
    }

    /// Maps a byte index to its physical (unwrapped) cell position.
    pub fn index_to_physical_pos(&self, index: usize) -> Vec2<usize> {
        let text = self.content.as_str();
        let index = index.min(text.len());
        let line_start = text[..index].rfind('\n').map_or(0, |i| i + 1);
        let y = text[..line_start].bytes().filter(|&b| b == b'\n').count();
        let mut x = 0usize;
        for part in TabIterator::new(0, self.tabstop(), &text[line_start..index]) {
            x += part.leftpad as usize + part.width as usize;
        }
        Vec2::new(x, y)
    }

    /// Maps a cell position back to a byte index, snapping to the nearest grapheme.
    pub fn pos_to_index(&self, pos: Vec2<usize>) -> usize {
        let content = &self.content;
        for line in self.overflow.iter_lines(
            content.as_str(),
            self.layout.rect.size.map(|a| a as usize),
            self.align,
            self.tabstop(),
        ) {
            if line.y as usize == pos.y {
                let mut remaining = pos.x as i32 - line.pad_left as i32;
                for part in TabIterator::new(
                    0,
                    self.tabstop(),
                    &content.as_str()
                        [line.offset..line.offset + line.content.len()],
                ) {
                    let mut offset = line.offset + part.offset;
                    remaining -= part.leftpad as i32;
                    if remaining < 0 {
                        return offset.saturating_sub(1);
                    }
                    if remaining > part.width as i32 {
                        remaining -= part.width as i32;
                    } else {
                        for grapheme in part.content.graphemes(true) {
                            let width =
                                tuie::terminal_grapheme_width(grapheme) as i32;
                            remaining -= width;
                            if remaining < 0 {
                                return offset;
                            }
                            offset += grapheme.len();
                        }
                    }
                }
                return line.offset + line.content.len();
            }
        }
        content.len()
    }

    /// Borrows the underlying text without styling.
    pub fn get_str(&self) -> &str {
        self.content.as_str()
    }

    /// Clones the underlying text without styling.
    pub fn get_string(&self) -> String {
        self.content.as_str().to_string()
    }

    /// Clones the styled content.
    pub fn get_content(&self) -> StyledString {
        self.content.clone()
    }

    crate::style_field! {
        /// Horizontal alignment of each line.
        align: Align
    }

    /// Builder shortcut for `.align(Align::Start)`.
    pub fn left(self: Box<Self>) -> Box<Self> {
        self.align(Align::Start)
    }

    /// Builder shortcut for `.align(Align::Center)`.
    pub fn center(self: Box<Self>) -> Box<Self> {
        self.align(Align::Center)
    }

    /// Builder shortcut for `.align(Align::End)`.
    pub fn right(self: Box<Self>) -> Box<Self> {
        self.align(Align::End)
    }

    crate::layout_field! {
        /// The overflow strategy.
        overflow: &'static TextOverflow
    }

    /// Builder shortcut for `.overflow(TextOverflow::WORD_WRAP)`.
    pub fn word_wrap(self: Box<Self>) -> Box<Self> {
        self.overflow(TextOverflow::WORD_WRAP)
    }

    /// Builder shortcut for `.overflow(TextOverflow::WRAP)`.
    pub fn wrap(self: Box<Self>) -> Box<Self> {
        self.overflow(TextOverflow::WRAP)
    }

    /// Builder shortcut for `.overflow(TextOverflow::ELLIPSIS)`.
    pub fn ellipsis(self: Box<Self>) -> Box<Self> {
        self.overflow(TextOverflow::ELLIPSIS)
    }

    /// Builder shortcut for `.overflow(TextOverflow::TRUNCATE)`.
    pub fn truncate(self: Box<Self>) -> Box<Self> {
        self.overflow(TextOverflow::TRUNCATE)
    }

    /// Applies `style` to the byte `range`.
    pub fn highlight(&mut self, range: std::ops::Range<usize>, style: Style) {
        self.dirty_paint();
        self.content.style_range(range, |s| *s = style);
    }

    /// Clears all style spans, leaving the text unstyled.
    pub fn clear_highlight(&mut self) {
        self.dirty_paint();
        self.content.clear_styles();
    }
}

/// Byte-indexed cursor into a [`Text`].
#[derive(Clone, Debug)]
pub struct TextCursor {
    index: usize,
}

impl PartialEq for TextCursor {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}

impl Eq for TextCursor {}

impl PartialOrd for TextCursor {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TextCursor {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.index.cmp(&other.index)
    }
}

impl PartialEq<usize> for TextCursor {
    fn eq(&self, other: &usize) -> bool {
        self.index == *other
    }
}

impl PartialOrd<usize> for TextCursor {
    fn partial_cmp(&self, other: &usize) -> Option<std::cmp::Ordering> {
        self.index.partial_cmp(other)
    }
}

impl Cursor for TextCursor {
    type Text = Text;

    fn get_index(&self) -> usize {
        self.index
    }

    fn set_index(&mut self, text: &Text, index: usize) {
        self.index = index;
        let len = text.len();
        while self.index > 0
            && self.index <= len
            && !text.is_char_boundary(self.index)
        {
            self.index -= 1;
        }
    }

    fn get_char(&self, text: &Text) -> char {
        let content = text.content.as_str();
        if self.index >= content.len() {
            return '\0';
        }
        content[self.index..]
            .chars()
            .next()
            .unwrap_or('\0')
    }

    fn next_char(&mut self, text: &Text) -> &mut Self {
        let content = text.content.as_str();
        if let Some(ch) = content[self.index..].chars().next() {
            self.index += ch.len_utf8();
        }

        self
    }

    fn prev_char(&mut self, text: &Text) -> &mut Self {
        let content = text.content.as_str();
        if let Some(ch) = content[..self.index].chars().next_back() {
            self.index -= ch.len_utf8();
        }

        self
    }

    fn next_grapheme(&mut self, text: &Text) -> &mut Self {
        let content = text.content.as_str();
        if let Some(g) = content[self.index..].graphemes(true).next()
        {
            self.index += g.len();
        }

        self
    }

    fn prev_grapheme(&mut self, text: &Text) -> &mut Self {
        let content = text.content.as_str();
        if let Some(g) =
            content[..self.index].graphemes(true).next_back()
        {
            self.index -= g.len();
        }

        self
    }

    fn line_start(&mut self, text: &Text) -> &mut Self {
        if self.index > 0 {
            let content = text.content.as_str();
            self.index = content[..self.index]
                .rfind('\n')
                .map(|i| i + 1)
                .unwrap_or(0);
        }
        self
    }

    fn line_end(&mut self, text: &Text) -> &mut Self {
        let content = text.content.as_str();
        self.index = content[self.index..]
            .find('\n')
            .map(|i| self.index + i)
            .unwrap_or(content.len());

        self
    }

    fn linewise_end(&mut self, text: &Text) -> &mut Self {
        let content = text.content.as_str();
        let len = content.len();
        if let Some(i) = content[self.index..].find('\n') {
            self.index = self.index + i + 1;
        } else {
            self.index = len + 1;
        }

        self
    }

    fn next_line_start(&mut self, text: &Text) -> &mut Self {
        let content = text.content.as_str();
        if let Some(i) = content[self.index..].find('\n') {
            self.index = self.index + i + 1;
        } else {
            self.index = content.len();
        }

        self
    }

    fn prev_line_start(&mut self, text: &Text) -> &mut Self {
        if self.index > 0 {
            let content = text.content.as_str();
            let line_start = content[..self.index]
                .rfind('\n')
                .map(|i| i + 1)
                .unwrap_or(0);
            if line_start == 0 {
                self.index = 0;
            } else {
                self.index = content[..line_start - 1]
                    .rfind('\n')
                    .map(|i| i + 1)
                    .unwrap_or(0);
            }
        }
        self
    }

    fn find_char_forward(&mut self, text: &Text, ch: char) -> &mut Self {
        let content = text.content.as_str();
        if let Some(i) = content[self.index..].find(ch) {
            self.index += i;
        }

        self
    }

    fn find_char_backward(&mut self, text: &Text, ch: char) -> &mut Self {
        let content = text.content.as_str();
        if let Some(i) = content[..self.index].rfind(ch) {
            self.index = i;
        }

        self
    }

    fn find_str_forward(&mut self, text: &Text, needle: &str) -> &mut Self {
        let content = text.content.as_str();
        if let Some(i) = content[self.index..].find(needle) {
            self.index += i;
        }

        self
    }

    fn find_str_backward(&mut self, text: &Text, needle: &str) -> &mut Self {
        let content = text.content.as_str();
        if let Some(i) = content[..self.index].rfind(needle) {
            self.index = i;
        }

        self
    }

    fn matches(&self, text: &Text, needle: &str) -> bool {
        let content = text.content.as_str();
        content[self.index..].starts_with(needle)
    }

    fn document_start(&mut self) -> &mut Self {
        self.index = 0;
        self
    }

    fn document_end(&mut self, text: &Text) -> &mut Self {
        self.index = text.len();
        self
    }

    fn get_virtual_pos(&self, text: &Text, wrap_bias: Sign) -> Vec2<usize> {
        text.index_to_virtual_pos(self.index, wrap_bias)
    }

    fn get_physical_pos(&self, text: &Text) -> Vec2<usize> {
        text.index_to_physical_pos(self.index)
    }
}

