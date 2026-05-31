//! Rendering primitives. Colors, styles, borders, cursors, and the cell-grid renderer.

pub mod border;
pub mod color;
pub mod cursor;
#[cfg(feature = "images")]
pub mod image;
pub mod style;
pub mod underline;

use crate::ansi;
use crate::prelude::*;
use crate::render::style::StyleAttribute;
use std::io::Write;
use unicode_segmentation::UnicodeSegmentation;

trait Emitter {
    fn full_clear(&mut self) -> std::io::Result<()>;
    fn line_wrap(&mut self, enable: bool) -> std::io::Result<()>;
    fn move_to(&mut self, col: u16, row: u16) -> std::io::Result<()>;
    fn move_to_column(&mut self, col: u16) -> std::io::Result<()>;
    fn move_left(&mut self, n: u16) -> std::io::Result<()>;
    fn apply_style(&mut self, style: &GridCellStyle) -> std::io::Result<()>;
    fn print(&mut self, s: &str) -> std::io::Result<()>;
    fn reset(&mut self) -> std::io::Result<()>;
}

struct AnsiEmitter<'a> {
    buf: &'a mut String,
    active: GridCellStyle,
}

impl<'a> AnsiEmitter<'a> {
    fn new(buf: &'a mut String) -> Self {
        buf.clear();
        Self { buf, active: GridCellStyle::DEFAULT }
    }
}

impl<'a> Emitter for AnsiEmitter<'a> {
    #[inline]
    fn full_clear(&mut self) -> std::io::Result<()> {
        ansi::clear_screen(self.buf);
        Ok(())
    }
    #[inline]
    fn line_wrap(&mut self, enable: bool) -> std::io::Result<()> {
        if enable {
            ansi::enable_line_wrap(self.buf);
        } else {
            ansi::disable_line_wrap(self.buf);
        }
        Ok(())
    }
    #[inline]
    fn move_to(&mut self, col: u16, row: u16) -> std::io::Result<()> {
        ansi::move_to(self.buf, col, row);
        Ok(())
    }
    #[inline]
    fn move_to_column(&mut self, col: u16) -> std::io::Result<()> {
        ansi::move_to_column(self.buf, col);
        Ok(())
    }
    #[inline]
    fn move_left(&mut self, n: u16) -> std::io::Result<()> {
        ansi::move_left(self.buf, n);
        Ok(())
    }
    #[inline]
    fn apply_style(&mut self, style: &GridCellStyle) -> std::io::Result<()> {
        #[cfg(feature = "harmonious")]
        let style = &{
            let mut s = *style;
            s.fg = crate::theme::harmonious::resolve_color(s.fg);
            s.bg = crate::theme::harmonious::resolve_color(s.bg);
            s.underline_color = crate::theme::harmonious::resolve_color(s.underline_color);
            s
        };
        if self.active != *style {
            ansi::write_style_diff(self.buf, &self.active, style);
            self.active = *style;
        }
        Ok(())
    }
    #[inline]
    fn print(&mut self, s: &str) -> std::io::Result<()> {
        self.buf.push_str(s);
        Ok(())
    }
    #[inline]
    fn reset(&mut self) -> std::io::Result<()> {
        ansi::reset_attrs(self.buf);
        self.active = GridCellStyle::DEFAULT;
        Ok(())
    }
}

fn resolve_rgb(color: Color) -> Option<(u8, u8, u8)> {
    match color {
        Color::Rgb(r, g, b) => Some((r, g, b)),
        _ => {
            #[cfg(feature = "harmonious")]
            {
                crate::theme::harmonious::resolve_rgb(color).map(|c| (c.r, c.g, c.b))
            }
            #[cfg(not(feature = "harmonious"))]
            {
                None
            }
        }
    }
}

fn resolve_dim(style: &mut GridCellStyle) {
    if style.attrs & StyleAttribute::Dim as u8 != 0 {
        style.fg = lerp_color(style.fg, style.bg, 50);
        style.attrs &= !(StyleAttribute::Dim as u8);
    }
}

fn lerp_color(from: Color, to: Color, t: u8) -> Color {
    let from_rgb = resolve_rgb(from);
    let to_rgb = resolve_rgb(to);
    match (from_rgb, to_rgb) {
        (Some((fr, fg, fb)), Some((tr, tg, tb))) => {
            let t = t as u16;
            let inv = 100 - t;
            Color::Rgb(
                ((fr as u16 * inv + tr as u16 * t) / 100) as u8,
                ((fg as u16 * inv + tg as u16 * t) / 100) as u8,
                ((fb as u16 * inv + tb as u16 * t) / 100) as u8,
            )
        }
        _ => {
            if t >= 50 {
                to
            } else {
                from
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct GridCellStyle {
    pub fg: Color,
    pub bg: Color,
    pub underline_color: Color,
    pub underline: UnderlineType,
    pub attrs: u8,
}

impl GridCellStyle {
    const DEFAULT: Self = Self {
        fg: Color::Foreground,
        bg: Color::Background,
        underline_color: Color::Foreground,
        underline: UnderlineType::None,
        attrs: 0,
    };

    fn apply(&mut self, style: &Style) {
        if let Some(fg) = style.get_fg() {
            self.fg = fg;
        }
        if let Some(bg) = style.get_bg() {
            self.bg = bg;
        }
        if let Some(uc) = style.get_underline_color() {
            self.underline_color = uc;
        }
        if let Some(u) = style.get_underline() {
            self.underline = u;
        }
        let mask = style.get_attrs_mask();
        self.attrs = (style.get_attrs_bits() & mask) | (self.attrs & !mask);
    }

    pub(crate) fn has_reverse(&self) -> bool {
        self.attrs & StyleAttribute::Reverse as u8 != 0
    }

    fn visible_bg(&self) -> Color {
        if self.has_reverse() {
            self.fg
        } else {
            self.bg
        }
    }

    fn set_visible_bg(&mut self, color: Color) {
        if self.has_reverse() {
            self.fg = color;
        } else {
            self.bg = color;
        }
    }
}

#[derive(Clone, Copy, Default)]
struct GridCellFlags(u8);

impl GridCellFlags {
    const WIDE: u8 = 1 << 0;
    const INVALID: u8 = 1 << 1;
    const DIRTY: u8 = 1 << 2;

    fn get(self, bit: u8) -> bool {
        self.0 & bit != 0
    }

    fn set(&mut self, bit: u8, value: bool) {
        if value {
            self.0 |= bit;
        } else {
            self.0 &= !bit;
        }
    }
}

#[derive(Clone, Copy)]
union GlyphData {
    bytes: [u8; 4],
    index: u32,
}

/// One cell in the renderer's grid.
#[derive(Clone, Copy)]
pub struct GridCell {
    style: GridCellStyle,
    flags: GridCellFlags,
    glyph_len: u8,
    glyph: GlyphData,
}

impl GridCell {
    const DEFAULT: Self = Self {
        style: GridCellStyle::DEFAULT,
        flags: GridCellFlags(0),
        glyph_len: 1,
        glyph: GlyphData { bytes: [b' ', 0, 0, 0] },
    };

    fn glyph<'a>(&'a self, graphemes: &'a [u8]) -> &'a str {
        let len = self.glyph_len as usize;
        if len <= 4 {
            unsafe { std::str::from_utf8_unchecked(&self.glyph.bytes[..len]) }
        } else {
            let idx = unsafe { self.glyph.index } as usize;
            unsafe { std::str::from_utf8_unchecked(&graphemes[idx..idx + len]) }
        }
    }

    fn set_glyph(&mut self, s: &str, graphemes: &mut Vec<u8>) {
        let len = s.len();
        if len <= 4 {
            unsafe { self.glyph.bytes[..len].copy_from_slice(s.as_bytes()) };
        } else {
            let idx = graphemes.len();
            graphemes.extend_from_slice(s.as_bytes());
            self.glyph.index = idx as u32;
        }
        self.glyph_len = len as u8;
    }

    fn set_glyph_char(&mut self, c: char) {
        let len = c.len_utf8();
        c.encode_utf8(unsafe { &mut self.glyph.bytes });
        self.glyph_len = len as u8;
    }

    fn eq(&self, other: &GridCell, self_graphemes: &[u8], other_graphemes: &[u8]) -> bool {
        self.style == other.style
            && self.flags.get(GridCellFlags::WIDE) == other.flags.get(GridCellFlags::WIDE)
            && self.glyph_len == other.glyph_len
            && self.glyph(self_graphemes) == other.glyph(other_graphemes)
    }

    /// Sets the glyph from a single char without grapheme or width measurement.
    ///
    /// # Safety
    ///
    /// `c` must be a single-cell-wide non-control character.
    #[inline]
    pub unsafe fn set_glyph_unchecked(&mut self, c: char) {
        self.set_glyph_char(c);
    }

    /// Sets the glyph by recording an `idx`/`len` pair into the grapheme buffer.
    ///
    /// # Safety
    ///
    /// `len` must be `> 4`, `[idx, idx+len)` must be a valid single-cell-wide UTF-8 cluster in the
    /// grapheme buffer, and the `WIDE` flag must be correct.
    #[inline]
    pub unsafe fn set_glyph_indexed_unchecked(&mut self, idx: u32, len: u8) {
        self.glyph.index = idx;
        self.glyph_len = len;
    }

    /// Clears the `WIDE` and `INVALID` flags on this cell.
    #[inline]
    pub fn mark_overwritten(&mut self) {
        let was_invalid = self.flags.0 & GridCellFlags::INVALID != 0;
        self.flags.0 &= !(GridCellFlags::WIDE | GridCellFlags::INVALID);
        if was_invalid {
            self.flags.0 |= GridCellFlags::DIRTY;
        }
    }

    #[inline]
    fn mark_dirty(&mut self) {
        let was_invalid = self.flags.0 & GridCellFlags::INVALID != 0;
        self.flags.0 &= !GridCellFlags::INVALID;
        if was_invalid {
            self.flags.0 |= GridCellFlags::DIRTY;
        }
    }
}

/// Row-scoped writer obtained from [`RenderContext::row_writer`].
pub struct RowWriter<'a> {
    cells: &'a mut [GridCell],
    graphemes: &'a mut Vec<u8>,
    skip: usize,
    sink: [GridCell; 2],
}

impl<'a> RowWriter<'a> {
    /// Returns the range of writable logical column indices relative to the cursor.
    #[inline]
    pub fn get_range(&self) -> std::ops::Range<usize> {
        self.skip..(self.skip + self.cells.len())
    }

    /// Returns a writer for the cell at logical column `col` (cursor-relative).
    #[inline]
    pub fn cell(&mut self, col: usize) -> CellWriter<'_> {
        if col < self.skip || col >= self.skip + self.cells.len() {
            return CellWriter {
                cells: &mut self.sink[..],
                graphemes: &mut *self.graphemes,
                idx: 0,
                wide: false,
            };
        }
        let idx = col - self.skip;
        let wide = self.cells[idx].flags.0 & GridCellFlags::WIDE != 0;
        CellWriter {
            cells: &mut *self.cells,
            graphemes: &mut *self.graphemes,
            idx,
            wide,
        }
    }

    /// Fills every visible cell with `glyph` styled by `style`.
    #[inline]
    pub fn fill(&mut self, glyph: &str, style: &Style) {
        if self.cells.is_empty() {
            return;
        }
        let control_buf;
        let glyph = if glyph.len() == 1 && glyph.as_bytes()[0].is_ascii_control() {
            control_buf = [b'^', ((glyph.as_bytes()[0] as u32 + 64) % 128) as u8];
            unsafe { std::str::from_utf8_unchecked(&control_buf) }
        } else {
            glyph
        };
        let wide = terminal_grapheme_width(glyph) > 1;
        let mut template = GridCell::DEFAULT;
        template.set_glyph(glyph, self.graphemes);
        template.style.apply(style);
        if wide {
            template.flags.0 |= GridCellFlags::WIDE;
        }
        let mut space = GridCell::DEFAULT;
        space.style = template.style;

        let write = |cell: &mut GridCell, src: &GridCell| {
            let was_invalid = cell.flags.0 & GridCellFlags::INVALID != 0;
            *cell = *src;
            if was_invalid {
                cell.flags.0 |= GridCellFlags::DIRTY;
            }
        };

        if !wide {
            for cell in self.cells.iter_mut() {
                write(cell, &template);
            }
        } else {
            let mut chunks = self.cells.chunks_exact_mut(2);
            for pair in &mut chunks {
                write(&mut pair[0], &template);
                write(&mut pair[1], &space);
            }
            if let Some(cell) = chunks.into_remainder().first_mut() {
                write(cell, &space);
            }
        }
    }

    /// Writes `text` from logical column `start_col`, returning the column past the last grapheme.
    #[inline]
    pub fn text(&mut self, start_col: usize, text: &str) -> usize {
        let range = self.get_range();
        if text.is_empty() || start_col >= range.end {
            return start_col;
        }
        if start_col > range.start {
            let prev_idx = start_col - self.skip - 1;
            let prev = &mut self.cells[prev_idx];
            if prev.flags.0 & GridCellFlags::WIDE != 0 {
                prev.flags.0 &= !GridCellFlags::WIDE;
                prev.set_glyph_char(' ');
            }
        }
        let mut col = start_col;
        for grapheme in text.graphemes(true) {
            if col >= range.end {
                break;
            }
            let c = grapheme.chars().next().unwrap();
            if c == '\n' {
                break;
            }
            let w = terminal_grapheme_width(grapheme);
            self.cell(col).grapheme(grapheme);
            col += w;
        }
        col
    }
}

/// Per-cell writer returned by [`RowWriter::cell`].
pub struct CellWriter<'a> {
    cells: &'a mut [GridCell],
    graphemes: &'a mut Vec<u8>,
    idx: usize,
    wide: bool,
}

impl CellWriter<'_> {
    /// Writes a single-char glyph at this cell.
    #[inline]
    pub fn glyph(&mut self, c: char) -> &mut Self {
        if c.is_ascii_control() {
            let buf = [b'^', ((u32::from(c) + 64) % 128) as u8];
            let s = unsafe { std::str::from_utf8_unchecked(&buf) };
            return unsafe { self.grapheme_unchecked(true, s) };
        }
        let wide = unicode_display_width::is_double_width(c) || c == '\u{FE0F}';
        unsafe { self.glyph_unchecked(wide, c) }
    }

    /// Writes a single-char glyph at this cell with caller-asserted width.
    ///
    /// # Safety
    ///
    /// `wide` must match `c`'s terminal display width and `c` must be non-control.
    #[inline]
    pub unsafe fn glyph_unchecked(&mut self, wide: bool, c: char) -> &mut Self {
        if wide && self.idx + 1 >= self.cells.len() {
            let was_wide = self.before_glyph(false);
            self.cells[self.idx].set_glyph_char(' ');
            self.after_glyph(false, was_wide);
            return self;
        }
        let was_wide = self.before_glyph(wide);
        self.cells[self.idx].set_glyph_char(c);
        self.after_glyph(wide, was_wide);
        self
    }

    /// Writes a multi-byte grapheme at this cell.
    #[inline]
    pub fn grapheme(&mut self, s: &str) -> &mut Self {
        if s.len() == 1 && s.as_bytes()[0].is_ascii_control() {
            return self.glyph(s.as_bytes()[0] as char);
        }
        let wide = terminal_grapheme_width(s) > 1;
        unsafe { self.grapheme_unchecked(wide, s) }
    }

    /// Writes a multi-byte grapheme at this cell with caller-asserted width.
    ///
    /// # Safety
    ///
    /// `wide` must match the grapheme's terminal display width and `s` must be a single cluster.
    #[inline]
    pub unsafe fn grapheme_unchecked(&mut self, wide: bool, s: &str) -> &mut Self {
        if wide && self.idx + 1 >= self.cells.len() {
            let was_wide = self.before_glyph(false);
            self.cells[self.idx].set_glyph_char(' ');
            self.after_glyph(false, was_wide);
            return self;
        }
        let was_wide = self.before_glyph(wide);
        self.cells[self.idx].set_glyph(s, self.graphemes);
        self.after_glyph(wide, was_wide);
        self
    }

    #[inline]
    fn before_glyph(&mut self, wide: bool) -> bool {
        let was_wide = self.wide;
        self.wide = wide;
        let cell = &mut self.cells[self.idx];
        cell.mark_overwritten();
        cell.style.bg = cell.style.visible_bg();
        cell.style.fg = Color::Foreground;
        cell.style.underline_color = Color::Foreground;
        cell.style.underline = UnderlineType::None;
        cell.style.attrs = 0;
        was_wide
    }

    #[inline]
    fn after_glyph(&mut self, wide: bool, was_wide: bool) {
        if wide {
            self.cells[self.idx].flags.0 |= GridCellFlags::WIDE;
        }
        match (wide, was_wide) {
            (false, false) | (true, true) => {}
            (true, false) => {
                let new_style = self.cells[self.idx].style;
                let next = &mut self.cells[self.idx + 1];
                next.mark_overwritten();
                next.set_glyph_char(' ');
                next.style = new_style;
            }
            (false, true) => {
                let next = &mut self.cells[self.idx + 1];
                next.mark_overwritten();
                next.set_glyph_char(' ');
            }
        }
    }

    /// Applies `style` to this cell.
    #[inline]
    pub fn style(&mut self, style: &Style) -> &mut Self {
        let cell = &mut self.cells[self.idx];
        match style.get_blend() {
            Some(b) if b < 100 => {
                let reversed = style.has_reverse();
                let overlay = style.get_overlay_color();
                resolve_dim(&mut cell.style);
                let prev = cell.style.visible_bg();
                cell.style.apply(style);
                resolve_dim(&mut cell.style);
                let blended = overlay.map_or(prev, |o| lerp_color(prev, o, b));
                if reversed {
                    cell.style.fg = blended;
                } else {
                    cell.style.bg = blended;
                }
            }
            _ => cell.style.apply(style),
        }
        cell.mark_dirty();
        if self.wide {
            let new_style = cell.style;
            let next = &mut self.cells[self.idx + 1];
            next.style = new_style;
            next.mark_dirty();
        }
        self
    }

    /// Blends `overlay_bg` into the visible background slot of this cell.
    #[inline]
    pub fn blend_visible_bg(&mut self, overlay_bg: Option<Color>, blend: u8) -> &mut Self {
        let Some(overlay_bg) = overlay_bg else {
            return self;
        };
        let cell = &mut self.cells[self.idx];
        resolve_dim(&mut cell.style);
        let prev = cell.style.visible_bg();
        cell.style.set_visible_bg(lerp_color(prev, overlay_bg, blend));
        cell.mark_dirty();
        if self.wide {
            let new_style = cell.style;
            let next = &mut self.cells[self.idx + 1];
            next.style = new_style;
            next.mark_dirty();
        }
        self
    }
}

pub(crate) struct CtxSnapshot {
    pub anchor: Vec2<i32>,
    pub position: Vec2<u16>,
    pub physical_size: Vec2<u16>,
    pub size: Vec2<u16>,
    pub viewport_pos: Vec2<u16>,
    pub viewport_size: Vec2<u16>,
    pub base: Style,
    pub style: Style,
}

#[cfg(feature = "gui")]
pub(crate) type OffsetCallback =
    Box<dyn for<'a> FnOnce(&'a dyn Widget, RenderContext<'a>) + 'static>;

pub(crate) enum Kind {
    #[cfg(feature = "gui")]
    Offset {
        viewport_size_cells: Vec2<u16>,
        content_offset_cells: Vec2<i32>,
        subcell_offset_px: Vec2<i32>,
        callback: OffsetCallback,
    },
    Z,
    Layer,
    Popup,
}

pub(crate) struct QueuedEntry {
    pub widget: *const dyn Widget,
    pub snapshot: CtxSnapshot,
    pub z: Layer,
    pub seq: u64,
    pub parent_screen_pos_px: Vec2<i32>,
    /// Viewport offset from parent's grid origin at queue time, in cells.
    /// Only stored for `Kind::Offset` entries — `parent_grid_origin_cells` is
    /// repurposed (set to 0) for those, so the queue-time origin isn't
    /// otherwise recoverable. Z/Layer/Popup entries derive this at read time
    /// from `snapshot.position - parent_grid_origin_cells`.
    #[cfg(feature = "gui")]
    pub cell_pos_in_parent: Vec2<i32>,
    pub parent_grid_origin_cells: Vec2<i32>,
    pub parent_clip_screen_px: Option<(Vec2<i32>, Vec2<u32>)>,
    pub kind: Kind,
}

fn kind_priority(kind: &Kind) -> u8 {
    match kind {
        #[cfg(feature = "gui")]
        Kind::Offset { .. } => 0,
        Kind::Z | Kind::Layer => 1,
        Kind::Popup => 2,
    }
}

#[cfg(feature = "gui")]
enum DeferDispatch {
    Callback(OffsetCallback),
    Widget,
}

#[derive(Clone, Copy)]
pub(crate) struct DrainCtx {
    pub entry_screen_pos_px: Vec2<i32>,
    pub grid_origin_cells: Vec2<i32>,
    pub parent_clip_screen_px: Option<(Vec2<i32>, Vec2<u32>)>,
}

/// Double-buffered cell grid backing [`GridRenderer`].
pub struct GridRendererState {
    cells: (Vec<GridCell>, Vec<GridCell>),
    graphemes: (Vec<u8>, Vec<u8>),
    size: Vec2<u16>,
    cells_stride: u16,
    cells_height: u16,
    full_dirty: bool,
    fmt_buf: String,
    pub(crate) defer_queue: Vec<QueuedEntry>,
    pub(crate) root_screen_pos_px: Vec2<i32>,
    pub(crate) root_clip_screen_px: Option<(Vec2<i32>, Vec2<u32>)>,
    pub(crate) drain_ctx: DrainCtx,
    pub(crate) seq_counter: u64,
    pub(crate) active_seq: Option<u64>,
}

#[cfg(feature = "gui")]
const SCRATCH_SLACK: u16 = 2;
#[cfg(not(feature = "gui"))]
const SCRATCH_SLACK: u16 = 0;

pub(crate) fn intersect_clip(
    a: (Vec2<i32>, Vec2<u32>),
    b: (Vec2<i32>, Vec2<u32>),
) -> (Vec2<i32>, Vec2<u32>) {
    let a_end = Vec2::new(a.0.x + a.1.x as i32, a.0.y + a.1.y as i32);
    let b_end = Vec2::new(b.0.x + b.1.x as i32, b.0.y + b.1.y as i32);
    let origin = Vec2::new(a.0.x.max(b.0.x), a.0.y.max(b.0.y));
    let end = Vec2::new(a_end.x.min(b_end.x), a_end.y.min(b_end.y));
    let size = Vec2::new(
        (end.x - origin.x).max(0) as u32,
        (end.y - origin.y).max(0) as u32,
    );
    (origin, size)
}

#[inline]
fn glyph_dirty_cols(glyph: &str, is_wide: bool, current: i32) -> i32 {
    let glyph_len = glyph.len();
    if glyph_len <= 1 {
        return current;
    }
    let first = glyph.as_bytes()[0];
    let first_char_len = if first < 0x80 {
        1
    } else if first < 0xE0 {
        2
    } else if first < 0xF0 {
        3
    } else {
        4
    };
    if first_char_len == glyph_len {
        if is_wide {
            std::cmp::max(current, 2)
        } else {
            current
        }
    } else {
        let num_chars = glyph.chars().count();
        std::cmp::max(current, (num_chars * 2) as i32)
    }
}

/// Returns the terminal cell width of a single grapheme cluster.
pub fn terminal_grapheme_width(grapheme: &str) -> usize {
    let bytes = grapheme.as_bytes();
    if bytes.len() == 1 && bytes[0] < 0x80 {
        return if bytes[0] < 0x20 || bytes[0] == 0x7F {
            2
        } else {
            1
        };
    }
    if grapheme.chars().any(|ch| {
        ch == '\u{FE0F}'
            || ch.is_ascii_control()
            || unicode_display_width::is_double_width(ch)
    }) {
        2
    } else {
        1
    }
}

/// Returns the sum of [`terminal_grapheme_width`] for every grapheme in `text`.
pub fn terminal_display_width(text: &str) -> usize {
    text.graphemes(true)
        .fold(0, |acc, grapheme| acc + terminal_grapheme_width(grapheme))
}

impl GridRendererState {
    /// Creates an empty state with a zero-sized grid.
    pub fn new() -> Self {
        Self {
            full_dirty: true,
            cells: (Vec::new(), Vec::new()),
            graphemes: (Vec::new(), Vec::new()),
            size: Vec2::of(0),
            cells_stride: 0,
            cells_height: 0,
            fmt_buf: String::new(),
            defer_queue: Vec::new(),
            root_screen_pos_px: Vec2::of(0i32),
            root_clip_screen_px: None,
            drain_ctx: DrainCtx {
                entry_screen_pos_px: Vec2::of(0i32),
                grid_origin_cells: Vec2::of(0i32),
                parent_clip_screen_px: None,
            },
            seq_counter: 0,
            active_seq: None,
        }
    }

    fn next_seq(&mut self) -> u64 {
        match self.active_seq {
            Some(s) => s,
            None => {
                self.seq_counter += 1;
                self.seq_counter
            }
        }
    }

    fn render_with<E: Emitter>(&mut self, emit: &mut E) -> std::io::Result<()> {
        if self.full_dirty {
            self.full_dirty = false;
            emit.full_clear()?;
        }
        let mut i = 0;
        let cells = &self.cells.0;
        let prev = &self.cells.1;
        let graphemes = &self.graphemes.0;
        let prev_graphemes = &self.graphemes.1;
        let mut should_move = true;
        let mut current_line: u16 = u16::MAX;
        let mut dirty_cols: i32 = 0;

        let mut linewrap = true;
        emit.line_wrap(true)?;

        for y in 0..self.size.y {
            let mut x = 0;
            let mut cols_until_last_cell = (self.size.y - y) as i32
                * (self.size.x as i32 - 1);
            while x < self.size.x {
                if !cells[i].flags.get(GridCellFlags::INVALID)
                    && (!cells[i].eq(&prev[i], graphemes, prev_graphemes)
                        || dirty_cols > 0
                        || i > 0 && prev[i - 1].flags.get(GridCellFlags::WIDE) && !cells[i - 1].flags.get(GridCellFlags::WIDE)
                        || cells[i].flags.get(GridCellFlags::DIRTY)
                        || prev[i].flags.get(GridCellFlags::INVALID))
                {
                    emit.apply_style(&cells[i].style)?;
                    if should_move {
                        if y == current_line {
                            emit.move_to_column(x)?;
                        } else {
                            emit.move_to(x, y)?;
                        }
                        should_move = false;
                    }
                    if cells[i].flags.get(GridCellFlags::WIDE) {
                        emit.print("  ")?;
                        if x < self.size.x - 2 {
                            emit.move_left(2)?;
                        } else {
                            emit.move_to(x, y)?;
                        }
                        should_move = true;
                    }
                    let glyph = cells[i].glyph(graphemes);
                    dirty_cols = glyph_dirty_cols(
                        glyph,
                        cells[i].flags.get(GridCellFlags::WIDE),
                        dirty_cols,
                    );
                    if linewrap && dirty_cols > cols_until_last_cell {
                        linewrap = false;
                        emit.line_wrap(false)?;
                    }
                    emit.print(glyph)?;
                    current_line = y;
                } else {
                    should_move = true;
                }
                let width = if cells[i].flags.get(GridCellFlags::WIDE) { 2 } else { 1 };
                if x as i32 + dirty_cols >= self.size.x as i32 {
                    current_line = u16::MAX;
                }
                dirty_cols -= width as i32;
                cols_until_last_cell -= width as i32;
                x += width as u16;
                i += width as usize;
            }
            should_move = true;
            emit.reset()?;
        }
        if !linewrap {
            emit.line_wrap(true)?;
        }
        std::mem::swap(&mut self.cells.0, &mut self.cells.1);
        std::mem::swap(&mut self.graphemes.0, &mut self.graphemes.1);
        self.graphemes.0.clear();
        for cell in &mut self.cells.0 {
            cell.flags.set(GridCellFlags::INVALID, false);
            cell.flags.set(GridCellFlags::DIRTY, false);
        }
        emit.reset()?;

        Ok(())
    }
}

/// Widget-tree renderer backed by a [`GridRendererState`].
pub struct GridRenderer {
    state: GridRendererState,
}

impl GridRenderer {
    /// Creates a renderer with an empty grid.
    pub fn new() -> Self {
        Self {
            state: GridRendererState::new(),
        }
    }

    /// Resets both cell buffers to defaults and forces a full redraw on the next [`flush`](Self::flush).
    pub fn clear(&mut self) {
        self.state.full_dirty = true;
        self.state.cells.0.fill(GridCell::DEFAULT);
        self.state.cells.1.fill(GridCell::DEFAULT);
    }

    /// Resizes both cell buffers to `size`.
    pub fn resize(&mut self, size: Vec2<u16>) {
        let state = &mut self.state;
        if state.size == size {
            return;
        }
        state.full_dirty = true;
        state.size = size;
        state.cells_stride = size.x.saturating_add(SCRATCH_SLACK);
        state.cells_height = size.y.saturating_add(SCRATCH_SLACK);
        state.cells.0.clear();
        state.cells.1.clear();
        state
            .cells
            .0
            .resize(
                state.cells_stride as usize * state.cells_height as usize,
                GridCell::DEFAULT,
            );
        state
            .cells
            .1
            .resize(
                state.cells_stride as usize * state.cells_height as usize,
                GridCell::DEFAULT,
            );
    }

    pub(crate) fn render_to_queue(
        &mut self,
        widget: &dyn Widget,
        offset: Vec2<i32>,
        raw_writer: &mut dyn Write,
    ) {
        let mut ctx = self.context(raw_writer);
        ctx.set_style(Style::new().bg(Color::Background));
        ctx.fill(" ");
        ctx.set_style(Style::new());
        ctx.render_child(widget, offset);
    }

    pub(crate) fn drain_queue(&mut self, raw_writer: &mut dyn Write) {
        while let Some(entry) = self.pop_defer_entry() {
            // SAFETY: see `QueuedEntry`'s safety invariant. The pointer was
            // captured at queue time from a live `&dyn Widget`, the queue is
            // cleared per paint and fully drained inside the same paint
            // pass, and nothing mutates the widget tree between push and
            // drain.
            let child: &dyn Widget = unsafe { &*entry.widget };
            let prev_drain = self.state.drain_ctx;
            self.state.drain_ctx = DrainCtx {
                entry_screen_pos_px: entry.parent_screen_pos_px,
                grid_origin_cells: entry.parent_grid_origin_cells,
                parent_clip_screen_px: entry.parent_clip_screen_px,
            };
            let ctx = RenderContext {
                state: &mut self.state,
                raw_writer: &mut *raw_writer,
                anchor: entry.snapshot.anchor,
                cursor: Vec2::of(0i32),
                position: entry.snapshot.position,
                physical_size: entry.snapshot.physical_size,
                size: entry.snapshot.size,
                viewport_pos: entry.snapshot.viewport_pos,
                viewport_size: entry.snapshot.viewport_size,
                base: entry.snapshot.base,
                style: entry.snapshot.style,
            };
            child.render(ctx);
            self.state.drain_ctx = prev_drain;
        }
    }

    /// Returns the root [`RenderContext`] covering the full grid.
    pub fn context<'a>(&'a mut self, raw_writer: &'a mut dyn Write) -> RenderContext<'a> {
        let base_style = Style::new();
        let screen = self.state.size;
        let scratch = Vec2::new(self.state.cells_stride, self.state.cells_height);
        self.state.drain_ctx = DrainCtx {
            entry_screen_pos_px: self.state.root_screen_pos_px,
            grid_origin_cells: Vec2::of(0i32),
            parent_clip_screen_px: self.state.root_clip_screen_px,
        };
        RenderContext {
            size: screen,
            physical_size: scratch,
            viewport_pos: Vec2::of(0u16),
            viewport_size: screen,
            state: &mut self.state,
            raw_writer,
            cursor: Vec2::of(0i32),
            base: base_style,
            style: base_style,
            position: Vec2::of(0u16),
            anchor: Vec2::of(0i32),
        }
    }

    /// Diffs the cell buffers and writes ANSI escapes to `buffer`.
    pub fn flush(&mut self, buffer: &mut dyn Write) -> std::io::Result<()> {
        let mut buf = std::mem::take(&mut self.state.fmt_buf);
        let mut emitter = AnsiEmitter::new(&mut buf);
        let r = self.state.render_with(&mut emitter);
        buffer.write_all(buf.as_bytes())?;
        self.state.fmt_buf = buf;
        r
    }

    /// Returns and clears the full-dirty flag.
    pub fn take_full_dirty(&mut self) -> bool {
        std::mem::replace(&mut self.state.full_dirty, false)
    }

    #[cfg(feature = "gui")]
    pub(crate) fn gui_size(&self) -> Vec2<u16> {
        self.state.size
    }

    #[cfg(feature = "gui")]
    pub(crate) fn set_root_screen_pos_px(&mut self, pos: Vec2<i32>) {
        self.state.root_screen_pos_px = pos;
    }

    #[cfg(feature = "gui")]
    pub(crate) fn set_root_clip_screen_px(&mut self, rect: (Vec2<i32>, Vec2<u32>)) {
        self.state.root_clip_screen_px = Some(rect);
    }

    pub(crate) fn pop_defer_entry(&mut self) -> Option<QueuedEntry> {
        if self.state.defer_queue.is_empty() {
            return None;
        }
        let (min_idx, _) = self
            .state
            .defer_queue
            .iter()
            .enumerate()
            .min_by_key(|(idx, e)| (kind_priority(&e.kind), e.z, e.seq, *idx))
            .unwrap();
        Some(self.state.defer_queue.remove(min_idx))
    }

    pub(crate) fn clear_defer_queue(&mut self) {
        self.state.defer_queue.clear();
        self.state.seq_counter = 0;
        self.state.active_seq = None;
    }

    #[cfg(feature = "gui")]
    pub(crate) fn render_defer_entry(
        &mut self,
        widget: &dyn Widget,
        entry: QueuedEntry,
        screen_pos_px: Vec2<i32>,
        clip_screen_px: Option<(Vec2<i32>, Vec2<u32>)>,
        raw_writer: &mut dyn Write,
    ) {
        let phys = entry.snapshot.physical_size;
        let grid_w = self.state.cells_stride as usize;
        for y in 0..phys.y {
            for x in 0..phys.x {
                let i = y as usize * grid_w + x as usize;
                if i < self.state.cells.0.len() {
                    self.state.cells.0[i] = GridCell::DEFAULT;
                }
            }
        }

        let snap_pos = entry.snapshot.position;
        let snap_pos_i = Vec2::new(snap_pos.x as i32, snap_pos.y as i32);
        let prev_active_seq = self.state.active_seq.replace(entry.seq);
        let prev_drain = self.state.drain_ctx;
        self.state.drain_ctx = DrainCtx {
            entry_screen_pos_px: screen_pos_px,
            grid_origin_cells: snap_pos_i,
            parent_clip_screen_px: clip_screen_px,
        };
        let anchor = entry.snapshot.anchor;
        let dispatch = match entry.kind {
            Kind::Offset { callback, .. } => DeferDispatch::Callback(callback),
            Kind::Z | Kind::Layer | Kind::Popup => DeferDispatch::Widget,
        };
        let ctx = RenderContext {
            state: &mut self.state,
            raw_writer: &mut *raw_writer,
            anchor,
            cursor: Vec2::of(0i32),
            position: snap_pos,
            physical_size: entry.snapshot.physical_size,
            size: entry.snapshot.size,
            viewport_pos: snap_pos,
            viewport_size: entry.snapshot.physical_size,
            base: entry.snapshot.base,
            style: entry.snapshot.style,
        };
        match dispatch {
            DeferDispatch::Callback(cb) => cb(widget, ctx),
            DeferDispatch::Widget => widget.render(ctx),
        }
        self.state.active_seq = prev_active_seq;
        self.state.drain_ctx = prev_drain;
    }

    #[cfg(feature = "gui")]
    pub(crate) fn gui_for_each_cell(
        &self,
        bounds: Vec2<u16>,
        mut f: impl FnMut(u16, u16, &str, &GridCellStyle, bool),
    ) {
        let row_w = self.state.cells_stride as usize;
        let cells = &self.state.cells.0;
        let graphemes = &self.state.graphemes.0;
        for y in 0..bounds.y {
            let mut x = 0u16;
            while x < bounds.x {
                let i = y as usize * row_w + x as usize;
                if i >= cells.len() {
                    break;
                }
                let cell = &cells[i];
                let glyph = cell.glyph(graphemes);
                let wide = cell.flags.get(GridCellFlags::WIDE);
                f(x, y, glyph, &cell.style, wide);
                x += if wide { 2 } else { 1 };
            }
        }
    }

    /// Returns the most recently flushed frame as a [`StyledString`].
    pub fn get_snapshot(&self) -> StyledString {
        let cells = &self.state.cells.1;
        let graphemes = &self.state.graphemes.1;
        let size = self.state.size;
        let stride = self.state.cells_stride as usize;
        let mut out = StyledString::new();
        for y in 0..size.y {
            if y > 0 {
                out.push_str("\n");
            }
            let mut x = 0u16;
            while x < size.x {
                let i = (y as usize) * stride + x as usize;
                if i >= cells.len() {
                    break;
                }
                let cell = &cells[i];
                let glyph = cell.glyph(graphemes);
                let style = grid_cell_style_to_style(&cell.style);
                out.push_span(StyledStr { text: glyph, style });
                let width = if cell.flags.get(GridCellFlags::WIDE) {
                    2u16
                } else {
                    1u16
                };
                x += width;
            }
        }
        out
    }
}

pub(crate) fn grid_cell_style_to_style(s: &GridCellStyle) -> Style {
    let mut out = Style::new();
    if s.fg != Color::Foreground {
        out = out.fg(s.fg);
    }
    if s.bg != Color::Background {
        out = out.bg(s.bg);
    }
    if s.underline_color != Color::Foreground {
        out = out.underline_color(s.underline_color);
    }
    if s.underline != UnderlineType::None {
        out = out.underline(s.underline);
    }
    if s.attrs & StyleAttribute::Bold as u8 != 0 {
        out = out.bold();
    }
    if s.attrs & StyleAttribute::Italic as u8 != 0 {
        out = out.italic();
    }
    if s.attrs & StyleAttribute::Strikethrough as u8 != 0 {
        out = out.strikethrough();
    }
    if s.attrs & StyleAttribute::Reverse as u8 != 0 {
        out = out.reverse();
    }
    if s.attrs & StyleAttribute::Dim as u8 != 0 {
        out = out.dim();
    }
    out
}

/// Per-widget rendering context.
pub struct RenderContext<'a> {
    state: &'a mut GridRendererState,
    raw_writer: &'a mut dyn Write,
    /// The screen-space origin of this region in the global cell grid.
    pub anchor: Vec2<i32>,
    /// The cursor offset within this region in local coordinates.
    pub cursor: Vec2<i32>,
    /// The screen-space top-left of the visible clip rect.
    pub position: Vec2<u16>,
    /// The visible clip size in cells.
    pub physical_size: Vec2<u16>,
    /// The virtual region size in cells.
    pub size: Vec2<u16>,
    /// The screen-space top-left of the enclosing viewport.
    pub viewport_pos: Vec2<u16>,
    /// The size of the enclosing viewport in cells.
    pub viewport_size: Vec2<u16>,
    base: Style,
    style: Style,
}

impl<'a> RenderContext<'a> {
    #[cfg(feature = "images")]
    pub(crate) fn queue_raw(&mut self, bytes: &[u8]) {
        let _ = self.raw_writer.write_all(bytes);
    }

    /// Applies `child_style` onto the inherited style and rebases `base`.
    fn apply_child_style(&mut self, child_style: Style) {
        self.set_style(child_style);
        if !self.resolve_inherited_reverse(&child_style) {
            let blend = self.style.get_blend().unwrap_or(100);
            if child_style.get_overlay_color().is_none() && blend < 100 {
                self.style.set_overlay_color(None);
            }
        }
        self.base = self.style;
    }

    /// Resolves an inherited reverse attribute into concrete `fg`/`bg` colors, returning whether it did so.
    fn resolve_inherited_reverse(&mut self, child_style: &Style) -> bool {
        let child_owns_reverse =
            child_style.get_attrs_mask() & StyleAttribute::Reverse as u8 != 0;
        if self.style.has_reverse() && !child_owns_reverse {
            let visible_bg = self.style.get_fg().unwrap_or(Color::Foreground);
            let visible_fg = self.style.get_bg().unwrap_or(Color::Background);
            self.style.set_reverse(false);
            self.style.set_fg(Some(visible_fg));
            self.style.set_bg(Some(visible_bg));
            return true;
        }
        false
    }

    fn queue_overlay(&mut self, child: &dyn Widget, pos: Vec2<i32>, kind: Kind) {
        self.move_to(pos);
        let render_size = child.get_rect_size();
        let child_style = child.get_style();
        let parent_pos = self.position;
        let parent_phys = self.physical_size;
        let mut region = self.region(render_size);
        region.apply_child_style(child_style);
        let (clip_pos, clip_size) = match kind {
            Kind::Layer => (parent_pos, parent_phys),
            _ => (Vec2::of(0u16), region.state.size),
        };
        let snap_pos =
            Axis2D::map(|a| region.anchor[a].max(clip_pos[a] as i32).max(0) as u16);
        let scratch_size = Vec2::new(region.state.cells_stride, region.state.cells_height);
        let snap_size = Axis2D::map(|a| {
            let natural_end = region.anchor[a] + region.size[a] as i32;
            let limit = clip_pos[a] as i32 + clip_size[a] as i32;
            #[cfg(feature = "gui")]
            let limit = if natural_end >= limit {
                limit + SCRATCH_SLACK as i32
            } else {
                limit
            };
            let limit = limit.min(scratch_size[a] as i32);
            let end = natural_end.min(limit);
            (end - snap_pos[a] as i32).max(0) as u16
        });
        let snapshot = CtxSnapshot {
            anchor: region.anchor,
            position: snap_pos,
            physical_size: snap_size,
            size: region.size,
            viewport_pos: region.viewport_pos,
            viewport_size: region.viewport_size,
            base: region.base,
            style: region.style,
        };
        let parent_grid_origin_cells = region.state.drain_ctx.grid_origin_cells;
        let parent_screen_pos_px = region.state.drain_ctx.entry_screen_pos_px;
        let seq = region.state.next_seq();
        let inherited_clip = region.state.drain_ctx.parent_clip_screen_px;
        let parent_clip_screen_px = match kind {
            Kind::Layer => crate::runtime::get_terminal_info()
                .and_then(|i| i.cell_px)
                .map(|cp| {
                    let off = Vec2::new(parent_pos.x as i32, parent_pos.y as i32)
                        - parent_grid_origin_cells;
                    let o = parent_screen_pos_px
                        + Vec2::new(off.x * cp.x as i32, off.y * cp.y as i32);
                    let s = Vec2::new(
                        parent_phys.x as u32 * cp.x as u32,
                        parent_phys.y as u32 * cp.y as u32,
                    );
                    inherited_clip.map_or((o, s), |p| intersect_clip((o, s), p))
                })
                .or(inherited_clip),
            _ => inherited_clip,
        };
        region.state.defer_queue.push(QueuedEntry {
            widget: child as *const dyn Widget,
            snapshot,
            z: child.get_layer(),
            seq,
            parent_screen_pos_px,
            #[cfg(feature = "gui")]
            cell_pos_in_parent: Vec2::new(snap_pos.x as i32, snap_pos.y as i32)
                - parent_grid_origin_cells,
            parent_grid_origin_cells,
            parent_clip_screen_px,
            kind,
        });
    }
}

impl<'a> RenderContext<'a> {
    /// Sets the cursor to `pos` in local coordinates.
    pub fn move_to(&mut self, pos: Vec2<i32>) {
        self.cursor = pos;
    }

    /// Draws `source` into this widget's cell rect using the best available terminal graphics protocol.
    #[cfg(feature = "images")]
    pub fn draw_image(&mut self, source: &crate::render::image::ImageSource, fill: bool) {
        crate::render::image::dispatch(self, source, fill);
    }

    /// Marks cells in the given screen-space rect as invalid.
    pub fn invalidate(&mut self, pos: Vec2<i32>, size: Vec2<u16>) {
        let screen = self.anchor + pos;
        let go = self.state.drain_ctx.grid_origin_cells;
        let local = screen - go;
        let w = self.state.cells_stride as usize;
        let x_start = local.x.max(0) as usize;
        let y_start = local.y.max(0) as usize;
        let x_end = (local.x + size.x as i32).min(self.state.cells_stride as i32).max(0) as usize;
        let y_end = (local.y + size.y as i32).min(self.state.cells_height as i32).max(0) as usize;
        for y in y_start..y_end {
            for x in x_start..x_end {
                self.state.cells.0[y * w + x].flags.set(GridCellFlags::INVALID, true);
            }
        }
    }

    /// Sets the active style by applying `style` over the current base.
    pub fn set_style(&mut self, style: Style) {
        self.style = self.base.apply(style);
        if let (Some(base), Some(over)) = (self.base.get_blend(), style.get_blend()) {
            let compound = (base as u16 * over as u16 / 100) as u8;
            self.style = self.style.blend(compound);
        }
    }

    /// Fills this region with the active background color.
    pub fn clear(&mut self) {
        let overlay = self.style.get_overlay_color();
        if overlay.is_none() {
            return;
        }
        let blend = self.style.get_blend().unwrap_or(100);
        if blend == 0 {
            return;
        }
        let position = self.position.map(|n| n as i32);
        let y_start = std::cmp::max(position.y, 0) as usize;
        let y_end = std::cmp::min(
            position.y + self.physical_size.y as i32,
            self.state.cells_height as i32,
        ) as usize;
        if blend >= 100 {
            let saved_cursor = self.cursor;
            let local_x = self.position.x as i32 - self.anchor.x;
            let style = self.style;
            for y_screen in y_start..y_end {
                let local_y = y_screen as i32 - self.anchor.y;
                self.move_to((local_x, local_y).into());
                self.row_writer().fill(" ", &style);
            }
            self.cursor = saved_cursor;
        } else {
            let x_start = std::cmp::max(position.x, 0) as usize;
            let x_end = std::cmp::min(
                position.x + self.physical_size.x as i32,
                self.state.cells_stride as i32,
            ) as usize;
            let w = self.state.cells_stride as usize;
            let go = self.state.drain_ctx.grid_origin_cells;
            let overlay_bg = overlay.unwrap();
            let resolve_default = |c: Color, default: Color| if c.is_default() { default } else { c };
            for y in y_start..y_end {
                let local_y = (y as i32 - go.y).max(0) as usize;
                for x in x_start..x_end {
                    let local_x = (x as i32 - go.x).max(0) as usize;
                    let cell = &mut self.state.cells.0[local_y * w + local_x];
                    let was_invalid = cell.flags.get(GridCellFlags::INVALID);
                    resolve_dim(&mut cell.style);
                    cell.style.bg = lerp_color(resolve_default(cell.style.bg, Color::Background), overlay_bg, blend);
                    cell.style.fg = lerp_color(resolve_default(cell.style.fg, Color::Foreground), overlay_bg, blend);
                    cell.flags.set(GridCellFlags::INVALID, false);
                    if was_invalid {
                        cell.flags.set(GridCellFlags::DIRTY, true);
                    }
                }
            }
            if self.style.has_reverse() {
                self.style.set_fg(None);
                self.base.set_fg(None);
            } else {
                self.style.set_bg(None);
                self.base.set_bg(None);
            }
        }
    }

    /// Fills every cell in this region with `glyph` using the active style.
    pub fn fill(&mut self, glyph: &str) {
        let position = self.position.map(|n| n as i32);
        let y_start = std::cmp::max(position.y, 0);
        let y_end = std::cmp::min(
            position.y + self.physical_size.y as i32,
            self.state.cells_height as i32,
        );
        let saved_cursor = self.cursor;
        let local_x = self.position.x as i32 - self.anchor.x;
        let style = self.style;
        for y_screen in y_start..y_end {
            let local_y = y_screen - self.anchor.y;
            self.move_to((local_x, local_y).into());
            self.row_writer().fill(glyph, &style);
        }
        self.cursor = saved_cursor;
    }

    /// Writes `text` at the cursor, advancing per grapheme width.
    pub fn write(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let blend = self.style.get_blend().unwrap_or(100);
        if blend == 0 {
            return;
        }
        let position = self.anchor + self.cursor;
        if position.x + self.size.x as i32 <= self.position.x as i32 {
            return;
        }

        let style = self.style;
        let overlay_bg = style.get_overlay_color();

        let mut row = self.row_writer();
        let range = row.get_range();
        if range.is_empty() {
            return;
        }

        let mut col: usize = 0;
        let mut advance: i32 = 0;
        for grapheme in text.graphemes(true) {
            if col >= range.end {
                break;
            }
            let c = grapheme.chars().next().unwrap();
            if c == '\n' {
                break;
            }
            let w = terminal_grapheme_width(grapheme);
            advance += w as i32;

            let (g, w) = if col + w > range.end {
                (" ", 1usize)
            } else {
                (grapheme, w)
            };

            if col >= range.start {
                if g == " " && blend < 100 {
                    row.cell(col).blend_visible_bg(overlay_bg, blend);
                } else {
                    row.cell(col).grapheme(g).style(&style);
                }
            }
            col += w;
        }
        drop(row);
        self.cursor.x += advance;
    }

    /// Returns a [`RowWriter`] for the visible cells in the current row.
    pub fn row_writer(&mut self) -> RowWriter<'_> {
        let position = self.anchor + self.cursor;
        let (clip_pos, clip_size) = (self.position, self.physical_size);
        let clip_x_start = clip_pos.x as i32;
        let clip_x_end = (clip_pos.x + clip_size.x) as i32;
        if position.y < clip_pos.y as i32
            || position.y >= (clip_pos.y + clip_size.y) as i32
            || position.x >= clip_x_end
        {
            return RowWriter {
                cells: &mut [],
                graphemes: &mut self.state.graphemes.0,
                skip: 0,
                sink: [GridCell::DEFAULT; 2],
            };
        }
        let actual_start = position.x.max(clip_x_start);
        let skip = (actual_start - position.x) as usize;
        let row_w = self.state.cells_stride as usize;
        let go = self.state.drain_ctx.grid_origin_cells;
        let local_y = position.y - go.y;
        let local_start = actual_start - go.x;
        let count = (clip_x_end - actual_start) as usize;
        let dst_start = local_y as usize * row_w + local_start as usize;
        if local_start > 0 {
            let prev = &mut self.state.cells.0[dst_start - 1];
            if prev.flags.0 & GridCellFlags::WIDE != 0 {
                prev.flags.0 &= !GridCellFlags::WIDE;
                prev.set_glyph_char(' ');
            }
        }
        if count > 0 {
            let last = &mut self.state.cells.0[dst_start + count - 1];
            if last.flags.0 & GridCellFlags::WIDE != 0 {
                last.flags.0 &= !GridCellFlags::WIDE;
                last.set_glyph_char(' ');
            }
        }
        RowWriter {
            cells: &mut self.state.cells.0[dst_start..dst_start + count],
            graphemes: &mut self.state.graphemes.0,
            skip,
            sink: [GridCell::DEFAULT; 2],
        }
    }

    /// Returns a sub-context anchored at the cursor with the given virtual `size`.
    pub fn region(&'_ mut self, mut size: Vec2<u16>) -> RenderContext<'_> {
        let virtual_size = size;

        let unclamped = self.anchor + self.cursor;

        let position = Axis2D::map(|a| {
            unclamped[a].clamp(
                self.position[a] as i32,
                (self.position[a] + self.physical_size[a]) as i32,
            ) as u16
        });

        size = Axis2D::map(|a| {
            let lost = (position[a] as i32 - unclamped[a]).max(0) as u16;
            size[a]
                .saturating_sub(lost)
                .min(self.physical_size[a] - (position[a] - self.position[a]))
        });

        let physical_size = Axis2D::map(|a| {
            let child_end = position[a].saturating_add(size[a]);
            let parent_logical_end = self.position[a].saturating_add(self.size[a]);
            let parent_phys_end = self.position[a].saturating_add(self.physical_size[a]);
            if child_end >= parent_logical_end {
                parent_phys_end.saturating_sub(position[a])
            } else {
                size[a]
            }
        });

        RenderContext {
            state: self.state,
            raw_writer: &mut *self.raw_writer,
            cursor: Vec2::of(0i32),
            style: self.style,
            base: self.style,
            anchor: self.anchor + self.cursor,
            position,
            physical_size,
            size: virtual_size,
            viewport_pos: self.viewport_pos,
            viewport_size: self.viewport_size,
        }
    }

    /// Returns a sub-context like [`region`](Self::region) with the viewport reset to this rect.
    pub fn viewport(&'_ mut self, size: Vec2<u16>) -> RenderContext<'_> {
        let mut ctx = self.region(size);
        ctx.viewport_pos = ctx.position;
        ctx.viewport_size = ctx.physical_size;
        ctx
    }

    /// Draws a border of `size` around the region using `border`'s glyph set.
    pub fn border(&mut self, size: Vec2<u16>, border: &Border) {
        self.move_to((0i32, 0i32).into());
        write!(self, "{}", border.get_corner(Vec2::of(false)));
        for _ in 0..self.size.x.saturating_sub(2) {
            write!(self, "{}", border.get_edge(Axis2D::Y));
        }
        if self.size.x < size.x {
            write!(self, "{}", border.get_edge(Axis2D::Y));
        } else {
            write!(self, "{}", border.get_corner(Vec2::new(true, false)));
        }
        for y in 1..self.size.y {
            self.move_to(Vec2::new(0i32, y as i32));
            write!(self, "{}", border.get_edge(Axis2D::X));
            self.move_to(Vec2::new(size.x.saturating_sub(1) as i32, y as i32));
            write!(self, "{}", border.get_edge(Axis2D::X));
        }
        if self.size.y > 1 {
            self.move_to(Vec2::new(0i32, size.y.saturating_sub(1) as i32));
            write!(self, "{}", border.get_corner(Vec2::new(false, true)));
            for _ in 1..self.size.x.saturating_sub(1) {
                write!(self, "{}", border.get_edge(Axis2D::Y));
            }
            if self.size.x < size.x {
                write!(self, "{}", border.get_edge(Axis2D::Y));
            } else {
                write!(self, "{}", border.get_corner(Vec2::of(true)));
            }
        }
    }

    /// Returns the terminal grid size in cells.
    pub fn get_screen_size(&self) -> Vec2<u16> {
        self.state.size
    }

    /// Routes formatted output from [`write!`] through [`write`](Self::write).
    pub fn write_fmt(&mut self, args: std::fmt::Arguments<'_>) {
        if let Some(s) = args.as_str() {
            self.write(s);
        } else {
            use std::fmt::Write;
            let mut buf = std::mem::take(&mut self.state.fmt_buf);
            buf.clear();
            let _ = buf.write_fmt(args);
            self.write(&buf);
            self.state.fmt_buf = buf;
        }
    }

    /// Renders `child` at `pos`.
    pub fn render_child(
        &mut self,
        child: &dyn Widget,
        pos: Vec2<i32>,
    ) {
        self.move_to(pos);
        let render_size = child.get_rect_size();
        let child_style = child.get_style();
        let mut region = self.region(render_size);
        region.apply_child_style(child_style);

        let z = child.get_layer();
        let blended = child.get_style().get_blend().is_some_and(|b| b < 100);

        if z == Layer::Bottom && !blended {
            child.render(region);
            return;
        }

        let (clip_pos, clip_size) = if z == Layer::Top {
            (Vec2::of(0u16), region.state.size)
        } else {
            (region.viewport_pos, region.viewport_size)
        };
        let snap_pos = Axis2D::map(|a| region.anchor[a].max(clip_pos[a] as i32).max(0) as u16);
        let snap_size = Axis2D::map(|a| {
            let end = (region.anchor[a] + region.size[a] as i32)
                .min(clip_pos[a] as i32 + clip_size[a] as i32);
            (end - snap_pos[a] as i32).max(0) as u16
        });
        let snapshot = CtxSnapshot {
            anchor: region.anchor,
            position: snap_pos,
            physical_size: snap_size,
            size: region.size,
            viewport_pos: region.viewport_pos,
            viewport_size: region.viewport_size,
            base: region.base,
            style: region.style,
        };
        let parent_grid_origin_cells = region.state.drain_ctx.grid_origin_cells;
        let parent_screen_pos_px = region.state.drain_ctx.entry_screen_pos_px;
        let seq = region.state.next_seq();
        let queued = QueuedEntry {
            widget: child as *const dyn Widget,
            snapshot,
            z,
            seq,
            parent_screen_pos_px,
            #[cfg(feature = "gui")]
            cell_pos_in_parent: Vec2::new(snap_pos.x as i32, snap_pos.y as i32)
                - parent_grid_origin_cells,
            parent_grid_origin_cells,
            parent_clip_screen_px: if z == Layer::Top {
                region.state.root_clip_screen_px
            } else {
                region.state.drain_ctx.parent_clip_screen_px
            },
            kind: Kind::Z,
        };
        region.state.defer_queue.push(queued);
    }

    /// Queues `f` to render a sub-cell-shifted offset region at the cursor.
    #[cfg(feature = "gui")]
    pub fn queue_offset_region<W: Widget + 'static>(
        &mut self,
        widget: &W,
        viewport_size: Vec2<u16>,
        content_size: Vec2<u16>,
        content_offset_cells: Vec2<i32>,
        subcell_offset_px: Vec2<i32>,
        f: impl FnOnce(&W, RenderContext) + 'static,
    ) {
        let unclamped = self.anchor + self.cursor;
        let viewport_pos = Axis2D::map(|a| {
            unclamped[a].clamp(
                self.position[a] as i32,
                (self.position[a] + self.physical_size[a]) as i32,
            ) as u16
        });
        let overflow = Axis2D::map(|a| {
            if content_size[a] <= viewport_size[a] {
                return 0u16;
            }
            let viewport_end = unclamped[a].saturating_add(viewport_size[a] as i32);
            let parent_logical_end = (self.position[a] as i32)
                .saturating_add(self.size[a] as i32);
            let parent_phys_end = (self.position[a] as i32)
                .saturating_add(self.physical_size[a] as i32);
            if viewport_end >= parent_logical_end && parent_phys_end > parent_logical_end {
                1u16
            } else {
                0u16
            }
        });
        let viewport_phys = Axis2D::map(|a| {
            let lost = (viewport_pos[a] as i32 - unclamped[a]).max(0) as u16;
            let cap = self.physical_size[a]
                .saturating_add(overflow[a])
                .saturating_sub(viewport_pos[a] - self.position[a]);
            viewport_size[a]
                .saturating_add(overflow[a])
                .saturating_sub(lost)
                .min(cap)
        });
        let clip_pos = self.viewport_pos;
        let clip_size = self.viewport_size;
        let viewport_snap_pos = Axis2D::map(|a| viewport_pos[a].max(clip_pos[a]));
        let viewport_snap_end = Axis2D::map(|a| {
            (viewport_pos[a] + viewport_phys[a])
                .min(clip_pos[a] + clip_size[a] + overflow[a])
        });
        let viewport_snap_size =
            Axis2D::map(|a| viewport_snap_end[a].saturating_sub(viewport_snap_pos[a]));

        let content_offset_clamped = Axis2D::map(|a| {
            content_offset_cells[a].max(-(viewport_snap_pos[a] as i32))
        });
        let content_pos = Axis2D::map(|a| {
            (viewport_snap_pos[a] as i32 + content_offset_clamped[a]).max(0) as u16
        });
        let scratch_size = Vec2::new(self.state.cells_stride, self.state.cells_height);
        let content_size_clamped = Axis2D::map(|a| {
            content_size[a].min(scratch_size[a].saturating_sub(content_pos[a]))
        });

        let snapshot = CtxSnapshot {
            anchor: self.anchor + self.cursor + content_offset_clamped,
            position: content_pos,
            physical_size: content_size_clamped,
            size: content_size_clamped,
            viewport_pos: content_pos,
            viewport_size: content_size_clamped,
            base: self.base,
            style: self.style,
        };

        let parent_screen_pos_px = self.state.drain_ctx.entry_screen_pos_px;
        let viewport_snap_pos_i =
            Vec2::new(viewport_snap_pos.x as i32, viewport_snap_pos.y as i32);
        let cell_pos_in_parent = viewport_snap_pos_i - self.state.drain_ctx.grid_origin_cells;

        let callback: OffsetCallback = Box::new(move |w, ctx| {
            if let Some(typed) = w.downcast_ref::<W>() {
                f(typed, ctx);
            }
        });

        let seq = self.state.next_seq();
        self.state.defer_queue.push(QueuedEntry {
            widget: widget as &dyn Widget as *const dyn Widget,
            snapshot,
            z: Layer::Bottom,
            seq,
            parent_screen_pos_px,
            cell_pos_in_parent,
            parent_grid_origin_cells: Vec2::of(0i32),
            parent_clip_screen_px: self.state.drain_ctx.parent_clip_screen_px,
            kind: Kind::Offset {
                viewport_size_cells: viewport_snap_size,
                content_offset_cells: content_offset_clamped,
                subcell_offset_px,
                callback,
            },
        });
    }

    /// Queues `child` as a layer overlay at `pos`.
    pub fn queue_layer(&mut self, child: &dyn Widget, pos: Vec2<i32>) {
        self.queue_overlay(child, pos, Kind::Layer);
    }

    /// Queues `child` as a popup at `pos`, painted above all other layers.
    pub fn queue_popup(&mut self, child: &dyn Widget, pos: Vec2<i32>) {
        self.queue_overlay(child, pos, Kind::Popup);
    }
}

