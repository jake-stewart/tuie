//! Virtualized list of widgets driven by a renderer callback.

use crate::prelude::*;
use crate::widget::chrome::ChromeHost;
use crate::widget::align::{AlignSpec, FlexAlign, Place};
use crate::widget::flow_output_size;
use chord_macro::chord;
use sign::Directional;

/// Request from a [`List`] for the given index range to be loaded.
pub struct ListRequestEvent(pub std::ops::Range<usize>);

#[derive(Clone, Copy, Debug)]
struct Anchor {
    index: usize,
    offset: i32,
}

impl Anchor {
    fn zero() -> Self {
        Self {
            index: 0,
            offset: 0,
        }
    }
}

#[derive(Clone, Debug)]
enum ScrollTarget {
    None,
    Index { index: usize, align: Option<Align> },
    Progress(f32),
    Delta(i32),
}

struct ListItem {
    widget: Box<dyn Widget>,
    dirty: bool,
}

struct ScrollConfig {
    main_mode: Scrollbar,
    cross_mode: Option<Scrollbar>,
    cross_scroll_offset: u32,
    cross_content_size: u16,
    scrollbar: Vec2<ScrollbarState>,
    subcell_scroll: Vec2<f32>,
    style: ScrollbarStyle,
}

impl ScrollConfig {
    fn new() -> Self {
        Self {
            main_mode: Scrollbar::Hidden,
            cross_mode: None,
            cross_scroll_offset: 0,
            cross_content_size: 0,
            scrollbar: Vec2::new(ScrollbarState::new(), ScrollbarState::new()),
            subcell_scroll: Vec2::of(0.0),
            style: ScrollbarStyle::new(),
        }
    }
}

struct GapConfig {
    size: u8,
    border: Option<&'static Border>,
    border_style: Style,
}

impl GapConfig {
    fn new() -> Self {
        Self {
            size: 0,
            border: None,
            border_style: Style::new(),
        }
    }
}

/// Virtualized list backed by a render callback.
pub struct List {
    layout: Layout,

    len: usize,
    render_context: Option<Box<dyn std::any::Any>>,
    render_fn: Option<Box<dyn Fn(&mut dyn std::any::Any, usize) -> Option<Box<dyn Widget>>>>,

    items: Vec<ListItem>,
    reuse_buf: Vec<Option<ListItem>>,
    window_start: usize,

    anchor: Anchor,
    scroll_target: ScrollTarget,
    pending_request: Option<std::ops::Range<usize>>,
    pending_progress: Option<f32>,
    avg_height: f64,

    orientation: Axis2D,
    direction: Sign,

    align: AlignSpec,
    gap: GapConfig,
    chrome: Option<Box<Chrome>>,
    scroll: ScrollConfig,
    insets: Spacing,

    first_item_offset: i32,
}

impl ChromeHost for List {
    fn get_chrome(&self) -> Option<&Chrome> {
        self.chrome.as_deref()
    }

    fn get_chrome_mut(&mut self) -> &mut Chrome {
        self.chrome.get_or_insert_with(|| Box::new(Chrome::new()))
    }

    fn get_insets(&self) -> Spacing {
        self.insets
    }
}

impl List {
    const CHUNK: usize = 16;


    fn item_contains(widget: &dyn Widget, pos: Vec2<f32>) -> bool {
        let slot_pos = widget.get_pos();
        let slot_size = widget.get_rect_size().map(|v| v as i32);
        Axis2D::all(|a| {
            pos[a] >= slot_pos[a] as f32 && pos[a] < (slot_pos[a] + slot_size[a]) as f32
        })
    }

    fn scrollbars_both_visible(&self) -> bool {
        let a = self.orientation;
        self.scroll.scrollbar[a].is_visible()
            && self.scroll.scrollbar[a.flip()].is_visible()
    }

    fn get_viewport_size_inner(&self, include_scrollbars: bool) -> Vec2<u16> {
        let content_size = self.layout.rect.size;
        let borders = self.get_border_cells() * 2;
        Axis2D::map(|axis| {
            let mut v = content_size[axis].saturating_sub(borders);
            if include_scrollbars && self.scroll.scrollbar[axis.flip()].is_visible() {
                v = v.saturating_sub(1);
            }
            v
        })
    }

    fn get_viewport_size(&self) -> Vec2<u16> {
        self.get_viewport_size_inner(true)
    }

    fn get_viewport_inner(&self, include_scrollbars: bool) -> Vec2<u16> {
        let size = self.get_viewport_size_inner(include_scrollbars);
        let pad = self.get_padding_total();
        Axis2D::map(|a| size[a].saturating_sub(pad[a]))
    }

    fn get_viewport(&self) -> Vec2<u16> {
        self.get_viewport_inner(true)
    }

    fn get_viewport_without_scrollbars(&self) -> Vec2<u16> {
        self.get_viewport_inner(false)
    }

    fn inner_pos(&self) -> Vec2<i32> {
        self.layout.rect.pos + self.get_chrome_before().map(|v| v as i32)
    }

    fn viewport_contains(&self, pos: Vec2<f32>) -> bool {
        let inner = self.inner_pos();
        let viewport = self.get_viewport();
        Axis2D::all(|a| {
            pos[a] >= inner[a] as f32 && pos[a] < (inner[a] + viewport[a] as i32) as f32
        })
    }

    fn subcell_offset(&self) -> Vec2<f32> {
        if !crate::runtime::is_gui() {
            return Vec2::of(0.0);
        }
        Axis2D::map(|a| -self.scroll.subcell_scroll[a])
    }

    fn main_content_size(&self) -> u32 {
        if self.len == 0 {
            return 0;
        }
        self.content_offset_of(self.len - 1)
            + self.item_height_or_estimate(self.len - 1) as u32
    }

    fn max_cross_scroll(&self) -> u32 {
        let viewport_cross = self.get_viewport()[self.orientation.flip()];
        self.scroll.cross_content_size.saturating_sub(viewport_cross) as u32
    }

    fn data_to_window(&self, data_index: usize) -> Option<usize> {
        if data_index >= self.window_start
            && data_index < self.window_start + self.items.len()
        {
            Some(data_index - self.window_start)
        } else {
            None
        }
    }

    fn avg_height_ceil(&self) -> u16 {
        (self.avg_height.ceil() as u16).max(1)
    }

    fn item_height(&self, wi: usize) -> u16 {
        self.items[wi].widget.get_outer_size()[self.orientation]
    }

    fn item_height_or_estimate(&self, index: usize) -> u16 {
        let wi = index.wrapping_sub(self.window_start);
        if wi < self.items.len() {
            self.item_height(wi)
        } else {
            self.avg_height_ceil()
        }
    }

    fn estimated_step(&self) -> u32 {
        self.avg_height_ceil() as u32 + self.gap.size as u32
    }

    fn screen_pos(&self, anchor_offset: i32, item_extent: i32) -> i32 {
        match self.direction {
            Sign::Positive => anchor_offset,
            Sign::Negative => {
                self.get_viewport()[self.orientation] as i32
                    - anchor_offset
                    - item_extent
            }
        }
    }

    fn screen_to_offset(&self, screen_pos: i32, item_extent: i32) -> i32 {
        match self.direction {
            Sign::Positive => screen_pos,
            Sign::Negative => {
                self.get_viewport()[self.orientation] as i32
                    - screen_pos
                    - item_extent
            }
        }
    }

    fn offset_from_anchor(&self, wi: usize) -> i32 {
        if wi == 0 {
            return self.first_item_offset;
        }
        self.compute_offset_from_anchor(wi)
    }

    fn compute_offset_from_anchor(&self, wi: usize) -> i32 {
        let anchor_wi = self.anchor.index.checked_sub(self.window_start);
        let gap = self.gap.size as i32;

        match anchor_wi {
            Some(anchor_wi) if anchor_wi < self.items.len() => {
                if wi >= anchor_wi {
                    let mut offset = self.anchor.offset;
                    for i in anchor_wi..wi {
                        offset += self.item_height(i) as i32 + gap;
                    }
                    offset
                } else {
                    let mut offset = self.anchor.offset;
                    for i in wi..anchor_wi {
                        offset -= self.item_height(i) as i32 + gap;
                    }
                    offset
                }
            }
            _ => 0,
        }
    }

    fn content_offset_of(&self, index: usize) -> u32 {
        let step = self.estimated_step();
        let window_end = self.window_start + self.items.len();

        if index <= self.window_start || self.items.is_empty() {
            return index as u32 * step;
        }

        let before = self.window_start as u32 * step;
        let in_window_end = index.min(window_end);
        let mut in_window = 0u32;
        for i in 0..(in_window_end - self.window_start) {
            in_window += self.item_height(i) as u32 + self.gap.size as u32;
        }
        let past = if index > window_end {
            (index - window_end) as u32 * step
        } else {
            0
        };
        before + in_window + past
    }

    fn scroll_from_anchor(&self) -> u32 {
        let content_pos = self.content_offset_of(self.anchor.index);
        (content_pos as i32 - self.anchor.offset).max(0) as u32
    }

    fn anchor_from_scroll(&self, scroll: u32) -> Anchor {
        let step = self.estimated_step();
        if step == 0 {
            return Anchor::zero();
        }
        let window_end = self.window_start + self.items.len();
        let before_window = self.window_start as u32 * step;

        if scroll < before_window || self.items.is_empty() {
            let index = ((scroll / step) as usize).min(self.len.saturating_sub(1));
            return Anchor {
                index,
                offset: -((scroll - index as u32 * step) as i32),
            };
        }

        let mut pos = before_window;
        for wi in 0..self.items.len() {
            let h = self.item_height(wi) as u32 + self.gap.size as u32;
            if pos + h > scroll {
                return Anchor {
                    index: self.window_start + wi,
                    offset: -((scroll - pos) as i32),
                };
            }
            pos += h;
        }

        let remaining = scroll - pos;
        let items_past = (remaining / step) as usize;
        let index = (window_end + items_past).min(self.len.saturating_sub(1));
        Anchor {
            index,
            offset: -((remaining - items_past as u32 * step) as i32),
        }
    }

    fn size_widget(viewport: Vec2<u16>, orientation: Axis2D, cross_scroll: bool, self_align: AlignSpec, widget: &mut dyn Widget) {
        let a = orientation;
        let cross = a.flip();
        let mut size = Vec2::of(0);
        size[a] = widget.get_layout().constraints.min_size[a];
        size[cross] = viewport[cross];
        let flowed = flow_child(widget, size);
        if flowed[a] != size[a] {
            size[a] = flowed[a];
            flow_child(widget, size);
        }
        if cross_scroll {
            let content_cross = flow_output_size(widget.get_layout())[cross];
            if content_cross > size[cross] {
                size[cross] = content_cross;
                flow_child(widget, size);
            }
        }
        let resolved_mode = widget.get_layout().align.get_mode(cross).unwrap_or_else(|| {
            self_align.get_place(cross).try_into().unwrap_or(FlexAlign::Start)
        });
        if matches!(resolved_mode, FlexAlign::Start | FlexAlign::Center | FlexAlign::End) {
            let min_cross = widget.get_layout().constraints.min_size[cross];
            let max_cross = widget.get_layout().constraints.max_size[cross];
            let target = if max_cross < u16::MAX {
                max_cross.min(size[cross]).max(min_cross)
            } else {
                min_cross
            };
            if target < size[cross] {
                size[cross] = target;
                flow_child(widget, size);
            }
        }
    }

    #[track_caller]
    fn init_widget(&mut self, index: usize) -> Option<Box<dyn Widget>> {
        let render_fn = self.render_fn.as_ref()?;
        let ctx = self.render_context.as_mut()?;
        let mut widget = render_fn(ctx.as_mut(), index)?;
        constrain_child(&mut *widget);
        Self::size_widget(self.get_viewport(), self.orientation, self.scroll.cross_mode.is_some(), self.align, &mut *widget);
        Some(widget)
    }

    fn clamp_cross_scroll(&mut self) {
        self.scroll.cross_scroll_offset = self.scroll.cross_scroll_offset.min(self.max_cross_scroll());
    }

    fn apply_cross_scroll(&mut self, new_scroll: u32) -> bool {
        if self.scroll.cross_mode.is_none() {
            return false;
        }
        let current = self.scroll.cross_scroll_offset;
        let clamped = new_scroll.min(self.max_cross_scroll());
        if clamped == current {
            return false;
        }
        self.scroll.cross_scroll_offset = clamped;
        self.reposition_items();
        self.sync_scrollbars();
        self.dirty_paint();
        self.flush_events(true);
        true
    }

    fn reposition_items(&mut self) {
        let a = self.orientation;
        let cross = a.flip();
        let content_pos = self.inner_pos();
        let gap = self.gap.size as i32;
        let cross_offset = self.scroll.cross_scroll_offset as i32;
        let viewport_cross = self.get_viewport()[cross] as i32;
        let cross_mode: FlexAlign = self.align.get_place(cross).try_into().unwrap_or(FlexAlign::Start);
        let mut offset = self.offset_from_anchor(0);

        for i in 0..self.items.len() {
            let extent = self.item_height(i) as i32;
            let mut child_pos = content_pos;
            child_pos[a] += self.screen_pos(offset, extent);
            child_pos[cross] -= cross_offset;
            let slack = (viewport_cross - self.items[i].widget.get_outer_size()[cross] as i32).max(0);
            let child_mode = self.items[i].widget.get_layout().align.get_mode(cross).unwrap_or(cross_mode);
            match child_mode {
                FlexAlign::Start | FlexAlign::Stretch => {}
                FlexAlign::Center => child_pos[cross] += slack / 2,
                FlexAlign::End => child_pos[cross] += slack,
            }
            let margin_before = self.items[i].widget.get_layout().get_margin_before().map(|v| v as i32);
            self.items[i].widget.set_pos(child_pos + margin_before);
            self.items[i].widget.layout_position();
            offset += extent + gap;
        }
    }

    fn update_scrollbar_visibility(&mut self) {
        let a = self.orientation;
        let main_mode = self.scroll.main_mode;
        let cross_mode = self.scroll.cross_mode;
        let main_total = self.main_content_size();
        let cross_total = self.scroll.cross_content_size as u32;

        let resolve_main = |vp: u32| -> bool {
            match main_mode {
                Scrollbar::Hidden => false,
                Scrollbar::Visible => true,
                Scrollbar::AutoHide => main_total > vp,
            }
        };
        let resolve_cross = |vp: u32| -> bool {
            match cross_mode {
                None | Some(Scrollbar::Hidden) => false,
                Some(Scrollbar::Visible) => true,
                Some(Scrollbar::AutoHide) => cross_total > vp,
            }
        };

        let needs_settling = main_mode == Scrollbar::AutoHide
            || cross_mode == Some(Scrollbar::AutoHide);

        let (main_vis, cross_vis) = if !needs_settling {
            (resolve_main(0), resolve_cross(0))
        } else {
            let vp_naked = self.get_viewport_without_scrollbars();
            let main_p0 = resolve_main(vp_naked[a] as u32);
            let cross_p0 = resolve_cross(vp_naked[a.flip()] as u32);
            if !main_p0 && !cross_p0 {
                (false, false)
            } else {
                self.scroll.scrollbar[a].set_visible(main_p0);
                self.scroll.scrollbar[a.flip()].set_visible(cross_p0);
                let vp = self.get_viewport();
                let main_p1 = resolve_main(vp[a] as u32);
                let cross_p1 = resolve_cross(vp[a.flip()] as u32);
                self.scroll.scrollbar[a].set_visible(main_p1);
                self.scroll.scrollbar[a.flip()].set_visible(cross_p1);
                let vp = self.get_viewport();
                let main_p2 = resolve_main(vp[a] as u32);
                let cross_p2 = resolve_cross(vp[a.flip()] as u32);
                (main_p2, cross_p2)
            }
        };
        self.scroll.scrollbar[a].set_visible(main_vis);
        self.scroll.scrollbar[a.flip()].set_visible(cross_vis);
    }

    fn size_items(&mut self) {
        let a = self.orientation;
        let viewport = self.get_viewport();
        let cross = self.scroll.cross_mode.is_some();
        let self_align = self.align;
        for item in self.items.iter_mut() {
            Self::size_widget(viewport, a, cross, self_align, &mut *item.widget);
        }
        self.recompute_avg_height();
        self.scroll.cross_content_size = self.scroll.cross_content_size.max(
            self.items.iter().map(|item| flow_output_size(item.widget.get_layout())[a.flip()]).max().unwrap_or(0)
        );
    }

    fn recompute_avg_height(&mut self) -> bool {
        if self.items.is_empty() {
            return false;
        }
        let a = self.orientation;
        let old_avg = self.avg_height;
        let sum: u32 = self.items.iter().map(|w| w.widget.get_outer_size()[a] as u32).sum();
        self.avg_height = sum as f64 / self.items.len() as f64;
        self.avg_height != old_avg
    }

    fn sync_scrollbars(&mut self) {
        let a = self.orientation;
        let main_progress = self.get_scroll_progress(a);
        let main_progress = if self.direction.is_negative() {
            1.0 - main_progress
        } else {
            main_progress
        };
        let main_ratio = self.get_scroll_ratio(a);
        let cross_progress = self.get_scroll_progress(a.flip());
        let cross_ratio = self.get_scroll_ratio(a.flip());
        self.scroll.scrollbar[a].set_progress(main_progress);
        self.scroll.scrollbar[a].set_ratio(main_ratio);
        self.scroll.scrollbar[a.flip()].set_progress(cross_progress);
        self.scroll.scrollbar[a.flip()].set_ratio(cross_ratio);
    }

    fn scrollbar_axis_size(&self, axis: Axis2D) -> f32 {
        let vp_size = self.get_viewport_size();
        let a = self.orientation;
        if axis == a {
            let corner_extra = if self.scrollbars_both_visible() {
                0.5
            } else {
                0.0
            };
            vp_size[a] as f32 + corner_extra
        } else {
            vp_size[a.flip()] as f32
        }
    }

    fn apply_scrollbar_progress(&mut self, axis: Axis2D, progress: f32) {
        if axis == self.orientation && self.direction.is_negative() {
            self.set_scroll_progress(axis, 1.0 - progress);
        } else {
            self.set_scroll_progress(axis, progress);
        }
        self.sync_scrollbars();
    }

    fn resolve_scroll_target(&mut self, viewport: i32) {
        let target =
            std::mem::replace(&mut self.scroll_target, ScrollTarget::None);
        match target {
            ScrollTarget::None => {}
            ScrollTarget::Delta(delta) => {
                self.anchor.offset -= delta;
                self.normalize_anchor(viewport);
            }
            ScrollTarget::Index { index, align } => {
                let index = index.min(self.len.saturating_sub(1));
                match align {
                    Some(Align::Start) => {
                        self.anchor = Anchor { index, offset: 0 };
                    }
                    Some(Align::Center) => {
                        self.anchor = Anchor {
                            index,
                            offset: viewport / 2,
                        };
                    }
                    Some(Align::End) => {
                        self.anchor = Anchor {
                            index,
                            offset: viewport - self.avg_height_ceil() as i32,
                        };
                    }
                    None => {
                        if !self.is_index_visible(index, viewport) {
                            self.anchor = Anchor { index, offset: 0 };
                        }
                    }
                }
            }
            ScrollTarget::Progress(progress) => {
                let progress = progress.clamp(0.0, 1.0);
                let max_scroll = self.main_content_size()
                    .saturating_sub(self.get_viewport()[self.orientation] as u32);
                self.anchor = self.anchor_from_scroll((max_scroll as f64 * progress as f64) as u32);
            }
        }
    }

    fn is_index_visible(&self, index: usize, viewport: i32) -> bool {
        let Some(wi) = self.data_to_window(index) else {
            return false;
        };
        if self.data_to_window(self.anchor.index).is_none() {
            return false;
        }
        let offset = self.offset_from_anchor(wi);
        let height = self.item_height(wi) as i32;
        offset + height > 0 && offset < viewport
    }

    fn normalize_anchor(&mut self, viewport: i32) {
        let gap = self.gap.size as i32;
        let Some(anchor_wi) = self.data_to_window(self.anchor.index) else {
            return;
        };
        let anchor_height = self.item_height(anchor_wi) as i32;
        if self.anchor.offset + anchor_height <= 0 {
            let next = self.anchor.index + 1;
            if next < self.len {
                self.anchor = Anchor {
                    index: next,
                    offset: self.anchor.offset + anchor_height + gap,
                };
            }
        } else if self.anchor.offset >= viewport {
            if self.anchor.index > 0 {
                let prev = self.anchor.index - 1;
                if let Some(prev_wi) = self.data_to_window(prev) {
                    let prev_height = self.item_height(prev_wi) as i32;
                    self.anchor = Anchor {
                        index: prev,
                        offset: self.anchor.offset - prev_height - gap,
                    };
                }
            }
        }
    }

    fn clamp_anchor(&mut self, viewport: i32) {
        if self.anchor.index == 0 && self.anchor.offset > 0 {
            self.anchor.offset = 0;
        } else if self.anchor.index > 0 && self.window_start == 0 && self.anchor.index < self.items.len() {
            let top_offset = self.compute_offset_from_anchor(0);
            if top_offset > 0 {
                self.anchor.offset -= top_offset;
            }
        }

        if self.window_start + self.items.len() >= self.len && !self.items.is_empty() {
            let last_wi = self.items.len() - 1;
            let content_end = self.compute_offset_from_anchor(last_wi) + self.item_height(last_wi) as i32;
            if content_end < viewport {
                self.anchor.offset += viewport - content_end;
                if self.window_start == 0 {
                    let first_offset = self.compute_offset_from_anchor(0);
                    if first_offset > 0 {
                        self.anchor.offset -= first_offset;
                    }
                }
            }
        }
    }

    fn can_scroll_forward(&self) -> bool {
        if self.items.is_empty() {
            return self.len > 0;
        }
        let viewport = self.get_viewport()[self.orientation] as i32;
        if self.window_start + self.items.len() < self.len {
            return true;
        }
        let last_wi = self.items.len() - 1;
        self.offset_from_anchor(last_wi) + self.item_height(last_wi) as i32 > viewport
    }

    fn can_scroll_backward(&self) -> bool {
        if self.items.is_empty() {
            return false;
        }
        if self.window_start > 0 {
            return true;
        }
        self.offset_from_anchor(0) < 0
    }

    fn compute_render_range(&self, anchor_idx: usize, viewport: i32) -> std::ops::Range<usize> {
        let margin = viewport as usize * 3;
        let render_start = (anchor_idx.saturating_sub(margin) / Self::CHUNK) * Self::CHUNK;
        let render_end = ((anchor_idx + margin + 1 + Self::CHUNK - 1) / Self::CHUNK * Self::CHUNK).min(self.len);

        let old_start = self.window_start;
        let old_end = old_start + self.items.len();
        let reuse = !self.items.is_empty()
            && render_start >= old_start
            && render_end <= old_end
            && (render_start - old_start) <= Self::CHUNK * 4
            && (old_end - render_end) <= Self::CHUNK * 4;

        if reuse {
            old_start..old_end.min(self.len)
        } else {
            render_start..render_end.min(self.len)
        }
    }

    fn fill(&mut self) -> bool {
        if self.len == 0 {
            self.items.clear();
            self.window_start = 0;
            self.anchor = Anchor::zero();
            self.scroll_target = ScrollTarget::None;
            self.first_item_offset = 0;
            return true;
        }

        let viewport = self.get_viewport()[self.orientation] as i32;
        let saved_anchor = self.anchor;
        let saved_target = self.scroll_target.clone();

        self.resolve_scroll_target(viewport);

        let anchor_idx = self.anchor.index.min(self.len - 1);
        let anchor_offset = self.anchor.offset;
        let std::ops::Range { start: render_start, end: render_end } =
            self.compute_render_range(anchor_idx, viewport);

        let old_start = self.window_start;
        self.reuse_buf.clear();
        self.reuse_buf.extend(self.items.drain(..).map(Some));

        self.items.reserve(render_end - render_start);
        let mut need_request = false;

        for i in render_start..render_end {
            let old_slot = i.wrapping_sub(old_start);
            let reusable = old_slot < self.reuse_buf.len()
                && !self.reuse_buf[old_slot]
                    .as_ref()
                    .map_or(true, |item| item.dirty);

            let item = if reusable {
                self.reuse_buf[old_slot].take().unwrap()
            } else {
                match self.init_widget(i) {
                    Some(w) => ListItem {
                        widget: w,
                        dirty: false,
                    },
                    None => {
                        need_request = true;
                        break;
                    }
                }
            };
            self.items.push(item);
        }

        if need_request {
            let loaded_end = render_start + self.items.len();
            if anchor_idx < render_start || anchor_idx >= loaded_end {
                if let ScrollTarget::Progress(p) = &saved_target {
                    self.pending_progress = Some(*p);
                }
                self.items.clear();
                for slot in self.reuse_buf.drain(..) {
                    if let Some(item) = slot {
                        self.items.push(item);
                    } else {
                        break;
                    }
                }
                self.reuse_buf.clear();
                self.anchor = saved_anchor;
                self.scroll_target = saved_target;
                self.window_start = old_start;
                self.pending_request = Some(render_start..render_end);
                return false;
            }
            self.pending_request = Some(render_start..render_end);
        }

        self.reuse_buf.clear();

        self.pending_progress = None;
        self.window_start = render_start;
        self.anchor = Anchor {
            index: anchor_idx,
            offset: anchor_offset,
        };

        self.clamp_anchor(viewport);

        if need_request && !self.items.is_empty() {
            let last_wi = self.items.len() - 1;
            let content_end =
                self.offset_from_anchor(last_wi)
                    + self.item_height(last_wi) as i32;
            if content_end < viewport {
                self.anchor.offset += viewport - content_end;
                if self.window_start == 0 && self.anchor.offset > 0 {
                    self.anchor.offset = 0;
                }
            }
        }

        self.first_item_offset = self.compute_offset_from_anchor(0);
        true
    }

    fn flush_events(&mut self, scrolled: bool) {
        if scrolled {
            tuie::emit(self.get_id(), ScrollEvent);
        }
        if let Some(range) = self.pending_request.take() {
            tuie::emit(self.get_id(), ListRequestEvent(range));
        }
    }

    fn render_items(&self, ctx: &mut crate::render::RenderContext, slot_origin: Vec2<i32>) {
        let a = self.orientation;
        let d = self.direction;
        let viewport = self.get_viewport();
        let vp_main = viewport[a] as i32;
        let vp_cross = viewport[a.flip()] as i32;
        let slack_main = slot_origin[a];
        let slack_cross = slot_origin[a.flip()];
        let main_min = -slack_main;
        let main_max = vp_main + slack_main;
        let cross_min = -slack_cross;
        let cross_max = vp_cross + slack_cross;
        let gap = self.gap.size as i32;
        let content_pos_cross = self.inner_pos()[a.flip()];
        let mut anchor_offset = self.offset_from_anchor(0);

        for (i, item) in self.items.iter().enumerate() {
            let extent = item.widget.get_outer_size()[a] as i32;
            let pos_main = self.screen_pos(anchor_offset, extent);

            if (d.is_positive() && pos_main >= main_max)
                || (d.is_negative() && pos_main + extent + self.gap.size as i32 <= main_min)
            {
                break;
            }

            if pos_main + extent > main_min && pos_main < main_max {
                let cross_pos = item.widget.get_pos()[a.flip()] - content_pos_cross;

                if cross_pos + item.widget.get_rect_size()[a.flip()] as i32 > cross_min
                    && cross_pos < cross_max
                {
                    let margin_main = item.widget.get_layout().get_margin_before()[a] as i32;
                    let mut slot_pos = slot_origin;
                    slot_pos[a] += pos_main + margin_main;
                    slot_pos[a.flip()] += cross_pos;
                    ctx.render_child(&*item.widget, slot_pos);
                }
            }

            let is_edge = self.window_start + i == d.bound(self.len).saturating_sub(1);
            if let Some(border) = self.gap.border.filter(|_| !is_edge) {
                let border_pos = pos_main + extent + gap / 2;
                if border_pos >= main_min && border_pos < main_max {
                    let mut border_offset = slot_origin;
                    border_offset[a] += border_pos;
                    ctx.move_to(border_offset);
                    let mut size = viewport;
                    size[a] = 1;
                    ctx.set_style(self.gap.border_style);
                    ctx.region(size).fill(&border.get_edge(a).to_string());
                    ctx.set_style(self.layout.style);
                }
            }

            anchor_offset += extent + gap;
        }
    }
}

impl Widget for List {
    fn get_layout(&self) -> &Layout {
        &self.layout
    }

    fn get_layout_mut(&mut self) -> &mut Layout {
        &mut self.layout
    }

    fn get_name(&self) -> &'static str {
        "List"
    }

    fn get_scroll_clip_range(&self) -> Vec2<Option<(i32, i32)>> {
        let vp_pos = self.layout.rect.pos + self.get_border_offset();
        let vp_size = self.get_viewport_size();
        Axis2D::map(|a| {
            Some((vp_pos[a], vp_pos[a] + vp_size[a] as i32))
        })
    }

    fn measure_constraints(&mut self) -> Constraints {
        self.each_child_mut(&mut constrain_child, Sign::Positive);
        let cross = self.orientation.flip();
        let border = self.get_border_cells() * 2;
        let pad = self.get_padding_total()[cross];
        let gutter = match self.scroll.main_mode {
            Scrollbar::Visible | Scrollbar::AutoHide => 1,
            Scrollbar::Hidden => 0,
        };
        let items_min_cross = self.items.iter()
            .map(|item| item.widget.get_layout().constraints.min_size[cross])
            .max()
            .unwrap_or(0);
        let mut min_size = Vec2::of(0u16);
        min_size[cross] = items_min_cross
            .saturating_add(border)
            .saturating_add(pad)
            .saturating_add(gutter);
        Constraints {
            min_size,
            max_size: Vec2::of(u16::MAX),
            preferred_size: Vec2::of(0),
        }
    }

    fn layout_measure(&self, _allocated: Vec2<u16>) -> Vec2<u16> {
        Vec2::of(0)
    }

    fn layout_flow(&mut self, allocated: Vec2<u16>) -> Vec2<u16> {
        let prev_size = self.layout.rect.size;
        self.layout.rect.size = allocated;
        let had_scroll_target = !matches!(self.scroll_target, ScrollTarget::None);

        self.update_scrollbar_visibility();
        let filled = self.fill();
        self.size_items();
        let prev_vis = Axis2D::map(|a| self.scroll.scrollbar[a].is_visible());
        self.update_scrollbar_visibility();
        let new_vis = Axis2D::map(|a| self.scroll.scrollbar[a].is_visible());
        if prev_vis != new_vis {
            self.size_items();
        }

        self.clamp_cross_scroll();
        if self.scroll.cross_scroll_offset == self.max_cross_scroll() {
            self.scroll.subcell_scroll[self.orientation.flip()] = 0.0;
        }
        self.sync_scrollbars();
        self.flush_events(had_scroll_target && filled);
        self.layout.rect.size = prev_size;
        Vec2::of(0)
    }

    fn layout_position(&mut self) {
        self.reposition_items();
    }

    fn after_layout(&mut self) {
        if self.recompute_avg_height() {
            self.dirty_paint();
        }
        self.sync_scrollbars();
        self.flush_events(false);
    }

    fn before_focus_move(
        &mut self,
        selected_child: WidgetId,
        _axis: Option<Axis2D>,
        direction: Sign,
    ) {
        if direction != self.direction {
            return;
        }
        let Some(window_idx) =
            self.items.iter().position(|item| item.widget.get_id() == selected_child)
        else {
            return;
        };

        for item in &self.items[window_idx + 1..] {
            if item.widget.is_focusable() {
                return;
            }
        }

        let mut i = self.window_start + self.items.len();
        while i < self.len {
            match self.init_widget(i) {
                Some(widget) => {
                    let selectable = widget.is_focusable();
                    self.items.push(ListItem { widget, dirty: false });
                    i += 1;
                    if selectable {
                        break;
                    }
                }
                None => {
                    break;
                }
            }
        }
        self.reposition_items();
    }

    fn render(&self, mut ctx: crate::render::RenderContext) {
        let a = self.orientation;
        let viewport = self.get_viewport();
        let vp_size = self.get_viewport_size();
        let border = self.get_border_offset();
        let chrome_before = self.get_chrome_before().map(|v| v as i32);

        ctx.set_style(self.layout.style);
        ctx.clear();

        if let Some(chrome) = self.get_chrome() {
            chrome.render(&mut ctx);
        }
        ctx.set_style(self.layout.style);

        let fract = self.scroll.subcell_scroll;
        let has_subcell = crate::runtime::is_gui() && fract != Vec2::of(0.0);

        ctx.move_to(chrome_before);

        if has_subcell {
            #[cfg(feature = "gui")]
            {
                let cell_px = crate::runtime::get_terminal_info().cell_size.unwrap_or(Vec2::of(1));
                let subcell = Axis2D::map(|axis| -((fract[axis] * cell_px[axis] as f32).round() as i32));
                let mut content_size = viewport;
                let mut content_offset = Vec2::of(0i32);
                for axis in [Axis2D::X, Axis2D::Y] {
                    if subcell[axis] != 0 {
                        content_size[axis] = content_size[axis].saturating_add(2);
                        content_offset[axis] = -1;
                    }
                }
                let slot_origin = Axis2D::map(|axis| -content_offset[axis]);
                ctx.queue_offset_region(
                    self,
                    viewport,
                    content_size,
                    content_offset,
                    subcell,
                    move |this: &Self, mut child_ctx| {
                        this.render_items(&mut child_ctx, slot_origin);
                    },
                );
            }
        } else {
            let mut vp_ctx = ctx.viewport(viewport);
            self.render_items(&mut vp_ctx, Vec2::of(0i32));
            drop(vp_ctx);
        }
        let thumb = self.scroll.style.get_resolved_thumb();
        let (extend, _) = thumb.corner_extension(self.scrollbars_both_visible());
        for axis in [a, a.flip()] {
            if !self.scroll.scrollbar[axis].is_visible() {
                continue;
            }
            let cross = axis.flip();
            let inset_before = self.get_inset_before(axis);
            let inset_after = self.get_inset_after(axis);
            let corner_extra: u16 = if extend[axis] { 1 } else { 0 };
            let total = vp_size[axis] + corner_extra;
            if inset_before + inset_after >= total {
                continue;
            }
            let mut pos = border;
            pos[cross] += vp_size[cross] as i32;
            pos[axis] += inset_before as i32;
            ctx.move_to(pos);
            let mut bar_size = Vec2::of(0u16);
            bar_size[axis] = total - inset_before - inset_after;
            bar_size[cross] = 1;
            let corner_extra_size = if extend[axis] { 0.5 } else { 0.0 };
            let size = vp_size[axis] as f32
                + corner_extra_size
                - inset_before as f32
                - inset_after as f32;

            self.scroll.scrollbar[axis].render_smooth(
                &mut ctx,
                self,
                axis,
                bar_size,
                size,
                &self.scroll.style,
                move |this: &Self| Some((&this.scroll.style, &this.scroll.scrollbar[axis])),
            );
        }
    }

    fn on_input(&mut self, queue: &mut InputQueue) -> InputResult {
        let Some(event) = queue.peek() else {
            return InputResult::Rejected;
        };
        match &event.chord {
            chord!(LeftClick) => {
                let vp_size = self.get_viewport_size();
                let border = self.get_border_offset();
                let local = event.pos - border.map(|v| v as f32);
                let local_cell = event.cell() - border;
                let a = self.orientation;

                let has_both = self.scrollbars_both_visible();
                for axis in [a, a.flip()] {
                    if !self.scroll.scrollbar[axis].is_visible() {
                        continue;
                    }
                    let cross = axis.flip();
                    let inset_before = self.get_inset_before(axis) as i32;
                    let inset_after = self.get_inset_after(axis) as i32;
                    let corner_extra = if has_both && axis == a {
                        1
                    } else {
                        0
                    };
                    let along_limit = vp_size[axis] as i32 + corner_extra - inset_after;
                    let in_gutter = local_cell[cross] >= vp_size[cross] as i32
                        && local_cell[axis] >= inset_before
                        && local_cell[axis] < along_limit;
                    if !in_gutter {
                        continue;
                    }
                    let size = self.scrollbar_axis_size(axis) - inset_before as f32 - inset_after as f32;
                    let result = self.scroll.scrollbar[axis].handle_input(
                        &event.chord,
                        local[axis] - inset_before as f32,
                        size,
                    );
                    if let ScrollbarInputResult::Handled(progress) = result {
                        queue.next();
                        if let Some(p) = progress {
                            self.apply_scrollbar_progress(axis, p);
                        }
                        return InputResult::Handled;
                    }
                }
                InputResult::Rejected
            }
            chord!(LeftDrag) | chord!(LeftRelease) => {
                let a = self.orientation;
                let border = self.get_border_offset();
                let local = event.pos - border.map(|v| v as f32);

                for axis in [a, a.flip()] {
                    if !self.scroll.scrollbar[axis].is_dragging() {
                        continue;
                    }
                    let inset_before = self.get_inset_before(axis) as i32;
                    let inset_after = self.get_inset_after(axis) as i32;
                    let size = self.scrollbar_axis_size(axis) - inset_before as f32 - inset_after as f32;
                    if !matches!(&event.chord, chord!(LeftRelease)) && size <= 0.0 {
                        continue;
                    }
                    let result = self.scroll.scrollbar[axis].handle_input(
                        &event.chord,
                        local[axis] - inset_before as f32,
                        size,
                    );
                    if let ScrollbarInputResult::Handled(progress) = result {
                        queue.next();
                        if let Some(p) = progress {
                            self.apply_scrollbar_progress(axis, p);
                        }
                        return InputResult::Handled;
                    }
                }
                InputResult::Rejected
            }
            chord!(MouseSmoothScroll(direction, delta)) => {
                let direction = *direction;
                let delta = *delta;
                let a = direction.axis();
                if a != self.orientation && self.scroll.cross_mode.is_none() {
                    return InputResult::Rejected;
                }
                if delta == 0.0 {
                    return InputResult::Rejected;
                }
                queue.next();

                if a == self.orientation {
                    let s = direction.screen_sign().relative_to(self.direction);
                    let signed = delta as f64 * s.delta() as f64;
                    let max = self.main_content_size()
                        .saturating_sub(self.get_viewport()[a] as u32) as f64;
                    let cur_total = self.scroll_from_anchor() as f64
                        + self.scroll.subcell_scroll[a] as f64;
                    let new_total = (cur_total + signed).clamp(0.0, max);
                    let new_cells = new_total as u32;
                    let new_sub = (new_total - new_cells as f64) as f32;
                    if new_total == cur_total {
                        return InputResult::Handled;
                    }
                    self.anchor = self.anchor_from_scroll(new_cells);
                    self.scroll.subcell_scroll[a] = new_sub;
                    self.fill();
                    self.sync_scrollbars();
                    self.dirty_paint();
                    self.flush_events(true);
                    return InputResult::Handled;
                }

                let max_total = self.max_cross_scroll() as f64;
                let cur_total = self.scroll.cross_scroll_offset as f64
                    + self.scroll.subcell_scroll[a] as f64;
                let new_total = if direction.screen_sign() == Sign::Positive {
                    (cur_total + delta as f64).min(max_total)
                } else {
                    (cur_total - delta as f64).max(0.0)
                };
                let new_cross = new_total as u32;
                let new_sub = (new_total - new_cross as f64) as f32;
                let changed = new_cross != self.scroll.cross_scroll_offset
                    || new_sub != self.scroll.subcell_scroll[a];
                self.scroll.cross_scroll_offset = new_cross;
                self.scroll.subcell_scroll[a] = new_sub;
                if changed {
                    self.reposition_items();
                    self.sync_scrollbars();
                    self.dirty_paint();
                    self.flush_events(true);
                }
                InputResult::Handled
            }
            chord!(MouseScroll(direction)) => {
                queue.next();
                if direction.axis() != self.orientation {
                    if self.scroll.cross_mode.is_some() {
                        let scroll = self.scroll.cross_scroll_offset;
                        self.apply_cross_scroll((scroll as i32 + direction.screen_sign().delta()).max(0) as u32);
                        return InputResult::Handled;
                    }
                    return InputResult::Rejected;
                }
                if self.can_scroll(*direction) {
                    self.scroll_by(direction.screen_sign().relative_to(self.direction).delta());
                }
                InputResult::Handled
            }
            _ => InputResult::Rejected,
        }
    }

    fn can_scroll(&self, direction: Direction2D) -> bool {
        let a = direction.axis();
        if self.orientation != a {
            if self.scroll.cross_mode.is_some() {
                let scroll = self.scroll.cross_scroll_offset;
                let max = self.max_cross_scroll();
                if direction.screen_sign().is_positive() {
                    scroll < max
                } else {
                    scroll > 0 || self.scroll.subcell_scroll[a] > 0.0
                }
            } else {
                false
            }
        } else {
            let scroll_sign = direction.screen_sign().relative_to(self.direction);
            if scroll_sign.is_positive() {
                self.can_scroll_forward()
            } else {
                self.can_scroll_backward() || self.scroll.subcell_scroll[a] > 0.0
            }
        }
    }

    fn find_descendant(
        &self,
        predicate: &dyn Fn(&dyn Widget) -> bool,
        mut path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        for item in &self.items {
            let grandchild = item.widget
                .find_descendant(predicate, path.as_mut().map(|p| &mut **p));
            if grandchild.is_some() {
                if let Some(p) = &mut path {
                    p.push(item.widget.get_id());
                }
                return grandchild;
            }
            if predicate(&*item.widget) {
                if let Some(p) = &mut path {
                    p.push(item.widget.get_id());
                }
                return Some(item.widget.get_id());
            }
        }
        None
    }

    fn each_child(
        &self,
        f: &mut dyn FnMut(&dyn Widget),
        direction: Sign,
    ) {
        for item in self.items.iter().direction(direction) {
            f(&*item.widget);
        }
    }

    fn each_child_mut(
        &mut self,
        f: &mut dyn FnMut(&mut dyn Widget),
        direction: Sign,
    ) {
        for item in self.items.iter_mut().direction(direction) {
            f(&mut *item.widget);
        }
    }

    fn reveal(
        &mut self,
        child: Option<WidgetId>,
        revelation: &mut Revelation,
        scroll_align: Vec2<Option<Align>>,
    ) {
        let a = self.orientation;
        let cross = a.flip();

        if let Some(child_id) = child {
            if let Some(wi) = self.items.iter().position(|item| item.widget.get_id() == child_id) {
                let data_index = self.window_start + wi;
                let offset = self.offset_from_anchor(wi);
                self.anchor = Anchor {
                    index: data_index,
                    offset,
                };
            }
        }

        let viewport = self.get_viewport();
        let vp_main = viewport[a] as i32;

        let main_delta = crate::widget::resolve_revelation_axis(
            revelation.get_rects().iter()
                .map(|r| (self.screen_to_offset(r.pos[a], r.size[a] as i32), r.size[a] as i32)),
            vp_main,
            scroll_align[a],
            0,
        );
        self.anchor.offset -= main_delta;
        self.fill();
        let mut scrolled = main_delta != 0;

        let old_cross_scroll = self.scroll.cross_scroll_offset;
        if self.scroll.cross_mode.is_some() {
            let vp_cross = viewport[cross] as i32;
            let cur = self.scroll.cross_scroll_offset as i32;
            let cross_d = crate::widget::resolve_revelation_axis(
                revelation.get_rects().iter()
                    .map(|r| (r.pos[cross] + cur, r.size[cross] as i32)),
                vp_cross,
                scroll_align[cross],
                0,
            );
            let target = cur + cross_d;
            let clamped = (target.max(0) as u32).min(self.max_cross_scroll());
            if clamped != self.scroll.cross_scroll_offset {
                self.scroll.cross_scroll_offset = clamped;
                self.reposition_items();
                scrolled = true;
            }
        }

        if scrolled {
            self.sync_scrollbars();
            self.dirty_paint();
        }

        let main_screen_delta = match self.direction {
            Sign::Positive => main_delta,
            Sign::Negative => -main_delta,
        };
        let cross_applied = old_cross_scroll as i32 - self.scroll.cross_scroll_offset as i32;
        let mut delta = Vec2::of(0i32);
        delta[a] = -main_screen_delta;
        delta[cross] = cross_applied;
        revelation.translate(delta);

        revelation.clip_axis(a, 0, vp_main);
        let vp_cross = viewport[cross] as i32;
        revelation.clip_axis(cross, 0, vp_cross);

        self.flush_events(scrolled);
    }

    fn descendant_at_pos(
        &self,
        pos: Vec2<f32>,
        mut path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        if !self.viewport_contains(pos) {
            return None;
        }
        let pos = pos - self.subcell_offset();
        for item in &self.items {
            if !Self::item_contains(&*item.widget, pos) {
                continue;
            }
            if let Some(r) = item.widget.descendant_at_pos(
                pos,
                path.as_mut().map(|p| &mut **p),
            ) {
                if let Some(p) = &mut path {
                    p.push(item.widget.get_id());
                }
                return Some(r);
            }
            if let Some(p) = &mut path {
                p.push(item.widget.get_id());
            }
            return Some(item.widget.get_id());
        }
        None
    }

    fn find_descendant_at_pos(
        &self,
        pos: Vec2<f32>,
        predicate: &dyn Fn(&dyn Widget) -> bool,
        mut path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        if !self.viewport_contains(pos) {
            return None;
        }
        let pos = pos - self.subcell_offset();
        for item in &self.items {
            if !Self::item_contains(&*item.widget, pos) {
                continue;
            }
            let grandchild = item.widget.find_descendant_at_pos(
                pos,
                predicate,
                path.as_mut().map(|p| &mut **p),
            );
            if grandchild.is_some() {
                if let Some(p) = &mut path {
                    p.push(item.widget.get_id());
                }
                return grandchild;
            } else if predicate(&*item.widget) {
                if let Some(p) = &mut path {
                    p.push(item.widget.get_id());
                }
                return Some(item.widget.get_id());
            }
        }
        None
    }

    fn get_cursor(
        &self,
        selected: Option<WidgetId>,
    ) -> Option<(CursorShape, Vec2<i32>)> {
        let selected = selected?;
        for item in &self.items {
            if let Some((style, pos)) = self.layout.get_child_cursor(&*item.widget, selected) {
                let viewport = self.get_viewport().map(|v| v as i32);
                if pos.x < 0 || pos.x > viewport.x {
                    return None;
                }
                if pos.y < 0 || pos.y >= viewport.y {
                    return None;
                }
                return Some((style, pos));
            }
        }
        None
    }
}

impl List {
    /// Creates an empty list with no renderer attached.
    pub fn new() -> Box<Self> {
        Box::new(Self {
            layout: Layout::new(),
            len: 0,
            render_context: None,
            render_fn: None,
            items: Vec::new(),
            reuse_buf: Vec::new(),
            window_start: 0,
            anchor: Anchor::zero(),
            scroll_target: ScrollTarget::None,
            pending_request: None,
            pending_progress: None,
            avg_height: 1.0,
            orientation: Axis2D::Y,
            direction: Sign::Positive,
            align: AlignSpec::default(),
            gap: GapConfig::new(),
            chrome: None,
            scroll: ScrollConfig::new(),
            insets: Spacing::new(),
            first_item_offset: 0,
        })
    }

    /// Sets the render function and its owned context.
    pub fn set_renderer<T: 'static>(
        &mut self,
        context: T,
        render: fn(&mut T, usize) -> Option<Box<dyn Widget>>,
    ) {
        self.render_context = Some(Box::new(context));
        self.render_fn = Some(Box::new(move |any, index| {
            render(any.downcast_mut::<T>().unwrap(), index)
        }));
    }

    /// Returns the renderer context as `T`, if it is a `T`.
    pub fn get_context<T: 'static>(&self) -> Option<&T> {
        self.render_context.as_ref()?.downcast_ref()
    }

    /// Returns the renderer context as `&mut T`, if it is a `T`.
    pub fn get_context_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.render_context.as_mut()?.downcast_mut()
    }

    /// Builder form of [`List::set_item_count`].
    pub fn item_count(mut self: Box<Self>, count: usize) -> Box<Self> {
        self.set_item_count(count);
        self
    }

    /// Sets the total item count.
    pub fn set_item_count(&mut self, count: usize) {
        self.len = count;
        self.scroll.cross_content_size = 0;
        self.scroll.subcell_scroll = Vec2::of(0.0);
        if count == 0 {
            self.anchor = Anchor::zero();
        } else if self.anchor.index >= count {
            self.anchor.index = count - 1;
        }
        self.dirty_layout();
    }

    /// Returns the total item count.
    pub fn get_item_count(&self) -> usize {
        self.len
    }

    /// Returns `true` if the item count is zero.
    pub fn is_empty(&self) -> bool {
        self.get_item_count() == 0
    }

    /// Returns the estimated content size in cells.
    pub fn get_content_size(&self) -> Vec2<u32> {
        let mut size = Vec2::of(0u32);
        size[self.orientation] = self.main_content_size();
        size[self.orientation.flip()] = self.scroll.cross_content_size as u32;
        size
    }

    /// Returns the range of item indices currently intersecting the viewport.
    pub fn get_visible_range(&self) -> std::ops::Range<usize> {
        let viewport = self.get_viewport();
        if self.items.is_empty() || viewport[self.orientation] == 0 {
            return 0..0;
        }
        let a = self.orientation;
        let vp = viewport[a] as i32;
        let gap = self.gap.size as i32;
        let mut start = None;
        let mut end = self.window_start;
        let mut offset = self.offset_from_anchor(0);

        for i in 0..self.items.len() {
            let h = self.item_height(i) as i32;
            let data_idx = self.window_start + i;
            if offset + h > 0 && start.is_none() {
                start = Some(data_idx);
            }
            if offset < vp {
                end = data_idx + 1;
            } else if start.is_some() {
                break;
            }
            offset += h + gap;
        }

        start.unwrap_or(end)..end
    }

    /// Marks items in `range` dirty so they are re-rendered on the next layout.
    pub fn invalidate_range(&mut self, range: std::ops::Range<usize>) {
        let window_end = self.window_start + self.items.len();
        let overlap_start = range.start.max(self.window_start);
        let overlap_end = range.end.min(window_end);
        for i in overlap_start..overlap_end {
            let slot = i - self.window_start;
            if slot < self.items.len() {
                self.items[slot].dirty = true;
            }
        }
    }

    /// Marks all cached items dirty so they are re-rendered on the next layout.
    pub fn invalidate_all(&mut self) {
        for item in &mut self.items {
            item.dirty = true;
        }
        self.dirty_layout();
    }

    /// Resets the list to empty, clearing all cached items and scroll state.
    pub fn reset(&mut self) {
        self.len = 0;
        self.items.clear();
        self.window_start = 0;
        self.anchor = Anchor::zero();
        self.scroll_target = ScrollTarget::None;
        self.pending_progress = None;
        self.scroll.cross_content_size = 0;
        self.scroll.subcell_scroll = Vec2::of(0.0);
        self.dirty_layout();
    }

    /// Scrolls minimally to bring `index` into view.
    pub fn ensure_visible(&mut self, index: usize) -> bool {
        self.ensure_visible_scrolloff(index, 0)
    }

    /// Like [`List::ensure_visible`] but preserves `scrolloff` cells of margin around the item.
    pub fn ensure_visible_scrolloff(&mut self, index: usize, scrolloff: u16) -> bool {
        if index >= self.len {
            return false;
        }
        let scrolloff = scrolloff as i32;
        let viewport = self.get_viewport()[self.orientation] as i32;
        if viewport == 0 {
            return false;
        }

        let scrolled = if let Some(wi) = self.data_to_window(index) {
            let offset = self.offset_from_anchor(wi);
            let height = self.item_height(wi) as i32;
            let scrolloff = if height + scrolloff * 2 <= viewport {
                scrolloff
            } else {
                0
            };

            if offset - scrolloff >= 0
                && offset + height + scrolloff <= viewport
            {
                false
            } else {
                let target_screen = self.screen_pos(offset, height).clamp(
                    scrolloff,
                    (viewport - height - scrolloff).max(scrolloff),
                );
                self.anchor = Anchor {
                    index,
                    offset: self.screen_to_offset(target_screen, height),
                };
                self.fill();
                self.sync_scrollbars();
                self.dirty_paint();
                true
            }
        } else {
            self.scroll_target = ScrollTarget::Index { index, align: None };
            self.dirty_layout();
            false
        };

        self.flush_events(scrolled);
        scrolled
    }

    /// Returns the [`WidgetId`] of the cached widget at `index`, if it is materialized.
    pub fn get_item_id(&self, index: usize) -> Option<WidgetId> {
        self.data_to_window(index)
            .map(|wi| self.items[wi].widget.get_id())
    }

    /// Calls `f` with the cached widget at `index` if it is materialized.
    pub fn with_item_widget<R>(&self, index: usize, f: impl FnOnce(&dyn Widget) -> R) -> Option<R> {
        self.data_to_window(index).map(|wi| f(&*self.items[wi].widget))
    }

    /// Calls `f` with mutable access to the cached widget at `index` if it is materialized.
    pub fn with_item_widget_mut<R>(&mut self, index: usize, f: impl FnOnce(&mut dyn Widget) -> R) -> Option<R> {
        self.data_to_window(index).map(|wi| f(&mut *self.items[wi].widget))
    }

    /// Returns the scroll position on `axis` as a fraction in `[0.0, 1.0]`.
    pub fn get_scroll_progress(&self, axis: Axis2D) -> f32 {
        let viewport = self.get_viewport();
        let sub = self.scroll.subcell_scroll[axis];
        if axis == self.orientation {
            if let Some(p) = self.pending_progress {
                return p;
            }
            if self.len <= 1 {
                return 0.0;
            }
            let max_scroll = self.main_content_size().saturating_sub(viewport[self.orientation] as u32);
            ScrollbarState::progress_from_subcell(self.scroll_from_anchor(), sub, max_scroll)
        } else {
            ScrollbarState::progress_from_subcell(self.scroll.cross_scroll_offset, sub, self.max_cross_scroll())
        }
    }

    /// Scrolls `axis` to fractional position `progress` in `[0.0, 1.0]`.
    pub fn set_scroll_progress(&mut self, axis: Axis2D, progress: f32) {
        if axis == self.orientation {
            if self.len == 0 {
                return;
            }
            self.scroll_target = ScrollTarget::Progress(progress);
            let filled = self.fill();
            if filled {
                self.sync_scrollbars();
                self.dirty_paint();
                self.flush_events(true);
            } else {
                self.flush_events(false);
            }
        } else if self.scroll.cross_mode.is_some() {
            let progress = progress.clamp(0.0, 1.0);
            self.apply_cross_scroll((self.max_cross_scroll() as f64 * progress as f64) as u32);
        }
    }

    /// Returns the viewport-to-content size ratio on `axis`, clamped to `[0.0, 1.0]`.
    pub fn get_scroll_ratio(&self, axis: Axis2D) -> f32 {
        let vp = self.get_viewport()[axis] as f32;
        let content = self.get_content_size()[axis] as f32;
        if content <= 0.0 {
            1.0
        } else {
            (vp / content).min(1.0)
        }
    }

    /// Scrolls by `delta` cells along the main axis.
    pub fn scroll_by(&mut self, delta: i32) {
        if delta == 0 {
            return;
        }
        self.scroll_target = ScrollTarget::Delta(delta);
        self.fill();
        self.sync_scrollbars();
        self.dirty_paint();
        self.flush_events(true);
    }

    crate::layout_field! {
        /// Axis the items stack along.
        orientation: Axis2D
    }

    crate::style_field! {
        /// Edge cells reserved so the scrollbar does not overlap them.
        insets: Spacing => insets
    }

    /// Sets the orientation to [`Axis2D::Y`].
    pub fn vertical(mut self: Box<Self>) -> Box<Self> {
        self.set_orientation(Axis2D::Y);
        self
    }

    /// Sets the orientation to [`Axis2D::X`].
    pub fn horizontal(mut self: Box<Self>) -> Box<Self> {
        self.set_orientation(Axis2D::X);
        self
    }

    /// Returns `true` if the orientation is [`Axis2D::X`].
    pub fn is_horizontal(&self) -> bool {
        self.get_orientation() == Axis2D::X
    }

    /// Returns `true` if the orientation is [`Axis2D::Y`].
    pub fn is_vertical(&self) -> bool {
        self.get_orientation() == Axis2D::Y
    }

    crate::layout_field! {
        /// Lays items out forwards or in reverse along the axis.
        direction: Sign
    }

    /// Builder form of [`List::set_x_place`].
    pub fn x_place(mut self: Box<Self>, p: Place) -> Box<Self> {
        self.set_x_place(p);
        self
    }

    /// Builder form of [`List::set_y_place`].
    pub fn y_place(mut self: Box<Self>, p: Place) -> Box<Self> {
        self.set_y_place(p);
        self
    }

    /// Sets the cross-axis placement for items along x.
    pub fn set_x_place(&mut self, p: Place) {
        let new_align = self.align.place_x(p);
        if self.align == new_align {
            return;
        }
        self.align = new_align;
        self.dirty_layout();
    }

    /// Sets the cross-axis placement for items along y.
    pub fn set_y_place(&mut self, p: Place) {
        let new_align = self.align.place_y(p);
        if self.align == new_align {
            return;
        }
        self.align = new_align;
        self.dirty_layout();
    }

    /// Returns the cross-axis placement for items along x.
    pub fn get_x_place(&self) -> Place {
        self.align.get_place_x()
    }

    /// Returns the cross-axis placement for items along y.
    pub fn get_y_place(&self) -> Place {
        self.align.get_place_y()
    }

    crate::layout_field! {
        /// Cells of space between items.
        gap: u8 => gap.size
    }

    crate::style_field! {
        /// Border drawn in the gap between items.
        gap_border: Option<&'static Border> => gap.border
    }

    crate::style_field! {
        /// Style for the gap separators.
        gap_border_style: Style => gap.border_style
    }

    crate::layout_field! {
        /// Scrollbar mode for the main axis, which always scrolls. Stored
        /// orientation-relative so rotation keeps the config on the item axis.
        scroll: Scrollbar => scroll.main_mode
    }

    crate::field! {
        /// Scrollbar mode for the cross axis, or `None` to disable it.
        cross_scroll: Option<Scrollbar> => scroll.cross_mode;
        cross_scroll_did_change
    }

    fn cross_scroll_did_change(&mut self) {
        if self.scroll.cross_mode.is_none() {
            self.scroll.cross_scroll_offset = 0;
            self.scroll.subcell_scroll[self.orientation.flip()] = 0.0;
        }
        self.dirty_layout();
    }

    /// Sets the scrollbar style.
    pub fn set_scrollbar_style(&mut self, style: ScrollbarStyle) {
        self.scroll.style = style;
        self.dirty_paint();
    }

    /// Returns the scrollbar style.
    pub fn get_scrollbar_style(&self) -> ScrollbarStyle {
        self.scroll.style.clone()
    }

    /// Builder form of [`List::set_scrollbar_style`].
    pub fn scrollbar_style(mut self: Box<Self>, style: ScrollbarStyle) -> Box<Self> {
        self.set_scrollbar_style(style);
        self
    }

    crate::layout_field! {
        /// Whether to draw the default border.
        bordered: bool => chrome?.bordered
    }

    crate::field! {
        /// The border to draw, or `None` to remove it.
        border: Option<&'static Border> => chrome?.border;
        border_did_change
    }

    crate::style_field! {
        /// Style for the border.
        border_style: Style => chrome?.border_style
    }

    crate::layout_field! {
        /// Space between the border and the content.
        padding: Spacing => chrome?.padding
    }

    /// Sets top and bottom padding to `n`.
    pub fn vertical_padding(mut self: Box<Self>, n: u8) -> Box<Self> {
        self.set_padding(self.get_padding().vertical(n));
        self
    }

    /// Sets left and right padding to `n`.
    pub fn horizontal_padding(mut self: Box<Self>, n: u8) -> Box<Self> {
        self.set_padding(self.get_padding().horizontal(n));
        self
    }

    /// Sets left padding to `n`.
    pub fn padding_left(mut self: Box<Self>, n: u8) -> Box<Self> {
        self.set_padding(self.get_padding().left(n));
        self
    }

    /// Sets right padding to `n`.
    pub fn padding_right(mut self: Box<Self>, n: u8) -> Box<Self> {
        self.set_padding(self.get_padding().right(n));
        self
    }

    /// Sets top padding to `n`.
    pub fn padding_top(mut self: Box<Self>, n: u8) -> Box<Self> {
        self.set_padding(self.get_padding().top(n));
        self
    }

    /// Sets bottom padding to `n`.
    pub fn padding_bottom(mut self: Box<Self>, n: u8) -> Box<Self> {
        self.set_padding(self.get_padding().bottom(n));
        self
    }

    /// Builder form of [`List::set_title_at`].
    pub fn title_at(
        mut self: Box<Self>,
        edge: VerticalEdge,
        align: Align,
        title: impl Into<String>,
    ) -> Box<Self> {
        self.set_title_at(edge, align, Some(title.into()));
        self
    }

    /// Sets the top-left title.
    pub fn title(self: Box<Self>, title: impl Into<String>) -> Box<Self> {
        self.title_at(VerticalEdge::Top, Align::Start, title)
    }

    /// Sets the title at `edge` and `align`, or clears it if `title` is `None`.
    pub fn set_title_at(
        &mut self,
        edge: VerticalEdge,
        align: Align,
        title: Option<String>,
    ) {
        self.get_chrome_mut().set_title_at(edge, align, title);
        self.dirty_paint();
    }

    /// Sets the top-left title, or clears it if `title` is `None`.
    pub fn set_title(&mut self, title: Option<String>) {
        self.set_title_at(VerticalEdge::Top, Align::Start, title);
    }

    /// Returns the top-left title, if set.
    pub fn get_title(&self) -> Option<&str> {
        self.get_title_at(VerticalEdge::Top, Align::Start)
    }

    /// Returns the title at `edge` and `align`, if set.
    pub fn get_title_at(&self, edge: VerticalEdge, align: Align) -> Option<&str> {
        self.get_chrome()?.get_title_at(edge, align)
    }
}
