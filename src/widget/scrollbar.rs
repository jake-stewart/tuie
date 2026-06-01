//! Scrollbar state, configuration, and rendering shared by scrollable containers.

use crate::prelude::*;
use chord_macro::chord;

const VERTICAL_PARTIALS: &[char] = &['█', '▇', '▆', '▅', '▄', '▃', '▂', '▁'];

/// Glyph style used to draw the scrollbar thumb.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ScrollbarThumb {
    /// Solid block thumb that supports per-cell partial coverage.
    Block,
    /// Thumb drawn from a [`Border`] glyph set.
    Border(&'static Border),
}

impl ScrollbarThumb {
    /// Solid block thumb that supports per-cell partial coverage.
    pub const BLOCK: ScrollbarThumb = ScrollbarThumb::Block;
    /// Thumb drawn from the single-line [`Border`] glyph set.
    pub const SINGLE: ScrollbarThumb = ScrollbarThumb::Border(Border::SINGLE);
    /// Thumb drawn from the double-line [`Border`] glyph set.
    pub const DOUBLE: ScrollbarThumb = ScrollbarThumb::Border(Border::DOUBLE);
    /// Thumb drawn from the heavy-line [`Border`] glyph set.
    pub const THICK: ScrollbarThumb = ScrollbarThumb::Border(Border::THICK);
    /// Thumb drawn from the dashed-line [`Border`] glyph set.
    pub const DASHED: ScrollbarThumb = ScrollbarThumb::Border(Border::DASHED);
    /// Thumb drawn from the heavy dashed-line [`Border`] glyph set.
    pub const THICK_DASHED: ScrollbarThumb = ScrollbarThumb::Border(Border::THICK_DASHED);
    /// Thumb drawn from the ASCII [`Border`] glyph set.
    pub const ASCII: ScrollbarThumb = ScrollbarThumb::Border(Border::ASCII);

    /// Returns the subpixels per cell along the scrollbar axis.
    pub fn get_subpixels(&self, axis: Axis2D) -> i32 {
        match self {
            Self::Block if axis == Axis2D::Y => 8,
            Self::Block => 1,
            Self::Border(b) if b.has_stubs(axis) => 2,
            Self::Border(_) => 1,
        }
    }

    /// Returns true when this thumb can render a half-cell along `axis`.
    pub fn has_half_cell(&self, axis: Axis2D) -> bool {
        self.get_subpixels(axis) > 1
    }

    /// Returns per-axis corner-extension flags and a TTY corner-sharing flag for two-axis scrollbar layout.
    pub fn corner_extension(&self, both_visible: bool) -> (Vec2<bool>, bool) {
        let is_gui = crate::runtime::is_gui();
        let share_corner = both_visible && self.has_half_cell(Axis2D::Y) && !is_gui;
        let extend_into_corner_gui = both_visible && is_gui;
        let extend = Axis2D::map(|axis| {
            share_corner
                || (extend_into_corner_gui
                    && (matches!(self, ScrollbarThumb::Border(_)) || self.has_half_cell(axis)))
        });
        (extend, share_corner)
    }

    fn glyph(&self, axis: Axis2D, covered: i32, n_levels: i32, leading: bool) -> (char, bool) {
        let full = covered >= n_levels;
        let full_row = n_levels >= self.get_subpixels(axis);
        match self {
            Self::Block if axis == Axis2D::Y => {
                if full {
                    if full_row {
                        (' ', true)
                    } else {
                        ('▄', true)
                    }
                } else if leading {
                    (VERTICAL_PARTIALS[(n_levels - covered) as usize], false)
                } else {
                    (VERTICAL_PARTIALS[covered as usize], true)
                }
            }
            Self::Block => ('▄', false),
            Self::Border(b) if full => {
                if full_row {
                    (b.get_edge(axis.flip()), false)
                } else {
                    let ch = match axis {
                        Axis2D::Y => b.get_arms(false, false, true, false),
                        Axis2D::X => b.get_arms(true, false, false, false),
                    };
                    (ch, false)
                }
            }
            Self::Border(b) => {
                let ch = match (axis, leading) {
                    (Axis2D::Y, true) => b.get_arms(false, false, false, true),
                    (Axis2D::Y, false) => b.get_arms(false, false, true, false),
                    (Axis2D::X, true) => b.get_arms(false, true, false, false),
                    (Axis2D::X, false) => b.get_arms(true, false, false, false),
                };
                (ch, false)
            }
        }
    }
}

/// Per-axis visibility mode applied to a scroll container's scrollbar.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scrollbar {
    /// Scrolling is enabled, but the bar is never drawn.
    Hidden,
    /// Bar is drawn only when the content overflows the viewport.
    AutoHide,
    /// Bar is always drawn whenever scrolling is enabled.
    Visible,
}

impl std::fmt::Display for Scrollbar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hidden => write!(f, "Hidden"),
            Self::AutoHide => write!(f, "AutoHide"),
            Self::Visible => write!(f, "Visible"),
        }
    }
}

/// Global default appearance for scrollbars.
#[derive(Clone, Copy)]
pub struct ScrollbarConfig {
    /// Glyph style used to draw the thumb.
    pub thumb: ScrollbarThumb,
    /// Style applied to thumb glyphs.
    pub thumb_style: Style,
    /// Glyph used to draw the track.
    pub track: char,
    /// Style applied to track glyphs.
    pub track_style: Style,
}

crate::config_module!(ScrollbarConfig {
    thumb: ScrollbarThumb::THICK,
    thumb_style: Style::new(),
    track: ' ',
    track_style: Style::new(),
});

/// Mutable per-scrollbar state covering thumb size, position, drag, and visibility.
pub struct ScrollbarState {
    ratio: f32,
    progress: f32,
    dragging: bool,
    drag_offset: f32,
    visible: bool,
    remap: Option<(f32, f32)>,
}

impl ScrollbarState {
    fn display_progress(&self) -> f32 {
        match self.remap {
            Some((_, display)) => display,
            None => self.progress,
        }
    }

    fn thumb_height(&self, view: f32) -> f32 {
        (self.ratio * view).max(1.0)
    }

    fn thumb_height_snapped(&self, view: f32, sub_px: f32) -> f32 {
        let raw = self.thumb_height(view);
        let snapped = (raw * sub_px).round().max(sub_px) as u32;
        snapped as f32 / sub_px
    }

    fn progress_from_pos(&self, pos: f32, view: f32, sub_px: f32) -> f32 {
        let thumb_height = self.thumb_height_snapped(view, sub_px);
        let track = view - thumb_height;
        if track <= 0.0 {
            return 0.0;
        }
        (pos / track).clamp(0.0, 1.0)
    }

    fn can_scroll(&self) -> bool {
        self.ratio < 1.0
    }

    pub(crate) fn thumb_top(&self, view: f32, sub_px: f32) -> f32 {
        let thumb_height = self.thumb_height_snapped(view, sub_px);
        let track = view - thumb_height;
        track * self.display_progress()
    }
}

impl ScrollbarState {
    /// Creates a hidden, unscrolled state with no active drag.
    pub const fn new() -> Self {
        Self {
            ratio: 0.0,
            progress: 0.0,
            dragging: false,
            drag_offset: 0.0,
            visible: false,
            remap: None,
        }
    }

    /// Returns whether the scrollbar is currently shown.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Sets whether the scrollbar is currently shown.
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    /// Returns true when the user is dragging the thumb.
    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    /// Sets the scroll progress in `0.0..=1.0`.
    pub fn set_progress(&mut self, new_progress: f32) {
        if new_progress == self.progress && self.remap.is_none() {
            return;
        }
        let old_display = self.display_progress();
        self.progress = new_progress;
        if let Some((anchor_in, anchor_out)) = self.remap {
            let new_display = piecewise_remap(new_progress, anchor_in, anchor_out);
            self.remap = Some((new_progress, new_display));
        } else if new_progress != old_display {
            self.remap = Some((new_progress, old_display));
        }
        tuie::dirty_paint();
    }

    /// Sets the ratio of viewport to total content, where `1.0` means fully visible.
    pub fn set_ratio(&mut self, ratio: f32) {
        if ratio != self.ratio {
            self.ratio = ratio;
            tuie::dirty_paint();
        }
    }

    /// Returns true when the thumb extends into the trailing half-cell of a `share_corner`-style view.
    pub fn thumb_reaches_corner_half(&self, view: f32, sub_px: f32) -> bool {
        if self.ratio >= 1.0 {
            return false;
        }
        let full_rows = view as i32;
        let thumb_top = self.thumb_top(view, sub_px);
        let thumb_height = self.thumb_height_snapped(view, sub_px);
        let thumb_bot_sp = ((thumb_top + thumb_height) * sub_px).round() as i32;
        thumb_bot_sp > full_rows * sub_px as i32
    }

    /// Converts whole-cell `scroll` and sub-cell fraction `sub` into a normalized `[0.0, 1.0]` progress.
    pub fn progress_from_subcell(scroll: u32, sub: f32, max_scroll: u32) -> f32 {
        if max_scroll == 0 {
            return 0.0;
        }
        ((scroll as f64 + sub as f64) / max_scroll as f64).clamp(0.0, 1.0) as f32
    }

    /// Splits `progress` back into `(whole cells, sub-cell fraction)` for the given scroll range.
    pub fn subcell_from_progress(progress: f32, max_scroll: u32) -> (u32, f32) {
        let p = (progress as f64).clamp(0.0, 1.0);
        let total = p * max_scroll as f64;
        let cells = (total as u64).min(max_scroll as u64) as u32;
        (cells, (total - cells as f64) as f32)
    }

    /// Draws the scrollbar track and thumb along `axis` over a track length of `view` cells.
    pub fn render(
        &self,
        ctx: &mut RenderContext,
        axis: Axis2D,
        view: f32,
        style: &ScrollbarStyle,
        leading_anchor: Option<(u32, i32)>,
    ) {
        if self.ratio >= 1.0 {
            return;
        }

        let resolved = style.get_resolved();
        let thumb = resolved.thumb;
        let thumb_style = resolved.thumb_style;
        let track_style = resolved.track_style;
        let track_char = resolved.track;

        let n = thumb.get_subpixels(axis);
        let sub_px = n as f32;
        let has_half_cell = view.fract() > 0.0;
        let full_rows = view as u16;
        let half_n = n / 2;
        let total_rows = if has_half_cell && half_n > 0 {
            full_rows + 1
        } else {
            full_rows
        };

        let thumb_top = self.thumb_top(view, sub_px);
        let thumb_height = self.thumb_height_snapped(view, sub_px);
        let leading: i32 = leading_anchor.map_or(0, |(_, l)| l);
        let trailing: i32 = if leading_anchor.is_some() { 1 } else { 0 };
        let thumb_top_sp = match leading_anchor {
            Some((whole, lead)) => (whole as i32 + lead) * n,
            None => (thumb_top * sub_px).round() as i32,
        };
        let thumb_bot_sp = thumb_top_sp + (thumb_height * sub_px).round() as i32;

        let move_to = |ctx: &mut RenderContext, i: i32| {
            if axis == Axis2D::Y {
                ctx.move_to((0, i).into());
            } else {
                ctx.move_to((i, 0).into());
            }
        };

        let r_start: i32 = -leading;
        let r_end: i32 = total_rows as i32 + trailing;
        for r in r_start..r_end {
            move_to(ctx, r + leading);
            let in_pad = r < 0 || r >= total_rows as i32;
            let n_levels = if !in_pad && r == full_rows as i32 {
                half_n
            } else {
                n
            };
            let row_top_sp = (r + leading) * n;
            let row_bot_sp = row_top_sp + n_levels;

            let cover_top = thumb_top_sp.max(row_top_sp);
            let cover_bot = thumb_bot_sp.min(row_bot_sp);
            let covered = (cover_bot - cover_top).max(0);

            if n_levels <= 0 || covered <= 0 {
                ctx.set_style(track_style);
                write!(ctx, "{}", track_char);
            } else {
                let leading_edge = thumb_top_sp > row_top_sp;
                let (ch, reversed) = thumb.glyph(axis, covered, n_levels, leading_edge);
                ctx.set_style(thumb_style.reverse_if(reversed));
                write!(ctx, "{}", ch);
            }
        }
    }

    /// Renders the scrollbar with sub-cell smooth offset in GUI mode, falling back to whole-cell rendering otherwise.
    pub fn render_smooth<W, F>(
        &self,
        ctx: &mut RenderContext,
        widget: &W,
        axis: Axis2D,
        bar_size: Vec2<u16>,
        view: f32,
        style: &ScrollbarStyle,
        accessor: F,
    ) where
        W: Widget + 'static,
        F: Fn(&W) -> Option<(&ScrollbarStyle, &ScrollbarState)> + 'static,
    {
        #[cfg(feature = "gui")]
        if crate::runtime::is_gui() {
            if let Some(cell_px) = crate::runtime::get_terminal_info()
                .and_then(|i| i.cell_px)
                .map(|c| c[axis])
                .filter(|&v| v > 1)
            {
                let thumb = style.get_resolved_thumb();
                let thumb_top = self.thumb_top(view, thumb.get_subpixels(axis) as f32);
                let thumb_top_px = (thumb_top * cell_px as f32).round() as i32;
                let whole = thumb_top_px.div_euclid(cell_px as i32) as u32;
                let sub = thumb_top_px.rem_euclid(cell_px as i32);
                let mut subcell_off = Vec2::of(0i32);
                subcell_off[axis] = sub;
                let leading: i32 = if ctx.anchor[axis] + ctx.cursor[axis] >= 1 {
                    1
                } else {
                    0
                };
                let mut content_size = bar_size;
                content_size[axis] = content_size[axis].saturating_add(leading as u16 + 1);
                let mut content_offset = Vec2::of(0i32);
                content_offset[axis] = -leading;
                ctx.queue_offset_region(
                    widget,
                    bar_size,
                    content_size,
                    content_offset,
                    subcell_off,
                    move |this: &W, mut sb_ctx| {
                        if let Some((style, state)) = accessor(this) {
                            state.render(
                                &mut sb_ctx,
                                axis,
                                view,
                                style,
                                Some((whole, leading)),
                            );
                        }
                    },
                );
                return;
            }
        }
        #[cfg(not(feature = "gui"))]
        {
            let _ = (widget, accessor);
        }
        let mut bar_ctx = ctx.region(bar_size);
        self.render(&mut bar_ctx, axis, view, style, None);
    }

    /// Routes a mouse `chord` at `mouse_pos` along the scroll axis to thumb drag and click-jump handling.
    pub fn handle_input(&mut self, chord: &Chord, mouse_pos: f32, view: f32) -> ScrollbarInputResult {
        let sub_px = 8.0;
        match chord {
            chord!(LeftClick) => {
                if !self.can_scroll() {
                    return ScrollbarInputResult::Rejected;
                }
                let click_pos = mouse_pos;
                let thumb_top = self.thumb_top(view, sub_px);
                let thumb_height = self.thumb_height_snapped(view, sub_px);

                if click_pos >= thumb_top && click_pos < thumb_top + thumb_height {
                    self.dragging = true;
                    self.drag_offset = click_pos - thumb_top;
                    ScrollbarInputResult::Handled(None)
                } else {
                    let new_top = click_pos - thumb_height / 2.0;
                    let new_progress = self.progress_from_pos(new_top, view, sub_px);
                    self.remap = None;
                    self.progress = new_progress;
                    self.dragging = true;
                    self.drag_offset = thumb_height / 2.0;
                    tuie::dirty_paint();
                    ScrollbarInputResult::Handled(Some(new_progress))
                }
            }
            chord!(LeftDrag) => {
                if !self.dragging {
                    return ScrollbarInputResult::Rejected;
                }
                let new_top = mouse_pos - self.drag_offset;
                let new_progress = self.progress_from_pos(new_top, view, sub_px);
                self.remap = None;
                tuie::dirty_paint();
                if new_progress != self.progress {
                    self.progress = new_progress;
                    ScrollbarInputResult::Handled(Some(new_progress))
                } else {
                    ScrollbarInputResult::Handled(None)
                }
            }
            chord!(LeftRelease) => {
                self.dragging = false;
                ScrollbarInputResult::Handled(None)
            }
            _ => ScrollbarInputResult::Rejected,
        }
    }
}

fn piecewise_remap(progress: f32, anchor_in: f32, anchor_out: f32) -> f32 {
    if anchor_in <= 0.0 {
        return (anchor_out + progress * (1.0 - anchor_out)).clamp(0.0, 1.0);
    }
    if anchor_in >= 1.0 {
        return (progress * anchor_out).clamp(0.0, 1.0);
    }
    if progress <= anchor_in {
        (progress * (anchor_out / anchor_in)).clamp(0.0, 1.0)
    } else {
        (anchor_out + (progress - anchor_in) * ((1.0 - anchor_out) / (1.0 - anchor_in))).clamp(0.0, 1.0)
    }
}

/// Per-instance style overrides layered atop the global [`ScrollbarConfig`].
#[derive(Clone)]
pub struct ScrollbarStyle {
    /// Override for the thumb glyph style, or `None` to defer to the global config.
    pub thumb: Option<ScrollbarThumb>,
    /// Style layered over the global config thumb style. Empty fields inherit.
    pub thumb_style: Style,
    /// Override for the track glyph, or `None` to defer to the global config.
    pub track: Option<char>,
    /// Style layered over the global config track style. Empty fields inherit.
    pub track_style: Style,
}

impl ScrollbarStyle {
    /// Creates an empty override set that inherits every field from the global config.
    pub fn new() -> Self {
        Self {
            thumb: None,
            thumb_style: Style::new(),
            track: None,
            track_style: Style::new(),
        }
    }

    /// Merges these overrides with the global config into a concrete [`ScrollbarConfig`].
    pub fn get_resolved(&self) -> ScrollbarConfig {
        let cfg = config::get();
        ScrollbarConfig {
            thumb: self.thumb.unwrap_or(cfg.thumb),
            thumb_style: cfg.thumb_style.apply(self.thumb_style),
            track: self.track.unwrap_or(cfg.track),
            track_style: cfg.track_style.apply(self.track_style),
        }
    }

    /// Returns the configured [`ScrollbarThumb`] or the global default.
    pub fn get_resolved_thumb(&self) -> ScrollbarThumb {
        self.thumb.unwrap_or_else(|| config::get().thumb)
    }
}

/// Outcome of feeding an input chord to [`ScrollbarState::handle_input`].
pub enum ScrollbarInputResult {
    /// Chord was consumed, carrying the new progress when the thumb moved.
    Handled(Option<f32>),
    /// Chord was not relevant to the scrollbar.
    Rejected,
}
