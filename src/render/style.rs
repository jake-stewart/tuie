//! Text styling primitives.

use crate::prelude::*;
use nonmax::NonMaxU8;

/// Boolean text attribute encoded as a single bit in a [`Style`]'s attribute mask.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StyleAttribute {
    /// Bold weight.
    Bold = 1 << 0,
    /// Italic slant.
    Italic = 1 << 1,
    /// Swaps foreground and background.
    Reverse = 1 << 2,
    /// Strikethrough line.
    Strikethrough = 1 << 3,
    /// Reduced intensity.
    Dim = 1 << 4,
}

impl std::fmt::Display for StyleAttribute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bold => write!(f, "Bold"),
            Self::Italic => write!(f, "Italic"),
            Self::Reverse => write!(f, "Reverse"),
            Self::Strikethrough => write!(f, "Strikethrough"),
            Self::Dim => write!(f, "Dim"),
        }
    }
}

const fn clamp_blend(v: u8) -> NonMaxU8 {
    let v = if v > 100 {
        100
    } else {
        v
    };
    unsafe { NonMaxU8::new_unchecked(v) }
}

/// Foreground, background, underline, and boolean attributes for a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    /// Foreground color, or `None` to inherit.
    pub fg: Option<Color>,
    /// Background color, or `None` to inherit.
    pub bg: Option<Color>,
    /// Underline color, or `None` to use the foreground.
    pub underline_color: Option<Color>,
    /// Underline shape, or `None` to inherit.
    pub underline: Option<UnderlineType>,
    attrs: u8,
    mask: u8,
    blend: Option<NonMaxU8>,
}

impl Style {
    /// Creates an empty style with no fields set.
    pub const fn new() -> Self {
        Self {
            fg: None,
            bg: None,
            underline_color: None,
            underline: None,
            attrs: 0,
            mask: 0,
            blend: None,
        }
    }

    /// Style with every field explicitly written to its terminal default.
    pub const fn plain() -> Self {
        Self {
            fg: Some(Color::Foreground),
            bg: Some(Color::Background),
            underline_color: None,
            underline: Some(UnderlineType::None),
            attrs: 0,
            mask: (StyleAttribute::Bold as u8)
                | (StyleAttribute::Italic as u8)
                | (StyleAttribute::Reverse as u8)
                | (StyleAttribute::Strikethrough as u8)
                | (StyleAttribute::Dim as u8),
            blend: None,
        }
    }

    /// Returns `true` if no fields are set (equivalent to [`Style::new`]).
    pub const fn is_empty(&self) -> bool {
        self.fg.is_none()
            && self.bg.is_none()
            && self.underline_color.is_none()
            && self.underline.is_none()
            && self.attrs == 0
            && self.mask == 0
            && self.blend.is_none()
    }

    /// Returns the result of layering `other` on top of `self`, with `other` winning on any field it sets.
    pub const fn apply(&self, other: Style) -> Self {
        Self {
            fg: match other.fg {
                Some(_) => other.fg,
                None => self.fg,
            },
            bg: match other.bg {
                Some(_) => other.bg,
                None => self.bg,
            },
            underline_color: match other.underline_color {
                Some(_) => other.underline_color,
                None => self.underline_color,
            },
            underline: match other.underline {
                Some(_) => other.underline,
                None => self.underline,
            },
            attrs: (other.attrs & other.mask) | (self.attrs & !other.mask),
            mask: other.mask | self.mask,
            blend: match other.blend {
                Some(_) => other.blend,
                None => self.blend,
            },
        }
    }

    /// Sets the blend percentage to `Some(percent)`, clamped to `0..=100`.
    #[must_use]
    pub const fn blend(mut self, percent: u8) -> Self {
        self.blend = Some(clamp_blend(percent));
        self
    }

    /// Sets the blend percentage to `blend`, clamped to `0..=100`. `None` clears it.
    #[must_use]
    pub const fn blend_opt(mut self, blend: Option<u8>) -> Self {
        self.blend = match blend {
            Some(v) => Some(clamp_blend(v)),
            None => None,
        };
        self
    }

    /// Returns the blend percentage if one is set.
    pub const fn get_blend(&self) -> Option<u8> {
        match self.blend {
            Some(v) => Some(v.get()),
            None => None,
        }
    }

    /// Sets the blend percentage. Values are clamped to `0..=100`.
    pub const fn set_blend(&mut self, blend: Option<u8>) {
        self.blend = match blend {
            Some(v) => Some(clamp_blend(v)),
            None => None,
        };
    }

    /// Sets the foreground color.
    #[must_use]
    pub const fn fg(mut self, color: Color) -> Self {
        self.fg = Some(color);
        self
    }

    /// Sets the background color.
    #[must_use]
    pub const fn bg(mut self, color: Color) -> Self {
        self.bg = Some(color);
        self
    }

    /// Sets the underline shape to `Some(underline)`.
    #[must_use]
    pub const fn underline(mut self, underline: UnderlineType) -> Self {
        self.underline = Some(underline);
        self
    }

    /// Sets or clears the underline shape via builder.
    #[must_use]
    pub const fn underline_opt(mut self, underline: Option<UnderlineType>) -> Self {
        self.underline = underline;
        self
    }

    /// Returns the underline shape, if any.
    pub const fn get_underline(&self) -> Option<UnderlineType> {
        self.underline
    }

    /// Sets or clears the underline shape.
    pub const fn set_underline(&mut self, underline: Option<UnderlineType>) {
        self.underline = underline;
    }

    /// Sets the underline color.
    #[must_use]
    pub const fn underline_color(mut self, color: Color) -> Self {
        self.underline_color = Some(color);
        self
    }

    const fn write_attr(&mut self, attr: StyleAttribute, value: bool) {
        self.mask |= attr as u8;
        if value {
            self.attrs |= attr as u8;
        } else {
            self.attrs &= !(attr as u8);
        }
    }

    const fn read_attr(&self, attr: StyleAttribute) -> bool {
        self.attrs & (attr as u8) != 0
    }

    /// Returns the raw packed bits for all [`StyleAttribute`] flags.
    pub const fn get_attrs_bits(&self) -> u8 {
        self.attrs
    }

    /// Returns the mask of which [`StyleAttribute`] flags are explicitly set in this style.
    pub const fn get_attrs_mask(&self) -> u8 {
        self.mask
    }

    /// Builder form of [`Style::set_bold`] that enables bold.
    #[must_use]
    pub const fn bold(self) -> Self {
        self.bold_if(true)
    }
    /// Builder form of [`Style::set_bold`].
    #[must_use]
    pub const fn bold_if(mut self, value: bool) -> Self {
        self.set_bold(value);
        self
    }
    /// Returns whether bold is set.
    pub const fn has_bold(&self) -> bool {
        self.read_attr(StyleAttribute::Bold)
    }
    /// Sets bold to `value`.
    pub const fn set_bold(&mut self, value: bool) {
        self.write_attr(StyleAttribute::Bold, value);
    }

    /// Builder form of [`Style::set_italic`] that enables italic.
    #[must_use]
    pub const fn italic(self) -> Self {
        self.italic_if(true)
    }
    /// Builder form of [`Style::set_italic`].
    #[must_use]
    pub const fn italic_if(mut self, value: bool) -> Self {
        self.set_italic(value);
        self
    }
    /// Returns whether italic is set.
    pub const fn has_italic(&self) -> bool {
        self.read_attr(StyleAttribute::Italic)
    }
    /// Sets italic to `value`.
    pub const fn set_italic(&mut self, value: bool) {
        self.write_attr(StyleAttribute::Italic, value);
    }

    /// Builder form of [`Style::set_strikethrough`] that enables strikethrough.
    #[must_use]
    pub const fn strikethrough(self) -> Self {
        self.strikethrough_if(true)
    }
    /// Builder form of [`Style::set_strikethrough`].
    #[must_use]
    pub const fn strikethrough_if(mut self, value: bool) -> Self {
        self.set_strikethrough(value);
        self
    }
    /// Returns whether strikethrough is set.
    pub const fn has_strikethrough(&self) -> bool {
        self.read_attr(StyleAttribute::Strikethrough)
    }
    /// Sets strikethrough to `value`.
    pub const fn set_strikethrough(&mut self, value: bool) {
        self.write_attr(StyleAttribute::Strikethrough, value);
    }

    /// Builder form of [`Style::set_reverse`] that enables reverse video.
    #[must_use]
    pub const fn reverse(self) -> Self {
        self.reverse_if(true)
    }
    /// Builder form of [`Style::set_reverse`].
    #[must_use]
    pub const fn reverse_if(mut self, value: bool) -> Self {
        self.set_reverse(value);
        self
    }
    /// Returns whether reverse video is set.
    pub const fn has_reverse(&self) -> bool {
        self.read_attr(StyleAttribute::Reverse)
    }
    /// Sets reverse video to `value`.
    pub const fn set_reverse(&mut self, value: bool) {
        self.write_attr(StyleAttribute::Reverse, value);
    }

    /// Returns the color that becomes the visible background: `fg` under reverse, else `bg`.
    pub const fn overlay_color(&self) -> Option<Color> {
        if self.has_reverse() {
            self.fg
        } else {
            self.bg
        }
    }

    /// Writes the visible background color: `fg` under reverse, else `bg`.
    pub const fn set_overlay_color(&mut self, color: Option<Color>) {
        if self.has_reverse() {
            self.fg = color;
        } else {
            self.bg = color;
        }
    }

    /// Builder form of [`Style::set_dim`] that enables dim.
    #[must_use]
    pub const fn dim(self) -> Self {
        self.dim_if(true)
    }
    /// Builder form of [`Style::set_dim`].
    #[must_use]
    pub const fn dim_if(mut self, value: bool) -> Self {
        self.set_dim(value);
        self
    }
    /// Returns whether dim is set.
    pub const fn has_dim(&self) -> bool {
        self.read_attr(StyleAttribute::Dim)
    }
    /// Sets dim to `value`.
    pub const fn set_dim(&mut self, value: bool) {
        self.write_attr(StyleAttribute::Dim, value);
    }
}

impl Default for Style {
    fn default() -> Self {
        Self::new()
    }
}

/// Run of `len` bytes sharing one [`Style`] inside a [`StyledString`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Style applied to every byte in the run.
    pub style: Style,
    /// Number of bytes in the run.
    pub len: usize,
}

impl Span {
    /// Creates a span of `len` bytes with the given [`Style`].
    pub const fn new(len: usize, style: Style) -> Self {
        Self { len, style }
    }
}

/// Owned string paired with per-byte styling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledString {
    /// Underlying text bytes.
    pub text: String,
    /// Per-byte style runs.
    pub spans: Vec<Span>,
}

impl StyledString {
    /// Builds an empty [`StyledString`].
    pub const fn new() -> Self {
        Self {
            text: String::new(),
            spans: Vec::new(),
        }
    }

    /// Appends `s` as a span and returns self for chaining.
    #[must_use]
    pub fn span<'a>(mut self, s: impl Into<StyledStr<'a>>) -> Self {
        self.push_span(s.into());
        self
    }

    /// Parses ANSI escape sequences in `input` into styled spans.
    pub fn from_ansi(input: &str) -> Self {
        let mut parser = AnsiStyleParser::new();
        parser.parse_line(input)
    }

    /// Appends `s` with default styling.
    pub fn push_str(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        self.text.push_str(s);
        if self.spans.is_empty() {
            return;
        }
        let last = self.spans.last_mut().unwrap();
        let eof_style = last.style;
        if last.len > 1 {
            last.len -= 1;
            if eof_style == Style::new() {
                last.len += s.len();
                self.spans.push(Span::new(1, Style::new()));
            } else {
                self.spans.push(Span { style: Style::new(), len: s.len() });
                self.spans.push(Span { style: eof_style, len: 1 });
            }
        } else {
            let span_count = self.spans.len();
            if span_count >= 2 && self.spans[span_count - 2].style == Style::new() {
                self.spans[span_count - 2].len += s.len();
            } else {
                self.spans.insert(span_count - 1, Span { style: Style::new(), len: s.len() });
            }
        }
    }

    /// Appends `span`, merging with the previous span when their styles match.
    pub fn push_span(&mut self, span: StyledStr) {
        if span.text.is_empty() {
            return;
        }
        if self.spans.is_empty() {
            if span.style == Style::new() {
                self.text.push_str(span.text);
                return;
            }
            self.spans.push(Span::new(self.text.len() + 1, Style::new()));
        }
        self.text.push_str(span.text);
        let last = self.spans.last_mut().unwrap();
        let eof_style = last.style;
        if last.len > 1 {
            last.len -= 1;
            if eof_style == span.style {
                last.len += span.text.len();
                self.spans.push(Span { style: eof_style, len: 1 });
            } else {
                self.spans.push(Span { style: span.style, len: span.text.len() });
                self.spans.push(Span { style: eof_style, len: 1 });
            }
        } else {
            let span_count = self.spans.len();
            if span_count >= 2 && self.spans[span_count - 2].style == span.style {
                self.spans[span_count - 2].len += span.text.len();
            } else {
                self.spans.insert(span_count - 1, Span { style: span.style, len: span.text.len() });
            }
        }
    }

    /// Removes all text and spans.
    pub fn clear(&mut self) {
        self.text.clear();
        self.spans.clear();
    }

    /// Applies `f` to the style of every byte inside `range`.
    pub fn style_range(&mut self, range: std::ops::Range<usize>, f: impl Fn(&mut Style)) {
        let start = range.start;
        let end = range.end.min(self.text.len());
        if start >= end {
            return;
        }
        if self.spans.is_empty() {
            self.spans.push(Span::new(self.text.len() + 1, Style::new()));
        }
        let mut left_pos = 0;
        let mut left = 0;
        while left < self.spans.len() && start > left_pos + self.spans[left].len {
            left_pos += self.spans[left].len;
            left += 1;
        }
        let mut right_pos = left_pos + self.spans[left].len;
        let mut right = left;
        while right < self.spans.len() && end > right_pos {
            right += 1;
            right_pos += self.spans[right].len;
        }
        let mut mid_style = self.spans[left].style;
        f(&mut mid_style);
        self.spans.splice(
            left..=right,
            [
                Span {
                    len: start - left_pos,
                    style: self.spans[left].style,
                },
                Span {
                    len: end - start,
                    style: mid_style,
                },
                Span {
                    len: right_pos - end,
                    style: self.spans[right].style,
                },
            ],
        );
        self.collapse_spans();
    }

    /// Drops the first `n` bytes from `text` and adjusts spans to match.
    pub fn trim_left(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        let n = n.min(self.text.len());
        self.text.drain(..n);
        if self.spans.is_empty() {
            return;
        }
        let mut remaining = n;
        let mut i = 0;
        while i < self.spans.len() && remaining > 0 {
            if self.spans[i].len <= remaining {
                remaining -= self.spans[i].len;
                i += 1;
            } else {
                self.spans[i].len -= remaining;
                remaining = 0;
            }
        }
        self.spans.drain(..i);
        if self.spans.is_empty() {
            self.spans.push(Span::new(1, Style::new()));
        }
    }

    /// Drops the last `n` bytes from `text` and adjusts spans to match.
    pub fn trim_right(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        let n = n.min(self.text.len());
        self.text.truncate(self.text.len() - n);
        if self.spans.is_empty() {
            return;
        }
        let mut remaining = n;
        while !self.spans.is_empty() && remaining > 0 {
            let last = self.spans.len() - 1;
            if self.spans[last].len <= remaining {
                remaining -= self.spans[last].len;
                self.spans.pop();
            } else {
                self.spans[last].len -= remaining;
                remaining = 0;
            }
        }
        if self.spans.is_empty() {
            self.spans.push(Span::new(1, Style::new()));
        }
    }

    /// Merges adjacent spans with equal styles and drops zero-sized spans.
    pub fn collapse_spans(&mut self) {
        let mut write = 0;
        for read in 0..self.spans.len() {
            if self.spans[read].len == 0 {
                continue;
            }
            if write > 0 && self.spans[write - 1].style == self.spans[read].style {
                self.spans[write - 1].len += self.spans[read].len;
            } else {
                self.spans[write] = self.spans[read];
                write += 1;
            }
        }
        self.spans.truncate(write);
    }
}

/// Borrowed string slice paired with one [`Style`].
#[derive(Debug, Clone, Copy)]
pub struct StyledStr<'a> {
    /// Style applied to the borrowed text.
    pub style: Style,
    /// The borrowed text.
    pub text: &'a str,
}

impl<'a> StyledStr<'a> {
    /// Wraps `text` with default styling.
    pub const fn new(text: &'a str) -> Self {
        Self {
            style: Style::new(),
            text,
        }
    }

    /// Enables bold.
    #[must_use]
    pub const fn bold(mut self) -> Self {
        self.style.set_bold(true);
        self
    }
    /// Enables italic.
    #[must_use]
    pub const fn italic(mut self) -> Self {
        self.style.set_italic(true);
        self
    }
    /// Enables reverse video.
    #[must_use]
    pub const fn reverse(mut self) -> Self {
        self.style.set_reverse(true);
        self
    }
    /// Enables strikethrough.
    #[must_use]
    pub const fn strikethrough(mut self) -> Self {
        self.style.set_strikethrough(true);
        self
    }
    /// Enables dim.
    #[must_use]
    pub const fn dim(mut self) -> Self {
        self.style.set_dim(true);
        self
    }
    /// Sets the underline shape.
    #[must_use]
    pub const fn underline(mut self, underline: UnderlineType) -> Self {
        self.style.set_underline(Some(underline));
        self
    }
    /// Sets the foreground color.
    #[must_use]
    pub const fn fg(mut self, color: Color) -> Self {
        self.style.fg = Some(color);
        self
    }
    /// Sets the background color.
    #[must_use]
    pub const fn bg(mut self, color: Color) -> Self {
        self.style.bg = Some(color);
        self
    }
    /// Replaces the entire style.
    #[must_use]
    pub const fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }
    /// Sets the foreground to [`Color::RED`].
    #[must_use]
    pub const fn red(self) -> Self {
        self.fg(Color::RED)
    }
    /// Sets the foreground to [`Color::BLUE`].
    #[must_use]
    pub const fn blue(self) -> Self {
        self.fg(Color::BLUE)
    }
    /// Sets the foreground to [`Color::GREEN`].
    #[must_use]
    pub const fn green(self) -> Self {
        self.fg(Color::GREEN)
    }
    /// Sets the foreground to [`Color::CYAN`].
    #[must_use]
    pub const fn cyan(self) -> Self {
        self.fg(Color::CYAN)
    }
    /// Sets the foreground to [`Color::MAGENTA`].
    #[must_use]
    pub const fn magenta(self) -> Self {
        self.fg(Color::MAGENTA)
    }
    /// Sets the foreground to [`Color::YELLOW`].
    #[must_use]
    pub const fn yellow(self) -> Self {
        self.fg(Color::YELLOW)
    }
    /// Sets the foreground to [`Color::BLACK`].
    #[must_use]
    pub const fn black(self) -> Self {
        self.fg(Color::BLACK)
    }
    /// Sets the foreground to [`Color::WHITE`].
    #[must_use]
    pub const fn white(self) -> Self {
        self.fg(Color::WHITE)
    }
    /// Sets the background to [`Color::RED`].
    #[must_use]
    pub const fn red_bg(self) -> Self {
        self.bg(Color::RED)
    }
    /// Sets the background to [`Color::BLUE`].
    #[must_use]
    pub const fn blue_bg(self) -> Self {
        self.bg(Color::BLUE)
    }
    /// Sets the background to [`Color::GREEN`].
    #[must_use]
    pub const fn green_bg(self) -> Self {
        self.bg(Color::GREEN)
    }
    /// Sets the background to [`Color::CYAN`].
    #[must_use]
    pub const fn cyan_bg(self) -> Self {
        self.bg(Color::CYAN)
    }
    /// Sets the background to [`Color::MAGENTA`].
    #[must_use]
    pub const fn magenta_bg(self) -> Self {
        self.bg(Color::MAGENTA)
    }
    /// Sets the background to [`Color::YELLOW`].
    #[must_use]
    pub const fn yellow_bg(self) -> Self {
        self.bg(Color::YELLOW)
    }
    /// Sets the background to [`Color::BLACK`].
    #[must_use]
    pub const fn black_bg(self) -> Self {
        self.bg(Color::BLACK)
    }
    /// Sets the background to [`Color::WHITE`].
    #[must_use]
    pub const fn white_bg(self) -> Self {
        self.bg(Color::WHITE)
    }
}

impl<'a> From<&'a str> for StyledStr<'a> {
    fn from(text: &'a str) -> Self {
        StyledStr::new(text)
    }
}

impl<'a> From<StyledStr<'a>> for StyledString {
    fn from(span: StyledStr<'a>) -> Self {
        let text = span.text.to_string();
        if span.style == Style::new() {
            return Self { text, spans: Vec::new() };
        }
        let len = text.len();
        Self {
            spans: vec![
                Span { style: span.style, len },
                Span::new(1, Style::new()),
            ],
            text,
        }
    }
}

/// Extension methods for borrowing `&str` as a styled [`StyledStr`].
pub trait Stylize {
    /// Wraps the slice and enables bold.
    fn bold(&self) -> StyledStr<'_>;
    /// Wraps the slice and enables italic.
    fn italic(&self) -> StyledStr<'_>;
    /// Wraps the slice and enables reverse video.
    fn reverse(&self) -> StyledStr<'_>;
    /// Wraps the slice and enables strikethrough.
    fn strikethrough(&self) -> StyledStr<'_>;
    /// Wraps the slice and enables dim.
    fn dim(&self) -> StyledStr<'_>;
    /// Wraps the slice and applies the given underline shape.
    fn underline(&self, underline: UnderlineType) -> StyledStr<'_>;
    /// Wraps the slice and sets the foreground color.
    fn fg(&self, color: Color) -> StyledStr<'_>;
    /// Wraps the slice and sets the background color.
    fn bg(&self, color: Color) -> StyledStr<'_>;
    /// Wraps the slice with [`Color::RED`] foreground.
    fn red(&self) -> StyledStr<'_>;
    /// Wraps the slice with [`Color::BLUE`] foreground.
    fn blue(&self) -> StyledStr<'_>;
    /// Wraps the slice with [`Color::GREEN`] foreground.
    fn green(&self) -> StyledStr<'_>;
    /// Wraps the slice with [`Color::CYAN`] foreground.
    fn cyan(&self) -> StyledStr<'_>;
    /// Wraps the slice with [`Color::MAGENTA`] foreground.
    fn magenta(&self) -> StyledStr<'_>;
    /// Wraps the slice with [`Color::YELLOW`] foreground.
    fn yellow(&self) -> StyledStr<'_>;
    /// Wraps the slice with [`Color::BLACK`] foreground.
    fn black(&self) -> StyledStr<'_>;
    /// Wraps the slice with [`Color::WHITE`] foreground.
    fn white(&self) -> StyledStr<'_>;
    /// Wraps the slice with [`Color::RED`] background.
    fn red_bg(&self) -> StyledStr<'_>;
    /// Wraps the slice with [`Color::BLUE`] background.
    fn blue_bg(&self) -> StyledStr<'_>;
    /// Wraps the slice with [`Color::GREEN`] background.
    fn green_bg(&self) -> StyledStr<'_>;
    /// Wraps the slice with [`Color::CYAN`] background.
    fn cyan_bg(&self) -> StyledStr<'_>;
    /// Wraps the slice with [`Color::MAGENTA`] background.
    fn magenta_bg(&self) -> StyledStr<'_>;
    /// Wraps the slice with [`Color::YELLOW`] background.
    fn yellow_bg(&self) -> StyledStr<'_>;
    /// Wraps the slice with [`Color::BLACK`] background.
    fn black_bg(&self) -> StyledStr<'_>;
    /// Wraps the slice with [`Color::WHITE`] background.
    fn white_bg(&self) -> StyledStr<'_>;
}

impl Stylize for str {
    fn bold(&self) -> StyledStr<'_> { StyledStr::new(self).bold() }
    fn italic(&self) -> StyledStr<'_> { StyledStr::new(self).italic() }
    fn reverse(&self) -> StyledStr<'_> { StyledStr::new(self).reverse() }
    fn strikethrough(&self) -> StyledStr<'_> { StyledStr::new(self).strikethrough() }
    fn dim(&self) -> StyledStr<'_> { StyledStr::new(self).dim() }
    fn underline(&self, underline: UnderlineType) -> StyledStr<'_> { StyledStr::new(self).underline(underline) }
    fn fg(&self, color: Color) -> StyledStr<'_> { StyledStr::new(self).fg(color) }
    fn bg(&self, color: Color) -> StyledStr<'_> { StyledStr::new(self).bg(color) }
    fn red(&self) -> StyledStr<'_> { StyledStr::new(self).red() }
    fn blue(&self) -> StyledStr<'_> { StyledStr::new(self).blue() }
    fn green(&self) -> StyledStr<'_> { StyledStr::new(self).green() }
    fn cyan(&self) -> StyledStr<'_> { StyledStr::new(self).cyan() }
    fn magenta(&self) -> StyledStr<'_> { StyledStr::new(self).magenta() }
    fn yellow(&self) -> StyledStr<'_> { StyledStr::new(self).yellow() }
    fn black(&self) -> StyledStr<'_> { StyledStr::new(self).black() }
    fn white(&self) -> StyledStr<'_> { StyledStr::new(self).white() }
    fn red_bg(&self) -> StyledStr<'_> { StyledStr::new(self).red_bg() }
    fn blue_bg(&self) -> StyledStr<'_> { StyledStr::new(self).blue_bg() }
    fn green_bg(&self) -> StyledStr<'_> { StyledStr::new(self).green_bg() }
    fn cyan_bg(&self) -> StyledStr<'_> { StyledStr::new(self).cyan_bg() }
    fn magenta_bg(&self) -> StyledStr<'_> { StyledStr::new(self).magenta_bg() }
    fn yellow_bg(&self) -> StyledStr<'_> { StyledStr::new(self).yellow_bg() }
    fn black_bg(&self) -> StyledStr<'_> { StyledStr::new(self).black_bg() }
    fn white_bg(&self) -> StyledStr<'_> { StyledStr::new(self).white_bg() }
}

/// Error returned when [`Style`]'s [`std::str::FromStr`] cannot parse a token.
#[derive(Debug)]
pub struct StyleParseError(String);
impl std::fmt::Display for StyleParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for StyleParseError {}
impl From<String> for StyleParseError {
    fn from(s: String) -> Self { Self(s) }
}
impl From<&str> for StyleParseError {
    fn from(s: &str) -> Self { Self(s.to_string()) }
}

/// Parses a style spec like `bold-red-on-blue` or `single-red-underline`.
/// Tokens are separated by dashes, underscores, or whitespace.
impl std::str::FromStr for Style {
    type Err = StyleParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        type Iter<'a> = dyn Iterator<Item = &'a str> + 'a;

        fn parse_color(iter: &mut Iter<'_>, token: &str) -> Result<Color, StyleParseError> {
            if token == "bright" {
                let next = iter.next().ok_or("'bright' missing colour")?;
                format!("bright-{next}")
                    .parse()
                    .map_err(|_| format!("invalid colour 'bright-{next}'").into())
            }
            else {
                token.parse().map_err(|_| format!("invalid colour '{token}'").into())
            }
        }

        fn parse_bg(iter: &mut Iter<'_>, style: &mut Style) -> Result<(), StyleParseError> {
            let mut next = iter.next().ok_or("'on' missing colour")?;
            if let Some(rest) = next.strip_suffix('%') {
                let pct: u8 = rest.parse().map_err(|_| format!("invalid blend '{next}'"))?;
                if pct > 100 {
                    return Err(format!("blend '{next}' out of range 0-100").into());
                }
                style.set_blend(Some(pct));
                next = iter.next().ok_or("blend missing colour")?;
            }
            style.bg = Some(parse_color(iter, next)?);
            Ok(())
        }

        fn parse_ul(
            iter: &mut Iter<'_>,
            line_style: UnderlineType,
            style: &mut Style,
        ) -> Result<(), StyleParseError> {
            let head = iter.next().ok_or("line-style not closed by 'underline'")?;
            style.underline = Some(line_style);
            if head != "underline" {
                style.underline_color = Some(parse_color(iter, head)?);
                let close = iter.next().ok_or("underline piece missing 'underline'")?;
                if close != "underline" {
                    return Err(format!("expected 'underline' after colour, got '{close}'").into());
                }
            }
            Ok(())
        }

        let mut iter = s
            .split(|c: char| c == '-' || c == '_' || c.is_whitespace())
            .filter(|t| !t.is_empty());

        let mut s = Style::new();

        while let Some(token) = iter.next() {
            match token {
                "bold" => s.set_bold(true),
                "italic" => s.set_italic(true),
                "dim" => s.set_dim(true),
                "strikethrough" => s.set_strikethrough(true),
                "reverse" => s.set_reverse(true),
                "single" => parse_ul(&mut iter, UnderlineType::Single, &mut s)?,
                "double" => parse_ul(&mut iter, UnderlineType::Double, &mut s)?,
                "curly" => parse_ul(&mut iter, UnderlineType::Curly, &mut s)?,
                "dotted" => parse_ul(&mut iter, UnderlineType::Dotted, &mut s)?,
                "dashed" => parse_ul(&mut iter, UnderlineType::Dashed, &mut s)?,
                "underline" => s.underline = Some(UnderlineType::Single),
                "on" => parse_bg(&mut iter, &mut s)?,
                "no" => {
                    let next = iter.next()
                        .ok_or("'no' expected an attribute")?;
                    match next {
                        "bold" => s.set_bold(false),
                        "italic" => s.set_italic(false),
                        "dim" => s.set_dim(false),
                        "strikethrough" => s.set_strikethrough(false),
                        "reverse" => s.set_reverse(false),
                        "underline" => {
                            s.underline = Some(UnderlineType::None);
                            s.underline_color = None;
                        },
                        _ => return Err(format!(
                            "'no' expected an attribute, got '{next}'").into())
                    }
                }
                "plain" => s = Style::plain(),
                _ => s.fg = Some(parse_color(&mut iter, token)?),
            }
        }

        Ok(s)
    }
}

impl From<&str> for StyledString {
    fn from(value: &str) -> Self {
        Self {
            text: value.to_string(),
            spans: Vec::new(),
        }
    }
}

impl From<String> for StyledString {
    fn from(value: String) -> Self {
        Self {
            text: value,
            spans: Vec::new(),
        }
    }
}

/// Streaming parser that carries [`Style`] state across successive ANSI input lines.
pub struct AnsiStyleParser {
    style: Style,
}

impl AnsiStyleParser {
    /// Creates a parser starting with default style state.
    pub const fn new() -> Self {
        Self {
            style: Style::new(),
        }
    }

    /// Consumes one line of ANSI-encoded text and returns its styled form.
    pub fn parse_line(&mut self, input: &str) -> StyledString {
        let mut text = String::new();
        let mut spans: Vec<Span> = Vec::new();
        let mut current_span_len: usize = 0;
        let mut changed = false;
        let bytes = input.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        while i < len {
            if bytes[i] == 0x1b && i + 1 < len && bytes[i + 1] == b'[' {
                changed = true;
                let seq_start = i + 2;
                let mut seq_end = seq_start;
                while seq_end < len && bytes[seq_end] != b'm' {
                    seq_end += 1;
                }
                if seq_end >= len {
                    text.push_str(&input[i..]);
                    current_span_len += len - i;
                    i = len;
                    continue;
                }
                if current_span_len > 0 {
                    spans.push(Span {
                        style: self.style,
                        len: current_span_len,
                    });
                    current_span_len = 0;
                }
                let params_str = &input[seq_start..seq_end];
                let params: Vec<u16> = if params_str.is_empty() {
                    vec![0]
                } else {
                    params_str
                        .split(';')
                        .map(|s| s.parse::<u16>().unwrap_or(0))
                        .collect()
                };
                let mut p = 0;
                while p < params.len() {
                    match params[p] {
                        0 => self.style = Style::new(),
                        1 => self.style.set_bold(true),
                        2 => self.style.set_dim(true),
                        3 => self.style.set_italic(true),
                        4 => self
                            .style
                            .set_underline(Some(UnderlineType::Single)),
                        7 => self.style.set_reverse(true),
                        9 => self.style.set_strikethrough(true),
                        22 => {
                            self.style.set_bold(false);
                            self.style.set_dim(false);
                        }
                        23 => self.style.set_italic(false),
                        24 => self.style.set_underline(None),
                        27 => self.style.set_reverse(false),
                        29 => self.style.set_strikethrough(false),
                        30..=37 => {
                            self.style.fg =
                                Some(Color::Base256((params[p] - 30) as u8))
                        }
                        39 => self.style.fg = None,
                        40..=47 => {
                            self.style.bg =
                                Some(Color::Base256((params[p] - 40) as u8))
                        }
                        49 => self.style.bg = None,
                        90..=97 => {
                            self.style.fg =
                                Some(Color::Base256((params[p] - 90 + 8) as u8))
                        }
                        100..=107 => {
                            self.style.bg = Some(Color::Base256(
                                (params[p] - 100 + 8) as u8,
                            ))
                        }
                        38 => {
                            if p + 1 < params.len() {
                                match params[p + 1] {
                                    5 => {
                                        if p + 2 < params.len() {
                                            self.style.fg =
                                                Some(Color::Base256(
                                                    params[p + 2] as u8,
                                                ));
                                            p += 2;
                                        }
                                    }
                                    2 => {
                                        if p + 4 < params.len() {
                                            self.style.fg = Some(Color::Rgb(
                                                params[p + 2] as u8,
                                                params[p + 3] as u8,
                                                params[p + 4] as u8,
                                            ));
                                            p += 4;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        48 => {
                            if p + 1 < params.len() {
                                match params[p + 1] {
                                    5 => {
                                        if p + 2 < params.len() {
                                            self.style.bg =
                                                Some(Color::Base256(
                                                    params[p + 2] as u8,
                                                ));
                                            p += 2;
                                        }
                                    }
                                    2 => {
                                        if p + 4 < params.len() {
                                            self.style.bg = Some(Color::Rgb(
                                                params[p + 2] as u8,
                                                params[p + 3] as u8,
                                                params[p + 4] as u8,
                                            ));
                                            p += 4;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                    p += 1;
                }
                i = seq_end + 1;
            } else {
                let char_len = if bytes[i] < 0x80 {
                    1
                } else if bytes[i] < 0xE0 {
                    2
                } else if bytes[i] < 0xF0 {
                    3
                } else {
                    4
                };
                let end = (i + char_len).min(len);
                text.push_str(&input[i..end]);
                current_span_len += end - i;
                i = end;
            }
        }

        if spans.is_empty() && self.style == Style::new() && !changed {
            return StyledString { text, spans: Vec::new() };
        }

        spans.push(Span {
            style: self.style,
            len: current_span_len,
        });

        StyledString { text, spans }
    }
}
