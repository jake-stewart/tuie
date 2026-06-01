//! 2D grid container widget.

use crate::prelude::*;
use crate::widget::align::{AlignSpec, FlexAlign, Place};
use crate::widget::flex::{self, FlexItem};
use chord_macro::chord;
use sign::Directional;

/// Row or column track sizing specification.
#[derive(Clone, Copy, Default)]
pub struct Track {
    /// Minimum size in cells.
    pub min: Option<u16>,
    /// Preferred size in cells.
    pub pref: Option<u16>,
    /// Maximum size in cells.
    pub max: Option<u16>,
    /// Grow weight.
    pub flex: u8,
    /// Top edge for rows, left edge for cols.
    pub start: Option<&'static Border>,
    /// Bottom edge for rows, right edge for cols.
    pub end: Option<&'static Border>,
    /// Fill style for this track's cells and adjacent gaps.
    pub style: Style,
    /// Border-glyph style layered over the grid's `border_style` for edges adjacent to this track.
    pub border_style: Style,
}

impl Track {
    /// Returns a track sized from its children.
    pub const fn auto() -> Self {
        Self { min: None, pref: None, max: None, flex: 0, start: None, end: None, style: Style::new(), border_style: Style::new() }
    }

    /// Returns a fixed-size track of `n` cells.
    pub const fn fixed(n: u16) -> Self {
        Self { min: Some(n), pref: Some(n), max: Some(n), flex: 0, start: None, end: None, style: Style::new(), border_style: Style::new() }
    }

    /// Returns a child-sized track with flex `weight`.
    pub const fn grow(weight: u8) -> Self {
        Self { min: None, pref: None, max: None, flex: weight, start: None, end: None, style: Style::new(), border_style: Style::new() }
    }

    /// Sets the minimum size in cells.
    pub const fn min(mut self, n: u16) -> Self {
        self.min = Some(n);
        self
    }

    /// Sets the preferred size in cells.
    pub const fn pref(mut self, n: u16) -> Self {
        self.pref = Some(n);
        self
    }

    /// Sets the maximum size in cells.
    pub const fn max(mut self, n: u16) -> Self {
        self.max = Some(n);
        self
    }

    /// Sets the grow weight.
    pub const fn flex(mut self, w: u8) -> Self {
        self.flex = w;
        self
    }

    /// Sets the fill style for this track's cells and adjacent gaps.
    pub const fn style(mut self, s: Style) -> Self {
        self.style = s;
        self
    }

    /// Sets the border-glyph style for edges adjacent to this track.
    pub const fn border_style(mut self, s: Style) -> Self {
        self.border_style = s;
        self
    }
}

/// A single widget placement inside a [`Grid`] at `(row, col)` with optional span and per-cell overrides.
pub struct Cell {
    /// The child widget.
    pub widget: Box<dyn Widget>,
    /// Origin row index.
    pub row: u16,
    /// Origin column index.
    pub col: u16,
    /// Row span.
    pub row_span: u16,
    /// Column span.
    pub col_span: u16,
    /// Per-cell border applied to all four edges.
    pub border: Option<&'static Border>,
    /// Per-cell padding override.
    pub padding: Option<Spacing>,
    /// Fill style layered over the combined row and column style.
    pub style: Style,
}

impl Cell {
    /// Creates a single-cell placement at `(row, col)` with no overrides.
    pub fn new(row: u16, col: u16, widget: Box<dyn Widget>) -> Self {
        Self {
            widget,
            row,
            col,
            row_span: 1,
            col_span: 1,
            border: None,
            padding: None,
            style: Style::new(),
        }
    }

    /// Sets the row and column span.
    pub fn span(mut self, row_span: u16, col_span: u16) -> Self {
        self.row_span = row_span.max(1);
        self.col_span = col_span.max(1);
        self
    }

    /// Sets the per-cell fill style layered over the combined row and column style.
    pub fn style(mut self, s: Style) -> Self {
        self.style = s;
        self
    }

    /// Sets the per-cell border applied to all four edges.
    pub fn border(mut self, b: &'static Border) -> Self {
        self.border = Some(b);
        self
    }

    /// Sets or clears the per-cell border.
    pub fn border_opt(mut self, b: Option<&'static Border>) -> Self {
        self.border = b;
        self
    }

    /// Sets the per-cell padding override.
    pub fn padding(mut self, p: Spacing) -> Self {
        self.padding = Some(p);
        self
    }

    /// Sets or clears the per-cell padding override.
    pub fn padding_opt(mut self, p: Option<Spacing>) -> Self {
        self.padding = p;
        self
    }
}

/// Guard returned by [`Grid::get_cell_mut`] for mutating a cell's style, border, and padding.
pub struct CellMut<'a> {
    grid: &'a mut Grid,
    idx: usize,
}

impl CellMut<'_> {
    /// Sets the per-cell fill style layered over the combined row and column style.
    pub fn set_style(&mut self, s: Style) {
        if self.grid.cells[self.idx].style == s {
            return;
        }
        self.grid.cells[self.idx].style = s;
        self.grid.dirty_paint();
    }

    /// Sets or clears the per-cell border.
    pub fn set_border(&mut self, b: Option<&'static Border>) {
        if self.grid.cells[self.idx].border == b {
            return;
        }
        self.grid.cells[self.idx].border = b;
        self.grid.dirty_layout();
    }

    /// Sets or clears the per-cell padding override.
    pub fn set_padding(&mut self, p: Option<Spacing>) {
        if self.grid.cells[self.idx].padding == p {
            return;
        }
        self.grid.cells[self.idx].padding = p;
        self.grid.dirty_layout();
    }

    /// Returns the per-cell fill style.
    pub fn get_style(&self) -> Style {
        self.grid.cells[self.idx].style
    }

    /// Returns the per-cell border override.
    pub fn get_border(&self) -> Option<&'static Border> {
        self.grid.cells[self.idx].border
    }

    /// Returns the per-cell padding override.
    pub fn get_padding(&self) -> Option<Spacing> {
        self.grid.cells[self.idx].padding
    }
}

struct DragState {
    axis: Axis2D,
    boundary: usize,
    grab_offset: i32,
}

#[derive(Default)]
struct SolveState {
    cols: Vec<FlexItem>,
    rows: Vec<FlexItem>,
    col_off: Vec<u16>,
    row_off: Vec<u16>,
}

impl SolveState {
    fn resize(&mut self, n_cols: usize, n_rows: usize) {
        self.cols.clear();
        self.cols.resize(n_cols, FlexItem::new(0, 0, u16::MAX, 0));
        self.rows.clear();
        self.rows.resize(n_rows, FlexItem::new(0, 0, u16::MAX, 0));
        self.col_off.clear();
        self.col_off.resize(n_cols + 1, 0);
        self.row_off.clear();
        self.row_off.resize(n_rows + 1, 0);
    }
}

/// 2D grid container that places children into rows and columns.
pub struct Grid {
    layout: Layout,
    cells: Vec<Cell>,
    columns: Vec<Track>,
    rows: Vec<Track>,
    col_gap: u8,
    row_gap: u8,
    align: AlignSpec,
    state: SolveState,
    border: Option<&'static Border>,
    row_borders: Option<&'static Border>,
    col_borders: Option<&'static Border>,
    border_style: Style,
    resizable_cols: bool,
    resizable_rows: bool,
    col_drag_size: Vec<Option<u16>>,
    row_drag_size: Vec<Option<u16>>,
    drag: Option<DragState>,
    padding: Spacing,
}

impl Grid {
    fn cell_covering(&self, row: usize, col: usize) -> Option<&Cell> {
        self.cells.iter().find(|c| {
            let r0 = c.row as usize;
            let c0 = c.col as usize;
            let rs = c.row_span.max(1) as usize;
            let cs = c.col_span.max(1) as usize;
            row >= r0 && row < r0 + rs && col >= c0 && col < c0 + cs
        })
    }

    fn resolve_h_edge(&self, r: usize, c: usize) -> Option<&'static Border> {
        let n_rows = self.rows.len();
        if r > 0 && r < n_rows {
            if let (Some(a), Some(b)) = (self.cell_covering(r - 1, c), self.cell_covering(r, c)) {
                if std::ptr::eq(a, b) {
                    return None;
                }
            }
        }
        if r > 0 {
            if let Some(cell) = self.cell_covering(r - 1, c) {
                if let Some(b) = cell.border {
                    return Some(b);
                }
            }
        }
        if r < n_rows {
            if let Some(cell) = self.cell_covering(r, c) {
                if let Some(b) = cell.border {
                    return Some(b);
                }
            }
        }
        if r > 0 {
            if let Some(b) = self.rows[r - 1].end {
                return Some(b);
            }
        }
        if r < n_rows {
            if let Some(b) = self.rows[r].start {
                return Some(b);
            }
        }
        if r > 0 && r < n_rows {
            if let Some(b) = self.row_borders {
                return Some(b);
            }
        }
        if (r == 0 || r == n_rows) && n_rows > 0 {
            if let Some(b) = self.border {
                return Some(b);
            }
        }
        None
    }

    fn resolve_v_edge(&self, r: usize, c: usize) -> Option<&'static Border> {
        let n_cols = self.columns.len();
        if c > 0 && c < n_cols {
            if let (Some(a), Some(b)) = (self.cell_covering(r, c - 1), self.cell_covering(r, c)) {
                if std::ptr::eq(a, b) {
                    return None;
                }
            }
        }
        if c > 0 {
            if let Some(cell) = self.cell_covering(r, c - 1) {
                if let Some(b) = cell.border {
                    return Some(b);
                }
            }
        }
        if c < n_cols {
            if let Some(cell) = self.cell_covering(r, c) {
                if let Some(b) = cell.border {
                    return Some(b);
                }
            }
        }
        if c > 0 {
            if let Some(b) = self.columns[c - 1].end {
                return Some(b);
            }
        }
        if c < n_cols {
            if let Some(b) = self.columns[c].start {
                return Some(b);
            }
        }
        if c > 0 && c < n_cols {
            if let Some(b) = self.col_borders {
                return Some(b);
            }
        }
        if (c == 0 || c == n_cols) && n_cols > 0 {
            if let Some(b) = self.border {
                return Some(b);
            }
        }
        None
    }

    fn row_sep(&self, between: usize) -> u8 {
        let r = between + 1;
        (0..self.columns.len()).any(|c| self.resolve_h_edge(r, c).is_some()) as u8
    }

    fn col_sep(&self, between: usize) -> u8 {
        let c = between + 1;
        (0..self.rows.len()).any(|r| self.resolve_v_edge(r, c).is_some()) as u8
    }

    fn pre_pad_x(&self) -> u8 {
        (0..self.rows.len()).any(|r| self.resolve_v_edge(r, 0).is_some()) as u8
    }
    fn post_pad_x(&self) -> u8 {
        let c = self.columns.len();
        (0..self.rows.len()).any(|r| self.resolve_v_edge(r, c).is_some()) as u8
    }
    fn pre_pad_y(&self) -> u8 {
        (0..self.columns.len()).any(|c| self.resolve_h_edge(0, c).is_some()) as u8
    }
    fn post_pad_y(&self) -> u8 {
        let r = self.rows.len();
        (0..self.columns.len()).any(|c| self.resolve_h_edge(r, c).is_some()) as u8
    }

    fn col_sep_total(&self) -> u32 {
        let n = self.columns.len();
        (0..n.saturating_sub(1)).map(|i| self.col_sep(i) as u32).sum()
    }

    fn row_sep_total(&self) -> u32 {
        let n = self.rows.len();
        (0..n.saturating_sub(1)).map(|i| self.row_sep(i) as u32).sum()
    }

    fn col_sep_total_in(&self, start: usize, n: usize) -> u32 {
        (0..n.saturating_sub(1)).map(|i| self.col_sep(start + i) as u32).sum()
    }

    fn row_sep_total_in(&self, start: usize, n: usize) -> u32 {
        (0..n.saturating_sub(1)).map(|i| self.row_sep(start + i) as u32).sum()
    }

    fn col_chrome_total(&self) -> u32 {
        let n = self.columns.len();
        let pads = self.pre_pad_x() as u32 + self.post_pad_x() as u32;
        let gaps = n.saturating_sub(1) as u32 * self.col_gap as u32;
        pads + gaps + self.col_sep_total()
    }
    fn row_chrome_total(&self) -> u32 {
        let n = self.rows.len();
        let pads = self.pre_pad_y() as u32 + self.post_pad_y() as u32;
        let gaps = n.saturating_sub(1) as u32 * self.row_gap as u32;
        pads + gaps + self.row_sep_total()
    }

    fn distribute_min(track_mins: &mut [u16], start: usize, n: usize, child_min: u32, gap: u8, sep_total: u32) {
        if n == 0 {
            return;
        }
        let n = n.min(track_mins.len().saturating_sub(start));
        if n == 0 {
            return;
        }
        let inner_gap = n.saturating_sub(1) as u32 * gap as u32 + sep_total;
        let current: u32 = (0..n).map(|i| track_mins[start + i] as u32).sum::<u32>()
            .saturating_add(inner_gap);
        if child_min <= current {
            return;
        }
        let deficit = child_min - current;
        let per = deficit / n as u32;
        let rem = deficit % n as u32;
        for i in 0..n {
            let add = per + if (i as u32) < rem { 1 } else { 0 };
            track_mins[start + i] = (track_mins[start + i] as u32 + add).min(u16::MAX as u32) as u16;
        }
    }

    fn cell_pad(&self, cell: &Cell) -> Spacing {
        cell.padding.unwrap_or(self.padding)
    }

    fn col_mins(&self) -> Vec<u16> {
        let n_cols = self.columns.len();
        let mut col_min: Vec<u16> = self.columns.iter().map(|t| t.min.unwrap_or(0)).collect();
        for cell in self.cells.iter() {
            let cmin = cell.widget.get_layout().constraints.min_size;
            let c0 = cell.col as usize;
            if c0 < n_cols {
                let cs = (cell.col_span.max(1) as usize).min(n_cols - c0);
                let sep = self.col_sep_total_in(c0, cs);
                Self::distribute_min(&mut col_min, c0, cs, cmin.x as u32, self.col_gap, sep);
            }
        }
        for (i, t) in self.columns.iter().enumerate() {
            if let Some(m) = t.min {
                col_min[i] = col_min[i].max(m);
            }
            if let Some(m) = t.max {
                col_min[i] = col_min[i].min(m);
            }
        }
        let mut pad_overhead: Vec<u16> = vec![0; n_cols];
        for cell in self.cells.iter() {
            let pad = self.cell_pad(cell).get_total();
            let c0 = cell.col as usize;
            if c0 < n_cols && pad.x > 0 {
                let cs = (cell.col_span.max(1) as usize).min(n_cols - c0);
                let sep = self.col_sep_total_in(c0, cs);
                Self::distribute_min(&mut pad_overhead, c0, cs, pad.x as u32, self.col_gap, sep);
            }
        }
        for i in 0..n_cols {
            col_min[i] = col_min[i].saturating_add(pad_overhead[i]);
        }
        col_min
    }

    fn col_prefs(&self, col_min: &[u16]) -> Vec<u16> {
        let n_cols = self.columns.len();
        let mut col_pref: Vec<u16> = col_min.to_vec();
        for cell in self.cells.iter() {
            let cpref = cell.widget.get_layout().constraints.preferred_size;
            let c0 = cell.col as usize;
            if c0 < n_cols {
                let cs = (cell.col_span.max(1) as usize).min(n_cols - c0);
                let sep = self.col_sep_total_in(c0, cs);
                Self::distribute_min(&mut col_pref, c0, cs, cpref.x as u32, self.col_gap, sep);
            }
        }
        for (i, t) in self.columns.iter().enumerate() {
            if let Some(p) = t.pref {
                col_pref[i] = p;
            }
            col_pref[i] = col_pref[i].max(col_min[i]);
            if let Some(m) = t.max {
                col_pref[i] = col_pref[i].min(m).max(col_min[i]);
            }
        }
        col_pref
    }

    fn row_prefs_from_constraints(&self, row_min: &[u16]) -> Vec<u16> {
        let n_rows = self.rows.len();
        let mut row_pref: Vec<u16> = row_min.to_vec();
        for cell in self.cells.iter() {
            let cpref = cell.widget.get_layout().constraints.preferred_size;
            let r0 = cell.row as usize;
            if r0 < n_rows {
                let rs = (cell.row_span.max(1) as usize).min(n_rows - r0);
                let sep = self.row_sep_total_in(r0, rs);
                Self::distribute_min(&mut row_pref, r0, rs, cpref.y as u32, self.row_gap, sep);
            }
        }
        for (i, t) in self.rows.iter().enumerate() {
            if let Some(p) = t.pref {
                row_pref[i] = p;
            }
            row_pref[i] = row_pref[i].max(row_min[i]);
            if let Some(m) = t.max {
                row_pref[i] = row_pref[i].min(m).max(row_min[i]);
            }
        }
        row_pref
    }

    fn row_mins_from_constraints(&self) -> Vec<u16> {
        let n_rows = self.rows.len();
        let mut row_min: Vec<u16> = self.rows.iter().map(|t| t.min.unwrap_or(0)).collect();
        for cell in self.cells.iter() {
            let cmin = cell.widget.get_layout().constraints.min_size;
            let r0 = cell.row as usize;
            if r0 < n_rows {
                let rs = (cell.row_span.max(1) as usize).min(n_rows - r0);
                let sep = self.row_sep_total_in(r0, rs);
                Self::distribute_min(&mut row_min, r0, rs, cmin.y as u32, self.row_gap, sep);
            }
        }
        for (i, t) in self.rows.iter().enumerate() {
            if let Some(m) = t.min {
                row_min[i] = row_min[i].max(m);
            }
            if let Some(m) = t.max {
                row_min[i] = row_min[i].min(m);
            }
        }
        let mut pad_overhead: Vec<u16> = vec![0; n_rows];
        for cell in self.cells.iter() {
            let pad = self.cell_pad(cell).get_total();
            let r0 = cell.row as usize;
            if r0 < n_rows && pad.y > 0 {
                let rs = (cell.row_span.max(1) as usize).min(n_rows - r0);
                let sep = self.row_sep_total_in(r0, rs);
                Self::distribute_min(&mut pad_overhead, r0, rs, pad.y as u32, self.row_gap, sep);
            }
        }
        for i in 0..n_rows {
            row_min[i] = row_min[i].saturating_add(pad_overhead[i]);
        }
        row_min
    }

    fn row_mins_at_cols(&self, state: &SolveState) -> Vec<u16> {
        let n_rows = self.rows.len();
        let mut row_min: Vec<u16> = self.rows.iter().map(|t| t.min.unwrap_or(0)).collect();
        for cell in self.cells.iter() {
            let r0 = cell.row as usize;
            let c0 = cell.col as usize;
            if r0 >= n_rows {
                continue;
            }
            let cs = (cell.col_span.max(1) as usize).min(state.cols.len().saturating_sub(c0));
            let mut w: u32 = 0;
            for i in 0..cs {
                if i > 0 {
                    w = w.saturating_add(self.col_gap as u32);
                    w = w.saturating_add(self.col_sep(c0 + i - 1) as u32);
                }
                w = w.saturating_add(state.cols[c0 + i].target as u32);
            }
            let pad = self.cell_pad(cell).get_total();
            let child_w = w.saturating_sub(pad.x as u32).min(u16::MAX as u32) as u16;
            let measured = flow_child_measure(&*cell.widget, Vec2::new(child_w, u16::MAX));
            let cmin_y = cell.widget.get_layout().constraints.min_size.y as u32;
            let need = (measured.y as u32).max(cmin_y);
            let rs = (cell.row_span.max(1) as usize).min(n_rows - r0);
            let sep = self.row_sep_total_in(r0, rs);
            Self::distribute_min(&mut row_min, r0, rs, need, self.row_gap, sep);
        }
        for (i, t) in self.rows.iter().enumerate() {
            if let Some(m) = t.min {
                row_min[i] = row_min[i].max(m);
            }
            if let Some(m) = t.max {
                row_min[i] = row_min[i].min(m);
            }
        }
        let mut pad_overhead: Vec<u16> = vec![0; n_rows];
        for cell in self.cells.iter() {
            let pad = self.cell_pad(cell).get_total();
            let r0 = cell.row as usize;
            if r0 < n_rows && pad.y > 0 {
                let rs = (cell.row_span.max(1) as usize).min(n_rows - r0);
                let sep = self.row_sep_total_in(r0, rs);
                Self::distribute_min(&mut pad_overhead, r0, rs, pad.y as u32, self.row_gap, sep);
            }
        }
        for i in 0..n_rows {
            row_min[i] = row_min[i].saturating_add(pad_overhead[i]);
        }
        row_min
    }

    fn solve_tracks_into(&self, allocated: Vec2<u16>, state: &mut SolveState) {
        let n_cols = self.columns.len();
        let n_rows = self.rows.len();
        state.resize(n_cols, n_rows);

        let col_min = self.col_mins();
        let col_pref = self.col_prefs(&col_min);
        for (i, track) in self.columns.iter().enumerate() {
            let track_max = track.max.unwrap_or(u16::MAX).max(col_min[i]);
            let (basis, item_min, item_max, flex) = match self.col_drag_size.get(i).copied().flatten() {
                Some(p) => {
                    let pinned = p.max(col_min[i]).min(track_max);
                    (pinned, pinned, pinned, 0)
                }
                None => {
                    let pref = col_pref[i].max(col_min[i]).min(track_max);
                    (pref, col_min[i], track_max, track.flex)
                }
            };
            state.cols[i] = FlexItem::new(basis, item_min, item_max, flex);
        }
        flex::resolve(&mut state.cols, allocated.x, self.col_chrome_total());
        let col_gap = self.col_gap;
        Self::bake_offsets(
            &state.cols,
            &mut state.col_off,
            allocated.x,
            self.pre_pad_x() as u16,
            self.post_pad_x() as u16,
            |i| col_gap as u16 + self.col_sep(i) as u16,
            self.align.get_place_x(),
        );

        let row_min = self.row_mins_at_cols(state);
        for (i, track) in self.rows.iter().enumerate() {
            let track_max = track.max.unwrap_or(u16::MAX).max(row_min[i]);
            let (basis, item_min, item_max, flex) = match self.row_drag_size.get(i).copied().flatten() {
                Some(p) => {
                    let pinned = p.max(row_min[i]).min(track_max);
                    (pinned, pinned, pinned, 0)
                }
                None => {
                    let pref = track.pref.unwrap_or(row_min[i]).max(row_min[i]).min(track_max);
                    (pref, row_min[i], track_max, track.flex)
                }
            };
            state.rows[i] = FlexItem::new(basis, item_min, item_max, flex);
        }
        flex::resolve(&mut state.rows, allocated.y, self.row_chrome_total());
        let row_gap = self.row_gap;
        Self::bake_offsets(
            &state.rows,
            &mut state.row_off,
            allocated.y,
            self.pre_pad_y() as u16,
            self.post_pad_y() as u16,
            |i| row_gap as u16 + self.row_sep(i) as u16,
            self.align.get_place_y(),
        );
    }

    fn bake_offsets(
        tracks: &[FlexItem],
        off: &mut [u16],
        allocated: u16,
        pre_pad: u16,
        post_pad: u16,
        gap_at: impl Fn(usize) -> u16,
        place: Place,
    ) {
        let n = tracks.len();
        if off.is_empty() {
            return;
        }
        let track_sum: u32 = tracks.iter().map(|t| t.target as u32).sum();
        let gap_sum: u32 = (0..n.saturating_sub(1)).map(|i| gap_at(i) as u32).sum();
        let used: u32 = track_sum
            .saturating_add(gap_sum)
            .saturating_add(pre_pad as u32)
            .saturating_add(post_pad as u32);
        let slack = (allocated as u32).saturating_sub(used);
        let mut acc: u32 = pre_pad as u32;
        for i in 0..n {
            let iu = i as u32;
            let pre_gap: u32 = match place {
                Place::Evenly if n >= 1 => {
                    slack * (iu + 1) / (n as u32 + 1) - slack * iu / (n as u32 + 1)
                }
                Place::Apart if n >= 2 => {
                    if i == 0 {
                        0
                    } else {
                        slack * iu / (n as u32 - 1) - slack * (iu - 1) / (n as u32 - 1)
                    }
                }
                Place::Center if i == 0 => slack / 2,
                Place::End if i == 0 => slack,
                _ => 0,
            };
            acc = acc.saturating_add(pre_gap);
            off[i] = acc.min(u16::MAX as u32) as u16;
            acc = acc.saturating_add(tracks[i].target as u32);
            if i + 1 < n {
                acc = acc.saturating_add(gap_at(i) as u32);
            }
        }
        off[n] = acc.min(u16::MAX as u32) as u16;
    }

    fn cell_size(&self, state: &SolveState, row: u16, col: u16, row_span: u16, col_span: u16) -> Vec2<u16> {
        let w = Self::span_size(&state.cols, &state.col_off, col as usize, col_span.max(1) as usize);
        let h = Self::span_size(&state.rows, &state.row_off, row as usize, row_span.max(1) as usize);
        Vec2::new(w, h)
    }

    fn span_size(tracks: &[FlexItem], off: &[u16], start: usize, span: usize) -> u16 {
        let n = tracks.len();
        if start >= n || span == 0 {
            return 0;
        }
        let end = (start + span).min(n) - 1;
        let extent = (off[end] as u32).saturating_add(tracks[end].target as u32)
            .saturating_sub(off[start] as u32);
        extent.min(u16::MAX as u32) as u16
    }

    fn cell_origin(&self, row: u16, col: u16) -> Vec2<u16> {
        let x = self.state.col_off.get(col as usize).copied().unwrap_or(0);
        let y = self.state.row_off.get(row as usize).copied().unwrap_or(0);
        Vec2::new(x, y)
    }

    fn total_extent(&self) -> Vec2<u16> {
        Vec2::new(
            Self::natural_extent_sum(&self.state.cols, self.col_chrome_total()),
            Self::natural_extent_sum(&self.state.rows, self.row_chrome_total()),
        )
    }

    fn natural_extent_sum(tracks: &[FlexItem], chrome: u32) -> u16 {
        let sum: u32 = tracks.iter().map(|t| t.target as u32).sum::<u32>()
            .saturating_add(chrome);
        sum.min(u16::MAX as u32) as u16
    }

    fn natural_min_extent_sum(tracks: &[FlexItem], chrome: u32) -> u16 {
        let sum: u32 = tracks.iter().map(|t| t.basis as u32).sum::<u32>()
            .saturating_add(chrome);
        sum.min(u16::MAX as u32) as u16
    }

    fn col_gutter_x(&self, boundary: usize) -> Option<std::ops::Range<i32>> {
        if boundary == 0 || boundary >= self.columns.len() {
            return None;
        }
        let start = self.state.col_off[boundary - 1] as i32
            + self.state.cols[boundary - 1].target as i32;
        let end = self.state.col_off[boundary] as i32;
        if end > start {
            Some(start..end)
        } else {
            None
        }
    }

    fn row_gutter_y(&self, boundary: usize) -> Option<std::ops::Range<i32>> {
        if boundary == 0 || boundary >= self.rows.len() {
            return None;
        }
        let start = self.state.row_off[boundary - 1] as i32
            + self.state.rows[boundary - 1].target as i32;
        let end = self.state.row_off[boundary] as i32;
        if end > start {
            Some(start..end)
        } else {
            None
        }
    }

    fn find_divider_at(&self, pos: Vec2<i32>) -> Option<(Axis2D, usize, i32)> {
        let n_cols = self.columns.len();
        let n_rows = self.rows.len();
        let content_h = self.state.row_off.get(n_rows).copied().unwrap_or(0) as i32;
        let content_w = self.state.col_off.get(n_cols).copied().unwrap_or(0) as i32;
        if self.resizable_cols && (0..content_h).contains(&pos.y) {
            for b in 1..n_cols {
                if let Some(r) = self.col_gutter_x(b) {
                    if r.contains(&pos.x) {
                        let center = (r.start + r.end) / 2;
                        return Some((Axis2D::X, b, center));
                    }
                }
            }
        }
        if self.resizable_rows && (0..content_w).contains(&pos.x) {
            for b in 1..n_rows {
                if let Some(r) = self.row_gutter_y(b) {
                    if r.contains(&pos.y) {
                        let center = (r.start + r.end) / 2;
                        return Some((Axis2D::Y, b, center));
                    }
                }
            }
        }
        None
    }

    fn divider_center(&self, axis: Axis2D, boundary: usize) -> Option<i32> {
        match axis {
            Axis2D::X => self.col_gutter_x(boundary).map(|r| (r.start + r.end) / 2),
            Axis2D::Y => self.row_gutter_y(boundary).map(|r| (r.start + r.end) / 2),
        }
    }

    fn cascade_shrink_seq(
        sizes: &mut [i32],
        mins: &[i32],
        indices: impl Iterator<Item = usize>,
        mut remaining: i32,
    ) -> i32 {
        let mut taken = 0;
        for i in indices {
            let give = (sizes[i] - mins[i]).max(0).min(remaining);
            sizes[i] -= give;
            remaining -= give;
            taken += give;
            if remaining == 0 {
                break;
            }
        }
        taken
    }

    fn apply_drag(&mut self, axis: Axis2D, boundary: usize, delta: i32) {
        if delta == 0 {
            return;
        }
        let (a_idx, b_idx) = (boundary - 1, boundary);
        let (mut sizes, mins): (Vec<i32>, Vec<i32>) = match axis {
            Axis2D::X => {
                let mins = self.col_mins();
                (
                    self.state.cols.iter().map(|c| c.target as i32).collect(),
                    mins.iter().map(|&m| m as i32).collect(),
                )
            }
            Axis2D::Y => {
                let mins = self.row_mins_at_cols(&self.state);
                (
                    self.state.rows.iter().map(|r| r.target as i32).collect(),
                    mins.iter().map(|&m| m as i32).collect(),
                )
            }
        };
        let n = sizes.len();
        let grow_idx = if delta > 0 { a_idx } else { b_idx };
        let grow_max = match axis {
            Axis2D::X => self.columns[grow_idx].max.unwrap_or(u16::MAX) as i32,
            Axis2D::Y => self.rows[grow_idx].max.unwrap_or(u16::MAX) as i32,
        };
        let headroom = (grow_max - sizes[grow_idx]).max(0);
        let amount = delta.abs().min(headroom);
        let taken = if delta > 0 {
            Self::cascade_shrink_seq(&mut sizes, &mins, b_idx..n, amount)
        } else {
            Self::cascade_shrink_seq(&mut sizes, &mins, (0..=a_idx).rev(), amount)
        };
        sizes[grow_idx] += taken;
        let slot = match axis {
            Axis2D::X => &mut self.col_drag_size,
            Axis2D::Y => &mut self.row_drag_size,
        };
        slot.clear();
        slot.extend(sizes.iter().map(|&s| Some(s as u16)));
        self.dirty_layout();
    }

    fn pos_in_rect(child: &dyn Widget, pos: Vec2<f32>) -> bool {
        let child_pos = child.get_pos();
        let child_size = child.get_rect_size().map(|v| v as i32);
        Axis2D::all(|a| {
            pos[a] >= child_pos[a] as f32 && pos[a] < (child_pos[a] + child_size[a]) as f32
        })
    }

    fn render_track_styles(&self, ctx: &mut crate::render::RenderContext) {
        let n_cols = self.columns.len();
        let n_rows = self.rows.len();
        if n_cols == 0 || n_rows == 0 {
            return;
        }

        let inner_x_start = self.state.col_off[0] as i32;
        let inner_y_start = self.state.row_off[0] as i32;
        let inner_x_end = self.state.col_off[n_cols - 1] as i32
            + self.state.cols[n_cols - 1].target as i32;
        let inner_y_end = self.state.row_off[n_rows - 1] as i32
            + self.state.rows[n_rows - 1].target as i32;

        let inner_w = (inner_x_end - inner_x_start).max(0) as u16;
        let inner_h = (inner_y_end - inner_y_start).max(0) as u16;

        for c in 0..n_cols {
            let s = self.columns[c].style;
            if s.is_empty() {
                continue;
            }
            let x_start = self.state.col_off[c] as i32;
            let w = self.state.cols[c].target;
            if w == 0 || inner_h == 0 {
                continue;
            }
            ctx.set_style(s);
            ctx.move_to(Vec2::new(x_start, inner_y_start));
            ctx.region(Vec2::new(w, inner_h)).clear();
        }

        for r in 0..n_rows {
            let s = self.rows[r].style;
            if s.is_empty() {
                continue;
            }
            let y_start = self.state.row_off[r] as i32;
            let h = self.state.rows[r].target;
            if inner_w == 0 || h == 0 {
                continue;
            }
            ctx.set_style(s);
            ctx.move_to(Vec2::new(inner_x_start, y_start));
            ctx.region(Vec2::new(inner_w, h)).clear();
        }

        for cell in self.cells.iter() {
            let rs = cell.row_span.max(1);
            let cs = cell.col_span.max(1);
            let r0 = cell.row as usize;
            let c0 = cell.col as usize;
            if r0 >= n_rows || c0 >= n_cols {
                continue;
            }
            let single = rs == 1 && cs == 1;
            if single && cell.style.is_empty() {
                continue;
            }
            let combined = self.columns[c0].style.apply(self.rows[r0].style).apply(cell.style);
            if combined.is_empty() {
                continue;
            }
            let origin = self.cell_origin(cell.row, cell.col).map(|v| v as i32);
            let size = self.cell_size(&self.state, cell.row, cell.col, rs, cs);
            if size.x == 0 || size.y == 0 {
                continue;
            }
            ctx.set_style(combined);
            ctx.move_to(origin);
            ctx.region(size).clear();
        }
    }

    fn render_borders(&self, ctx: &mut crate::render::RenderContext) {
        let n_cols = self.columns.len();
        let n_rows = self.rows.len();
        if n_cols == 0 || n_rows == 0 {
            return;
        }
        let cfg_style = crate::render::border::config::get().style;
        let base = cfg_style.apply(self.border_style);

        let half_col_gap = self.col_gap as u16 / 2;
        let half_row_gap = self.row_gap as u16 / 2;
        let mut h_div_y: Vec<u16> = Vec::with_capacity(n_rows + 1);
        let mut v_div_x: Vec<u16> = Vec::with_capacity(n_cols + 1);
        v_div_x.push(0);
        for c in 1..n_cols {
            let after_prev = self.state.col_off[c - 1]
                .saturating_add(self.state.cols[c - 1].target);
            v_div_x.push(after_prev.saturating_add(half_col_gap));
        }
        v_div_x.push(self.state.col_off[n_cols]);
        h_div_y.push(0);
        for r in 1..n_rows {
            let after_prev = self.state.row_off[r - 1]
                .saturating_add(self.state.rows[r - 1].target);
            h_div_y.push(after_prev.saturating_add(half_row_gap));
        }
        h_div_y.push(self.state.row_off[n_rows]);

        for r in 0..=n_rows {
            let y = h_div_y[r] as i32;
            for c in 0..n_cols {
                if let Some(b) = self.resolve_h_edge(r, c) {
                    let mut s = base;
                    s = s.apply(self.columns[c].border_style);
                    if r > 0 {
                        s = s.apply(self.rows[r - 1].border_style);
                    }
                    if r < n_rows {
                        s = s.apply(self.rows[r].border_style);
                    }
                    ctx.set_style(s);
                    let x_start = v_div_x[c] as i32 + 1;
                    let x_end = v_div_x[c + 1] as i32;
                    let g = b.get_edge(Axis2D::Y);
                    for x in x_start..x_end {
                        ctx.move_to(Vec2::new(x, y));
                        write!(ctx, "{}", g);
                    }
                }
            }
        }

        for c in 0..=n_cols {
            let x = v_div_x[c] as i32;
            for r in 0..n_rows {
                if let Some(b) = self.resolve_v_edge(r, c) {
                    let mut s = base;
                    s = s.apply(self.rows[r].border_style);
                    if c > 0 {
                        s = s.apply(self.columns[c - 1].border_style);
                    }
                    if c < n_cols {
                        s = s.apply(self.columns[c].border_style);
                    }
                    ctx.set_style(s);
                    let y_start = h_div_y[r] as i32 + 1;
                    let y_end = h_div_y[r + 1] as i32;
                    let g = b.get_edge(Axis2D::X);
                    for y in y_start..y_end {
                        ctx.move_to(Vec2::new(x, y));
                        write!(ctx, "{}", g);
                    }
                }
            }
        }

        ctx.set_style(base);
        for r in 0..=n_rows {
            for c in 0..=n_cols {
                let l = if c > 0 { self.resolve_h_edge(r, c - 1) } else { None };
                let rt = if c < n_cols { self.resolve_h_edge(r, c) } else { None };
                let u = if r > 0 { self.resolve_v_edge(r - 1, c) } else { None };
                let d = if r < n_rows { self.resolve_v_edge(r, c) } else { None };
                let on_sep_row = r > 0 && r < n_rows && self.row_sep(r - 1) > 0;
                let on_sep_col = c > 0 && c < n_cols && self.col_sep(c - 1) > 0;
                let draw_v = if r == 0 || r == n_rows || on_sep_row {
                    u.is_some() || d.is_some()
                } else {
                    d.is_some()
                };
                let draw_h = if c == 0 || c == n_cols || on_sep_col {
                    l.is_some() || rt.is_some()
                } else {
                    rt.is_some()
                };
                let glyph = match (draw_h, draw_v) {
                    (false, false) => continue,
                    (true, true) => crate::render::border::junction(
                        l.unwrap_or(Border::HIDDEN),
                        rt.unwrap_or(Border::HIDDEN),
                        u.unwrap_or(Border::HIDDEN),
                        d.unwrap_or(Border::HIDDEN),
                    )
                    .or_else(|| u.or(d).map(|b| b.get_edge(Axis2D::X))),
                    (true, false) => l.or(rt).map(|b| b.get_edge(Axis2D::Y)),
                    (false, true) => u.or(d).map(|b| b.get_edge(Axis2D::X)),
                };
                if let Some(g) = glyph {
                    ctx.move_to(Vec2::new(v_div_x[c] as i32, h_div_y[r] as i32));
                    write!(ctx, "{}", g);
                }
            }
        }
    }
}

impl Widget for Grid {
    fn get_layout(&self) -> &Layout {
        &self.layout
    }

    fn get_layout_mut(&mut self) -> &mut Layout {
        &mut self.layout
    }

    fn get_name(&self) -> &'static str {
        "Grid"
    }

    fn measure_constraints(&mut self) -> Constraints {
        self.each_child_mut(&mut constrain_child, Sign::Positive);
        let col_min = self.col_mins();
        let col_pref = self.col_prefs(&col_min);
        let row_min = self.row_mins_from_constraints();
        let row_pref = self.row_prefs_from_constraints(&row_min);
        let chrome_x = self.col_chrome_total();
        let chrome_y = self.row_chrome_total();
        let min_x: u32 = col_min.iter().map(|&v| v as u32).sum::<u32>().saturating_add(chrome_x);
        let min_y: u32 = row_min.iter().map(|&v| v as u32).sum::<u32>().saturating_add(chrome_y);
        let pref_x: u32 = col_pref.iter().map(|&v| v as u32).sum::<u32>().saturating_add(chrome_x);
        let pref_y: u32 = row_pref.iter().map(|&v| v as u32).sum::<u32>().saturating_add(chrome_y);
        let col_bounded = self.columns.iter().all(|t| t.max.is_some() && t.flex == 0);
        let row_bounded = self.rows.iter().all(|t| t.max.is_some() && t.flex == 0);
        let min_size = Vec2::new(
            min_x.min(u16::MAX as u32) as u16,
            min_y.min(u16::MAX as u32) as u16,
        );
        let preferred_size = Vec2::new(
            pref_x.min(u16::MAX as u32) as u16,
            pref_y.min(u16::MAX as u32) as u16,
        );
        let max_size = Vec2::new(
            if col_bounded { preferred_size.x } else { u16::MAX },
            if row_bounded { preferred_size.y } else { u16::MAX },
        );
        Constraints {
            min_size,
            max_size,
            preferred_size,
        }
    }

    fn layout_flow(&mut self, allocated: Vec2<u16>) -> Vec2<u16> {
        let mut state = std::mem::take(&mut self.state);
        self.solve_tracks_into(allocated, &mut state);
        self.state = state;
        let cell_sizes: Vec<Vec2<u16>> = self.cells.iter()
            .map(|cell| {
                let cs = self.cell_size(&self.state, cell.row, cell.col, cell.row_span, cell.col_span);
                let pad = self.cell_pad(cell).get_total();
                Vec2::new(cs.x.saturating_sub(pad.x), cs.y.saturating_sub(pad.y))
            })
            .collect();
        let parent_align = self.align;
        for (cell, cell_size) in self.cells.iter_mut().zip(cell_sizes) {
            let cl = cell.widget.get_layout();
            let mode_x = cl.align.get_mode_x().unwrap_or_else(|| {
                parent_align.get_place_x().try_into().unwrap_or(FlexAlign::Start)
            });
            let mode_y = cl.align.get_mode_y().unwrap_or_else(|| {
                parent_align.get_place_y().try_into().unwrap_or(FlexAlign::Start)
            });
            let actual = if mode_x == FlexAlign::Stretch && mode_y == FlexAlign::Stretch {
                cell_size
            } else {
                let measured = flow_child_measure(&*cell.widget, cell_size);
                Vec2::new(
                    if mode_x == FlexAlign::Stretch { cell_size.x } else { measured.x.min(cell_size.x) },
                    if mode_y == FlexAlign::Stretch { cell_size.y } else { measured.y.min(cell_size.y) },
                )
            };
            flow_child(&mut *cell.widget, actual);
        }
        self.total_extent()
    }

    fn layout_measure(&self, allocated: Vec2<u16>) -> Vec2<u16> {
        let mut local = SolveState::default();
        self.solve_tracks_into(allocated, &mut local);
        for cell in self.cells.iter() {
            let cell_size = self.cell_size(&local, cell.row, cell.col, cell.row_span, cell.col_span);
            let pad = self.cell_pad(cell).get_total();
            let child_size = Vec2::new(
                cell_size.x.saturating_sub(pad.x),
                cell_size.y.saturating_sub(pad.y),
            );
            flow_child_measure(&*cell.widget, child_size);
        }
        Vec2::new(
            Self::natural_min_extent_sum(&local.cols, self.col_chrome_total()),
            Self::natural_min_extent_sum(&local.rows, self.row_chrome_total()),
        )
    }

    fn layout_position(&mut self) {
        let base = self.layout.rect.pos;
        let placements: Vec<(Vec2<i32>, Vec2<u16>, Vec2<i32>)> = self
            .cells
            .iter()
            .map(|cell| {
                let origin = self.cell_origin(cell.row, cell.col).map(|v| v as i32);
                let cell_size = self.cell_size(&self.state, cell.row, cell.col, cell.row_span, cell.col_span);
                let pad = self.cell_pad(cell);
                let pad_before = Vec2::new(
                    pad.get_before(Axis2D::X) as i32,
                    pad.get_before(Axis2D::Y) as i32,
                );
                let pad_total = pad.get_total();
                let inner = Vec2::new(
                    cell_size.x.saturating_sub(pad_total.x),
                    cell_size.y.saturating_sub(pad_total.y),
                );
                (origin, inner, pad_before)
            })
            .collect();
        let parent_align = self.align;
        for (cell, (origin, inner_size, pad_before)) in self.cells.iter_mut().zip(placements) {
            let cl = cell.widget.get_layout();
            let mode_x = cl.align.get_mode_x().unwrap_or_else(|| {
                parent_align.get_place_x().try_into().unwrap_or(FlexAlign::Start)
            });
            let mode_y = cl.align.get_mode_y().unwrap_or_else(|| {
                parent_align.get_place_y().try_into().unwrap_or(FlexAlign::Start)
            });
            let actual = cell.widget.get_outer_size();
            let slack = Vec2::new(
                inner_size.x.saturating_sub(actual.x) as i32,
                inner_size.y.saturating_sub(actual.y) as i32,
            );
            let intra = Vec2::new(
                match mode_x {
                    FlexAlign::Stretch | FlexAlign::Start => 0,
                    FlexAlign::Center => slack.x / 2,
                    FlexAlign::End => slack.x,
                },
                match mode_y {
                    FlexAlign::Stretch | FlexAlign::Start => 0,
                    FlexAlign::Center => slack.y / 2,
                    FlexAlign::End => slack.y,
                },
            );
            let margin_before = cl.get_margin_before().map(|v| v as i32);
            cell.widget.set_pos(base + origin + pad_before + intra + margin_before);
            cell.widget.layout_position();
        }
    }

    fn render(&self, mut ctx: crate::render::RenderContext) {
        ctx.set_style(self.layout.style);
        ctx.clear();
        let anchor = ctx.anchor;
        self.render_track_styles(&mut ctx);
        self.render_borders(&mut ctx);
        for cell in self.cells.iter() {
            let cell_origin = self.cell_origin(cell.row, cell.col).map(|v| v as i32);
            let cell_size = self.cell_size(&self.state, cell.row, cell.col, cell.row_span, cell.col_span);
            let intra_offset = cell.widget.get_pos() - anchor - cell_origin;
            let col_style = self.columns.get(cell.col as usize).map(|t| t.style).unwrap_or_default();
            let row_style = self.rows.get(cell.row as usize).map(|t| t.style).unwrap_or_default();
            let combined = col_style.apply(row_style).apply(cell.style);
            ctx.set_style(combined);
            ctx.move_to(cell_origin);
            let mut cell_ctx = ctx.region(cell_size);
            cell_ctx.render_child(&*cell.widget, intra_offset);
        }
    }

    fn get_cursor(
        &self,
        selected: Option<WidgetId>,
    ) -> Option<(CursorShape, Vec2<i32>)> {
        let selected = selected?;
        for cell in self.cells.iter() {
            if let Some(r) = self.layout.get_child_cursor(&*cell.widget, selected) {
                return Some(r);
            }
        }
        None
    }

    fn on_input(&mut self, queue: &mut InputQueue) -> InputResult {
        let Some(event) = queue.peek() else { return InputResult::Rejected; };
        match &event.chord {
            chord!(LeftClick) => {
                let pos = event.cell();
                if let Some((axis, boundary, center)) = self.find_divider_at(pos) {
                    let grab_offset = pos[axis] - center;
                    queue.next();
                    self.drag = Some(DragState { axis, boundary, grab_offset });
                    return InputResult::Handled;
                }
                InputResult::Rejected
            }
            chord!(LeftDrag) => {
                let Some(drag) = self.drag.take() else { return InputResult::Rejected; };
                queue.next();
                let mouse = event.cell();
                if let Some(center) = self.divider_center(drag.axis, drag.boundary) {
                    let delta = mouse[drag.axis] - center - drag.grab_offset;
                    self.apply_drag(drag.axis, drag.boundary, delta);
                }
                self.drag = Some(drag);
                InputResult::Handled
            }
            chord!(LeftRelease) => {
                if self.drag.take().is_some() {
                    queue.next();
                    InputResult::Handled
                } else {
                    InputResult::Rejected
                }
            }
            _ => InputResult::Rejected,
        }
    }

    fn each_child(
        &self,
        f: &mut dyn FnMut(&dyn Widget),
        direction: Sign,
    ) {
        for cell in self.cells.iter().direction(direction) {
            f(&*cell.widget);
        }
    }

    fn each_child_mut(
        &mut self,
        f: &mut dyn FnMut(&mut dyn Widget),
        direction: Sign,
    ) {
        for cell in self.cells.iter_mut().direction(direction) {
            f(&mut *cell.widget);
        }
    }

    fn find_descendant(
        &self,
        predicate: &dyn Fn(&dyn Widget) -> bool,
        mut path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        for cell in self.cells.iter() {
            let grandchild = cell.widget
                .find_descendant(predicate, path.as_mut().map(|p| &mut **p));
            if grandchild.is_some() {
                if let Some(p) = &mut path {
                    p.push(cell.widget.get_id());
                }
                return grandchild;
            }
            if predicate(&*cell.widget) {
                if let Some(p) = &mut path {
                    p.push(cell.widget.get_id());
                }
                return Some(cell.widget.get_id());
            }
        }
        None
    }

    fn descendant_at_pos(
        &self,
        pos: Vec2<f32>,
        mut path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        for cell in self.cells.iter().rev() {
            if !Self::pos_in_rect(&*cell.widget, pos) {
                continue;
            }
            let descendant = cell.widget
                .descendant_at_pos(pos, path.as_mut().map(|p| &mut **p));
            if let Some(p) = &mut path {
                p.push(cell.widget.get_id());
            }
            return Some(descendant.unwrap_or_else(|| cell.widget.get_id()));
        }
        None
    }

    fn find_descendant_at_pos(
        &self,
        pos: Vec2<f32>,
        predicate: &dyn Fn(&dyn Widget) -> bool,
        mut path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        for cell in self.cells.iter().rev() {
            if !Self::pos_in_rect(&*cell.widget, pos) {
                continue;
            }
            let grandchild = cell.widget
                .find_descendant_at_pos(pos, predicate, path.as_mut().map(|p| &mut **p));
            if grandchild.is_some() {
                if let Some(p) = &mut path {
                    p.push(cell.widget.get_id());
                }
                return grandchild;
            }
            if predicate(&*cell.widget) {
                if let Some(p) = &mut path {
                    p.push(cell.widget.get_id());
                }
                return Some(cell.widget.get_id());
            }
        }
        None
    }
}

impl Grid {
    /// Creates an empty grid with no tracks and no children.
    pub fn new() -> Box<Self> {
        Box::new(Self {
            layout: Layout::new(),
            cells: Vec::new(),
            columns: Vec::new(),
            rows: Vec::new(),
            col_gap: 0,
            row_gap: 0,
            align: AlignSpec::default(),
            state: SolveState::default(),
            border: None,
            row_borders: None,
            col_borders: None,
            border_style: Style::new(),
            resizable_cols: false,
            resizable_rows: false,
            col_drag_size: Vec::new(),
            row_drag_size: Vec::new(),
            drag: None,
            padding: Spacing::new(),
        })
    }

    /// Builder form of [`Grid::set_columns`] taking an array.
    pub fn columns<const N: usize>(mut self: Box<Self>, tracks: [Track; N]) -> Box<Self> {
        self.set_columns(tracks.to_vec());
        self
    }

    /// Builder form of [`Grid::set_rows`] taking an array.
    pub fn rows<const N: usize>(mut self: Box<Self>, tracks: [Track; N]) -> Box<Self> {
        self.set_rows(tracks.to_vec());
        self
    }

    /// Replaces the column track template, dropping any cells outside the new range.
    pub fn set_columns(&mut self, tracks: Vec<Track>) {
        let n = tracks.len() as u16;
        self.cells.retain(|c| c.col < n);
        self.columns = tracks;
        self.col_drag_size = vec![None; self.columns.len()];
        self.drag = None;
        self.dirty_layout();
    }

    /// Replaces the row track template, dropping any cells outside the new range.
    pub fn set_rows(&mut self, tracks: Vec<Track>) {
        let n = tracks.len() as u16;
        self.cells.retain(|c| c.row < n);
        self.rows = tracks;
        self.row_drag_size = vec![None; self.rows.len()];
        self.drag = None;
        self.dirty_layout();
    }

    /// Removes all children.
    pub fn clear(&mut self) {
        self.cells.clear();
        self.dirty_layout();
    }

    /// Removes and returns the child with the given `id`, downcast to `T`.
    pub fn remove<T: AnyWidget + ?Sized>(&mut self, id: WidgetId<T>) -> Option<Box<T>> {
        if let Some(idx) = self.cells.iter().position(|c| c.widget.get_id() == id) {
            let cell = self.cells.remove(idx);
            self.dirty_layout();
            T::downcast_box(cell.widget)
        } else {
            None
        }
    }

    /// Removes the cell whose origin is exactly `(row, col)` and returns its widget.
    pub fn remove_at(&mut self, row: u16, col: u16) -> Option<Box<dyn Widget>> {
        let idx = self.cells.iter().position(|c| c.row == row && c.col == col)?;
        let cell = self.cells.remove(idx);
        self.dirty_layout();
        Some(cell.widget)
    }

    /// Inserts a fully configured [`Cell`].
    pub fn add_cell(&mut self, cell: Cell) {
        self.cells.push(cell);
        self.dirty_layout();
    }

    /// Builder form of [`Grid::add_cell`].
    pub fn cell(mut self: Box<Self>, cell: Cell) -> Box<Self> {
        self.add_cell(cell);
        self
    }

    /// Builder form of [`Grid::add_cell`] taking an array.
    pub fn cells<const N: usize>(mut self: Box<Self>, cells: [Cell; N]) -> Box<Self> {
        for cell in cells {
            self.add_cell(cell);
        }
        self
    }

    /// Places `widget` at `(row, col)` with default span and no overrides.
    pub fn add_child(&mut self, row: u16, col: u16, widget: Box<dyn Widget>) {
        self.add_cell(Cell::new(row, col, widget));
    }

    /// Builder form of [`Grid::add_child`].
    pub fn child(mut self: Box<Self>, row: u16, col: u16, widget: Box<dyn Widget>) -> Box<Self> {
        self.add_child(row, col, widget);
        self
    }

    /// Returns the cell whose origin is exactly `(row, col)`.
    pub fn get_cell(&self, row: u16, col: u16) -> Option<&Cell> {
        self.cells.iter().find(|c| c.row == row && c.col == col)
    }

    /// Returns a [`CellMut`] guard for the cell whose origin is exactly `(row, col)`.
    pub fn get_cell_mut(&mut self, row: u16, col: u16) -> Option<CellMut<'_>> {
        let idx = self.cells.iter().position(|c| c.row == row && c.col == col)?;
        Some(CellMut { grid: self, idx })
    }

    crate::layout_field! {
        /// The number of cells between columns.
        col_gap: u8
    }

    crate::layout_field! {
        /// The number of cells between rows.
        row_gap: u8
    }

    crate::layout_field! {
        /// The padding inside every cell around its child.
        padding: Spacing
    }

    /// Builder shortcut that sets top and bottom cell padding to `n`.
    pub fn vertical_padding(mut self: Box<Self>, n: u8) -> Box<Self> {
        self.set_padding(self.get_padding().vertical(n));
        self
    }

    /// Builder shortcut that sets left and right cell padding to `n`.
    pub fn horizontal_padding(mut self: Box<Self>, n: u8) -> Box<Self> {
        self.set_padding(self.get_padding().horizontal(n));
        self
    }

    /// Builder shortcut that sets left cell padding to `n`.
    pub fn padding_left(mut self: Box<Self>, n: u8) -> Box<Self> {
        self.set_padding(self.get_padding().left(n));
        self
    }

    /// Builder shortcut that sets right cell padding to `n`.
    pub fn padding_right(mut self: Box<Self>, n: u8) -> Box<Self> {
        self.set_padding(self.get_padding().right(n));
        self
    }

    /// Builder shortcut that sets top cell padding to `n`.
    pub fn padding_top(mut self: Box<Self>, n: u8) -> Box<Self> {
        self.set_padding(self.get_padding().top(n));
        self
    }

    /// Builder shortcut that sets bottom cell padding to `n`.
    pub fn padding_bottom(mut self: Box<Self>, n: u8) -> Box<Self> {
        self.set_padding(self.get_padding().bottom(n));
        self
    }

    /// Builder form of [`Grid::set_x_place`].
    pub fn x_place(mut self: Box<Self>, p: Place) -> Box<Self> {
        self.set_x_place(p);
        self
    }

    /// Builder form of [`Grid::set_y_place`].
    pub fn y_place(mut self: Box<Self>, p: Place) -> Box<Self> {
        self.set_y_place(p);
        self
    }

    /// Sets how column tracks are distributed along the x axis.
    pub fn set_x_place(&mut self, p: Place) {
        let new_align = self.align.place_x(p);
        if self.align == new_align {
            return;
        }
        self.align = new_align;
        self.dirty_layout();
    }

    /// Sets how row tracks are distributed along the y axis.
    pub fn set_y_place(&mut self, p: Place) {
        let new_align = self.align.place_y(p);
        if self.align == new_align {
            return;
        }
        self.align = new_align;
        self.dirty_layout();
    }

    /// Returns the x-axis distribution for column tracks.
    pub fn get_x_place(&self) -> Place {
        self.align.get_place_x()
    }

    /// Returns the y-axis distribution for row tracks.
    pub fn get_y_place(&self) -> Place {
        self.align.get_place_y()
    }

    crate::layout_field! {
        /// The border around the whole grid.
        border: Option<&'static Border>
    }

    crate::layout_field! {
        /// The border drawn between adjacent rows.
        row_borders: Option<&'static Border>
    }

    crate::layout_field! {
        /// The border drawn between adjacent columns.
        col_borders: Option<&'static Border>
    }

    /// Builder form of [`Grid::set_row_top`] with `Some(b)`.
    pub fn row_top(mut self: Box<Self>, row: u16, b: &'static Border) -> Box<Self> {
        self.set_row_top(row, Some(b));
        self
    }

    /// Builder form of [`Grid::set_row_bottom`] with `Some(b)`.
    pub fn row_bottom(mut self: Box<Self>, row: u16, b: &'static Border) -> Box<Self> {
        self.set_row_bottom(row, Some(b));
        self
    }

    /// Builder form of [`Grid::set_row_top`].
    pub fn row_top_opt(mut self: Box<Self>, row: u16, b: Option<&'static Border>) -> Box<Self> {
        self.set_row_top(row, b);
        self
    }

    /// Builder form of [`Grid::set_row_bottom`].
    pub fn row_bottom_opt(mut self: Box<Self>, row: u16, b: Option<&'static Border>) -> Box<Self> {
        self.set_row_bottom(row, b);
        self
    }

    /// Sets or clears the top-of-row border override for `row`.
    pub fn set_row_top(&mut self, row: u16, b: Option<&'static Border>) {
        if let Some(t) = self.rows.get_mut(row as usize) {
            if t.start == b {
                return;
            }
            t.start = b;
            self.dirty_layout();
        }
    }

    /// Sets or clears the bottom-of-row border override for `row`.
    pub fn set_row_bottom(&mut self, row: u16, b: Option<&'static Border>) {
        if let Some(t) = self.rows.get_mut(row as usize) {
            if t.end == b {
                return;
            }
            t.end = b;
            self.dirty_layout();
        }
    }

    /// Returns the top-of-row border override for `row`.
    pub fn get_row_top(&self, row: u16) -> Option<&'static Border> {
        self.rows.get(row as usize).and_then(|t| t.start)
    }

    /// Returns the bottom-of-row border override for `row`.
    pub fn get_row_bottom(&self, row: u16) -> Option<&'static Border> {
        self.rows.get(row as usize).and_then(|t| t.end)
    }

    /// Builder form of [`Grid::set_col_left`] with `Some(b)`.
    pub fn col_left(mut self: Box<Self>, col: u16, b: &'static Border) -> Box<Self> {
        self.set_col_left(col, Some(b));
        self
    }

    /// Builder form of [`Grid::set_col_right`] with `Some(b)`.
    pub fn col_right(mut self: Box<Self>, col: u16, b: &'static Border) -> Box<Self> {
        self.set_col_right(col, Some(b));
        self
    }

    /// Builder form of [`Grid::set_col_left`].
    pub fn col_left_opt(mut self: Box<Self>, col: u16, b: Option<&'static Border>) -> Box<Self> {
        self.set_col_left(col, b);
        self
    }

    /// Builder form of [`Grid::set_col_right`].
    pub fn col_right_opt(mut self: Box<Self>, col: u16, b: Option<&'static Border>) -> Box<Self> {
        self.set_col_right(col, b);
        self
    }

    /// Sets or clears the left-of-col border override for `col`.
    pub fn set_col_left(&mut self, col: u16, b: Option<&'static Border>) {
        if let Some(t) = self.columns.get_mut(col as usize) {
            if t.start == b {
                return;
            }
            t.start = b;
            self.dirty_layout();
        }
    }

    /// Sets or clears the right-of-col border override for `col`.
    pub fn set_col_right(&mut self, col: u16, b: Option<&'static Border>) {
        if let Some(t) = self.columns.get_mut(col as usize) {
            if t.end == b {
                return;
            }
            t.end = b;
            self.dirty_layout();
        }
    }

    /// Returns the left-of-col border override for `col`.
    pub fn get_col_left(&self, col: u16) -> Option<&'static Border> {
        self.columns.get(col as usize).and_then(|t| t.start)
    }

    /// Returns the right-of-col border override for `col`.
    pub fn get_col_right(&self, col: u16) -> Option<&'static Border> {
        self.columns.get(col as usize).and_then(|t| t.end)
    }

    /// Builder form of [`Grid::set_row_style`].
    pub fn row_style(mut self: Box<Self>, row: u16, s: Style) -> Box<Self> {
        self.set_row_style(row, s);
        self
    }

    /// Builder form of [`Grid::set_col_style`].
    pub fn col_style(mut self: Box<Self>, col: u16, s: Style) -> Box<Self> {
        self.set_col_style(col, s);
        self
    }

    /// Sets the fill style for row `row`'s cells and adjacent gaps.
    pub fn set_row_style(&mut self, row: u16, s: Style) {
        if let Some(t) = self.rows.get_mut(row as usize) {
            if t.style == s {
                return;
            }
            t.style = s;
            self.dirty_paint();
        }
    }

    /// Sets the fill style for column `col`'s cells and adjacent gaps.
    pub fn set_col_style(&mut self, col: u16, s: Style) {
        if let Some(t) = self.columns.get_mut(col as usize) {
            if t.style == s {
                return;
            }
            t.style = s;
            self.dirty_paint();
        }
    }

    /// Returns the fill style for row `row`.
    pub fn get_row_style(&self, row: u16) -> Style {
        self.rows.get(row as usize).map(|t| t.style).unwrap_or_default()
    }

    /// Returns the fill style for column `col`.
    pub fn get_col_style(&self, col: u16) -> Style {
        self.columns.get(col as usize).map(|t| t.style).unwrap_or_default()
    }

    /// Builder form of [`Grid::set_row_border_style`].
    pub fn row_border_style(mut self: Box<Self>, row: u16, s: Style) -> Box<Self> {
        self.set_row_border_style(row, s);
        self
    }

    /// Builder form of [`Grid::set_col_border_style`].
    pub fn col_border_style(mut self: Box<Self>, col: u16, s: Style) -> Box<Self> {
        self.set_col_border_style(col, s);
        self
    }

    /// Sets the border-glyph style override for row `row`.
    pub fn set_row_border_style(&mut self, row: u16, s: Style) {
        if let Some(t) = self.rows.get_mut(row as usize) {
            if t.border_style == s {
                return;
            }
            t.border_style = s;
            self.dirty_paint();
        }
    }

    /// Sets the border-glyph style override for column `col`.
    pub fn set_col_border_style(&mut self, col: u16, s: Style) {
        if let Some(t) = self.columns.get_mut(col as usize) {
            if t.border_style == s {
                return;
            }
            t.border_style = s;
            self.dirty_paint();
        }
    }

    /// Returns the border-glyph style override for row `row`.
    pub fn get_row_border_style(&self, row: u16) -> Style {
        self.rows.get(row as usize).map(|t| t.border_style).unwrap_or_default()
    }

    /// Returns the border-glyph style override for column `col`.
    pub fn get_col_border_style(&self, col: u16) -> Style {
        self.columns.get(col as usize).map(|t| t.border_style).unwrap_or_default()
    }

    crate::style_field! {
        /// The base style applied to all grid border glyphs.
        border_style: Style
    }

    crate::field! {
        /// Whether columns can be resized by dragging their gutters.
        resizable_cols: bool
    }

    crate::field! {
        /// Whether rows can be resized by dragging their gutters.
        resizable_rows: bool
    }

    /// Builder form that enables drag-resize on both axes.
    pub fn resizable(mut self: Box<Self>) -> Box<Self> {
        self.set_resizable_cols(true);
        self.set_resizable_rows(true);
        self
    }

    /// Clears all manual size overrides set by drag-resize, [`Grid::set_col_size`], and [`Grid::set_row_size`].
    pub fn clear_resize(&mut self) {
        self.col_drag_size.fill(None);
        self.row_drag_size.fill(None);
        self.drag = None;
        self.dirty_layout();
    }

    /// Builder form of [`Grid::set_col_size`] with `Some(size)`.
    pub fn col_size(mut self: Box<Self>, col: u16, size: u16) -> Box<Self> {
        self.set_col_size(col, Some(size));
        self
    }

    /// Builder form of [`Grid::set_row_size`] with `Some(size)`.
    pub fn row_size(mut self: Box<Self>, row: u16, size: u16) -> Box<Self> {
        self.set_row_size(row, Some(size));
        self
    }

    /// Pins or unpins column `col` to `size`.
    pub fn set_col_size(&mut self, col: u16, size: Option<u16>) {
        if let Some(slot) = self.col_drag_size.get_mut(col as usize) {
            if *slot == size {
                return;
            }
            *slot = size;
            self.dirty_layout();
        }
    }

    /// Pins or unpins row `row` to `size`.
    pub fn set_row_size(&mut self, row: u16, size: Option<u16>) {
        if let Some(slot) = self.row_drag_size.get_mut(row as usize) {
            if *slot == size {
                return;
            }
            *slot = size;
            self.dirty_layout();
        }
    }

    /// Returns the current size pin for column `col`.
    pub fn get_col_size(&self, col: u16) -> Option<u16> {
        self.col_drag_size.get(col as usize).copied().flatten()
    }

    /// Returns the current size pin for row `row`.
    pub fn get_row_size(&self, row: u16) -> Option<u16> {
        self.row_drag_size.get(row as usize).copied().flatten()
    }
}

