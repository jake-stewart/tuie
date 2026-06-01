//! Flex container widget.

use crate::prelude::*;
use crate::widget::chrome::ChromeHost;
use crate::widget::align::{AlignSpec, FlexAlign, Place};
use crate::widget::{get_flow_output_size_layout, get_flow_output_size_measure};
use crate::util::stack_pool::StackPool;
use crate::widget::flex::{self, AsFlexItem, FlexItem};
use chord_macro::chord;
use sign::Directional;

thread_local! {
    static FLEX_POOL: StackPool<FlexState> = const { StackPool::new() };
}

#[derive(Clone, Copy)]
struct Slot {
    item: FlexItem,
    measured: Vec2<u16>,
    commit: Vec2<u16>,
}

impl Slot {
    const EMPTY: Self = Self {
        item: FlexItem::new(0, 0, 0, 0),
        measured: Vec2::of(0),
        commit: Vec2::of(0),
    };
}

impl AsFlexItem for Slot {
    fn flex_item(&self) -> &FlexItem { &self.item }
    fn flex_item_mut(&mut self) -> &mut FlexItem { &mut self.item }
}

#[derive(Default)]
struct FlexState {
    slots: Vec<Slot>,
}

impl FlexState {
    fn resize_to(&mut self, n: usize) {
        self.slots.clear();
        self.slots.resize(n, Slot::EMPTY);
    }
}

struct ScrollConfig {
    scroll: Vec2<u32>,
    subcell_scroll: Vec2<f32>,
    mode: Vec2<Option<Scrollbar>>,
    scrollbar: Vec2<ScrollbarState>,
    style: ScrollbarStyle,
}

impl ScrollConfig {
    fn new() -> Self {
        Self {
            scroll: Vec2::of(0),
            subcell_scroll: Vec2::of(0.0),
            mode: Vec2::of(None),
            scrollbar: Vec2::new(ScrollbarState::new(), ScrollbarState::new()),
            style: ScrollbarStyle::new(),
        }
    }
}

struct WrapConfig {
    balanced: bool,
    cross_gap: u8,
    line_starts: Vec<usize>,
    resolved_size: Vec2<u16>,
}

impl Default for WrapConfig {
    fn default() -> Self {
        Self {
            balanced: false,
            cross_gap: 0,
            line_starts: Vec::new(),
            resolved_size: Vec2::of(0),
        }
    }
}

#[derive(Default)]
struct WrapScratch {
    items: Vec<FlexItem>,
    cross_sizes: Vec<u16>,
    line_cross_sizes: Vec<u16>,
    line_starts: Vec<usize>,
    best: Vec<u64>,
    from: Vec<usize>,
}

thread_local! {
    static WRAP_POOL: StackPool<WrapScratch> = const { StackPool::new() };
}

/// Flex container that lays children along an axis.
pub struct Pane {
    layout: Layout,
    children: Vec<Box<dyn Widget>>,
    gap: u8,
    orientation: Axis2D,
    align: AlignSpec,
    chrome: Option<Box<Chrome>>,
    scroll: Option<Box<ScrollConfig>>,
    wrap: Option<Box<WrapConfig>>,
    insets: Spacing,
}

impl Pane {
    fn get_scroll_cfg(&self) -> Option<&ScrollConfig> {
        self.scroll.as_deref()
    }

    fn get_scroll_cfg_mut(&mut self) -> &mut ScrollConfig {
        self.scroll.get_or_insert_with(|| Box::new(ScrollConfig::new()))
    }


    fn derive_basis(child: &dyn Widget, measured: Vec2<u16>, main: Axis2D) -> u16 {
        let cl = child.get_layout();
        let flex = child.get_flex();
        if flex > 0 {
            0
        } else if child.get_flow_axis() == main {
            measured[main]
        } else {
            cl.constraints.preferred_size[main]
        }
    }

    fn derive_min_main_eff(child: &dyn Widget, measured: Vec2<u16>, main: Axis2D) -> u16 {
        let cl = child.get_layout();
        let max_main = cl.constraints.max_size[main];
        let min_eff = if child.get_flow_axis() == main {
            cl.get_explicit_min(main).unwrap_or(measured[main])
        } else {
            cl.constraints.min_size[main]
        };
        min_eff.min(max_main)
    }

    fn get_children_min_size(&self, base_size: impl Fn(&Layout) -> Vec2<u16>) -> Vec2<u16> {
        let chrome = self.get_chrome_total();
        let axis = self.orientation;
        let cross = axis.flip();
        let mut size = Vec2::of(0u16);
        size[axis] = {
            let main_total: u32 = self.children.iter()
                .map(|child| base_size(child.get_layout())[axis] as u32)
                .sum::<u32>()
                .saturating_add(self.get_gap_total());
            std::cmp::min(main_total, u16::MAX as u32) as u16
        };
        size[cross] = self.children.iter()
            .map(|child| base_size(child.get_layout())[cross])
            .max()
            .unwrap_or(0);
        Axis2D::map(|a| size[a].saturating_add(chrome[a]))
    }

    fn get_scroll_val(&self) -> Vec2<u32> {
        self.get_scroll_cfg().map_or(Vec2::of(0), |sc| sc.scroll)
    }

    fn is_scrolling(&self) -> bool {
        self.is_scroll_enabled(Axis2D::X) || self.is_scroll_enabled(Axis2D::Y)
    }

    fn is_scroll_enabled(&self, axis: Axis2D) -> bool {
        self.get_scroll_cfg().is_some_and(|sc| sc.mode[axis].is_some())
    }

    fn get_scrollbar_gutter(&self, axis: Axis2D) -> u16 {
        match self.get_scroll_cfg() {
            Some(sc) if sc.scrollbar[axis].is_visible() => 1,
            _ => 0,
        }
    }

    fn get_axis_pad(&self, a: Axis2D) -> u16 {
        let gutter = self.get_scrollbar_gutter(a.flip());
        self.get_padding_before(a)
            + self.get_padding_after(a).saturating_sub(gutter)
    }

    fn get_effective_pad(&self) -> Vec2<u16> {
        Axis2D::map(|a| self.get_axis_pad(a))
    }

    fn get_axis_overhead(&self, a: Axis2D) -> u16 {
        let border = self.get_border_cells() * 2;
        let gutter = self.get_scrollbar_gutter(a.flip());
        let pad = if self.is_scroll_enabled(a) {
            0
        } else {
            self.get_axis_pad(a)
        };
        border.saturating_add(gutter).saturating_add(pad)
    }

    /// Returns the maximum scroll distance on `axis` in cells.
    fn get_max_scroll(&self, axis: Axis2D) -> u32 {
        let viewport = self.get_viewport_size();
        self.get_padded_content_size()[axis].saturating_sub(viewport[axis] as u32)
    }

    fn get_scroll_offset(&self) -> Vec2<i32> {
        match self.get_scroll_cfg() {
            Some(sc) => Axis2D::map(|a| {
                if sc.mode[a].is_some() {
                    sc.scroll[a] as i32
                } else {
                    0
                }
            }),
            None => Vec2::of(0),
        }
    }

    fn get_gap_total(&self) -> u32 {
        self.children.len().saturating_sub(1) as u32 * self.gap as u32
    }

    fn get_children_extent(&self) -> Vec2<u32> {
        if let Some(w) = self.get_wrap() {
            return Axis2D::map(|a| w.resolved_size[a] as u32);
        }
        let axis = self.orientation;
        let cross = axis.flip();
        let mut extent = Vec2::of(0u32);
        extent[axis] = self.children.iter()
            .map(|c| c.get_outer_size()[axis] as u32)
            .fold(0u32, u32::saturating_add)
            .saturating_add(self.get_gap_total());
        extent[cross] = self.children.iter()
            .map(|c| c.get_outer_size()[cross] as u32)
            .max()
            .unwrap_or(0);
        extent
    }

    fn get_padded_content_size(&self) -> Vec2<u32> {
        let extent = self.get_children_extent();
        let pad = self.get_effective_pad();
        Axis2D::map(|a| extent[a].saturating_add(pad[a] as u32))
    }

    fn clamp_scroll(&mut self) {
        let viewport = self.get_viewport_size();
        let padded = self.get_padded_content_size();
        let Some(sc) = self.scroll.as_deref_mut() else {
            return;
        };
        for a in [Axis2D::X, Axis2D::Y] {
            if sc.mode[a].is_some() {
                let max_scroll = padded[a].saturating_sub(viewport[a] as u32);
                sc.scroll[a] = sc.scroll[a].min(max_scroll);
            } else {
                sc.scroll[a] = 0;
            }
        }
    }

    fn sync_scrollbars(&mut self) {
        let viewport = self.get_viewport_size();
        let padded = self.get_padded_content_size();
        let Some(sc) = self.scroll.as_deref_mut() else {
            return;
        };
        for a in [Axis2D::X, Axis2D::Y] {
            let content_size = padded[a];
            let ratio = if content_size > 0 {
                viewport[a] as f32 / content_size as f32
            } else {
                1.0
            };
            let max_scroll = content_size.saturating_sub(viewport[a] as u32);
            let progress = ScrollbarState::progress_from_subcell(sc.scroll[a], sc.subcell_scroll[a], max_scroll);
            let bar = &mut sc.scrollbar[a];
            bar.set_ratio(ratio);
            bar.set_progress(progress);
        }
    }

    fn resolve_bar_visibility(&mut self) -> bool {
        let Some(sc) = self.get_scroll_cfg() else {
            return false;
        };
        let needs_overflow_check = sc.mode.x == Some(Scrollbar::AutoHide)
            || sc.mode.y == Some(Scrollbar::AutoHide);
        let children_min = if needs_overflow_check {
            let axis = self.orientation;
            let cross = axis.flip();
            let pad = self.get_effective_pad();
            let gap_total = self.get_gap_total();
            let mut v = Vec2::of(0u32);
            v[axis] = self.children.iter()
                .map(|c| c.get_outer_size()[axis] as u32)
                .sum::<u32>()
                + gap_total
                + pad[axis] as u32;
            v[cross] = self.children.iter()
                .map(|c| c.get_outer_size()[cross] as u32)
                .max()
                .unwrap_or(0)
                + pad[cross] as u32;
            v
        } else {
            Vec2::of(0u32)
        };
        let borders = self.get_border_cells() * 2;
        let base = self.layout.rect.size.map(|v| v.saturating_sub(borders) as u32);

        let sc = self.scroll.as_mut().unwrap();
        let cross_gutter = Axis2D::map(|a| sc.scrollbar[a.flip()].is_visible() as u32);
        let visible = Axis2D::map(|a| match sc.mode[a] {
            None | Some(Scrollbar::Hidden) => false,
            Some(Scrollbar::AutoHide) => children_min[a] > base[a].saturating_sub(cross_gutter[a]),
            Some(Scrollbar::Visible) => true,
        });
        let mut changed = false;
        for a in [Axis2D::X, Axis2D::Y] {
            if sc.scrollbar[a].is_visible() != visible[a] {
                sc.scrollbar[a].set_visible(visible[a]);
                changed = true;
            }
        }
        changed
    }


    fn get_inner_content_size(&self) -> Vec2<u16> {
        let chrome = self.get_chrome_total();
        let content = self.layout.rect.size;
        Axis2D::map(|a| content[a].saturating_sub(chrome[a]))
    }

    fn get_inner_content_pos(&self) -> Vec2<i32> {
        self.layout.rect.pos + self.get_chrome_before().map(|v| v as i32)
    }

    fn get_children_min_along(&self, a: Axis2D) -> u16 {
        if a == self.orientation {
            self.children.iter()
                .map(|c| c.get_layout().constraints.min_size[a] as u32)
                .sum::<u32>()
                .saturating_add(self.get_gap_total())
                .min(u16::MAX as u32) as u16
        } else {
            self.children.iter()
                .map(|c| c.get_layout().constraints.min_size[a])
                .max()
                .unwrap_or(0)
        }
    }

    fn pos_in_viewport(&self, pos: Vec2<f32>) -> bool {
        let border = self.get_border_offset();
        let content_pos = self.layout.rect.pos + border;
        let viewport = self.get_viewport_size();
        Axis2D::all(|a| {
            pos[a] >= content_pos[a] as f32
                && pos[a] < (content_pos[a] + viewport[a] as i32) as f32
        })
    }

    fn child_contains(child: &dyn Widget, pos: Vec2<f32>) -> bool {
        let slot_pos = child.get_pos();
        let slot_size = child.get_rect_size().map(|v| v as i32);
        Axis2D::all(|a| {
            pos[a] >= slot_pos[a] as f32 && pos[a] < (slot_pos[a] + slot_size[a]) as f32
        })
    }

    fn subcell_offset(&self) -> Vec2<f32> {
        if !crate::runtime::is_gui() {
            return Vec2::of(0.0);
        }
        let Some(sc) = self.scroll.as_deref() else {
            return Vec2::of(0.0);
        };
        Axis2D::map(|a| -sc.subcell_scroll[a])
    }

    fn handle_scrollbar_input(
        &mut self,
        chord: &Chord,
        mouse_pos: Vec2<f32>,
        filter: impl Fn(&ScrollbarState, Axis2D, Vec2<i32>, Vec2<u16>, bool, u16, u16) -> bool,
    ) -> bool {
        let border = self.get_border_offset();
        let viewport = self.get_viewport_size();
        let local = mouse_pos - border.map(|v| v as f32);
        let local_cell = Axis2D::map(|a| local[a].floor() as i32);
        let insets = self.insets;

        let mut handled = None;
        if let Some(sc) = &mut self.scroll {
            let has_both = sc.scrollbar[Axis2D::X].is_visible() && sc.scrollbar[Axis2D::Y].is_visible();
            for a in [Axis2D::X, Axis2D::Y] {
                let inset_before = insets.get_before(a) as u16;
                let inset_after = insets.get_after(a) as u16;
                if !filter(&sc.scrollbar[a], a, local_cell, viewport, has_both, inset_before, inset_after) {
                    continue;
                }
                let base = if a == Axis2D::Y && has_both {
                    viewport[a] as f32 + 0.5
                } else {
                    viewport[a] as f32
                };
                let size = base - inset_before as f32 - inset_after as f32;
                let is_release = matches!(chord, chord!(LeftRelease));
                if !is_release && size <= 0.0 {
                    continue;
                }
                let result = sc.scrollbar[a].handle_input(
                    chord,
                    local[a] - inset_before as f32,
                    size,
                );
                if let ScrollbarInputResult::Handled(progress) = result {
                    handled = Some((a, progress));
                    break;
                }
            }
        }
        if let Some((a, progress)) = handled {
            if let Some(p) = progress {
                self.set_scroll_progress(a, p);
            }
            return true;
        }
        false
    }

    fn flex_resolve(&self, allocated: Vec2<u16>, state: &mut FlexState) {
        if self.children.is_empty() {
            return;
        }
        let borders = self.get_border_cells() * 2;
        let viewport = Axis2D::map(|a| {
            let gutter = self.get_scrollbar_gutter(a.flip());
            allocated[a].saturating_sub(gutter).saturating_sub(borders)
        });
        let pad = self.get_effective_pad();
        let layout_size = Axis2D::map(|a| viewport[a].saturating_sub(pad[a]));
        let main = self.orientation;
        let cross = main.flip();
        let container_main = layout_size[main];
        let container_cross = layout_size[cross];
        let gap_total = self.get_gap_total();
        let cross_mode: FlexAlign = self.align.get_place(cross).try_into().unwrap_or(FlexAlign::Start);
        let cross_scroll = self.is_scroll_enabled(cross);

        let n = self.children.len();
        state.resize_to(n);

        for (i, child) in self.children.iter().enumerate() {
            let cl = child.get_layout();
            let flow_axis = child.get_flow_axis();
            if flow_axis != main {
                continue;
            }
            let pref_cross = cl.constraints.preferred_size[cross];
            let child_mode = cl.align.get_mode(cross).unwrap_or(cross_mode);
            let stretching = child_mode == FlexAlign::Stretch;
            let pre_cross = if stretching {
                container_cross
            } else {
                pref_cross.min(container_cross)
            }.max(cl.constraints.min_size[cross]);
            let mut size_in = Vec2::of(0u16);
            size_in[cross] = pre_cross;
            size_in[main] = if child.get_flex() > 0 {
                container_main
            } else {
                u16::MAX
            };
            let out = flow_child_measure(&**child, size_in);
            state.slots[i].measured = out;
        }

        for (i, child) in self.children.iter().enumerate() {
            let measured = state.slots[i].measured;
            let basis = Self::derive_basis(&**child, measured, main);
            let min = Self::derive_min_main_eff(&**child, measured, main);
            let max = child.get_layout().constraints.max_size[main];
            state.slots[i].item = FlexItem::new(basis, min, max, child.get_flex());
        }
        flex::resolve(&mut state.slots, container_main, gap_total);

        for (i, child) in self.children.iter().enumerate() {
            let cl = child.get_layout();
            let explicit_pref_cross = cl.get_explicit_pref(cross);
            let min_cross = cl.constraints.min_size[cross];
            let max_cross = cl.constraints.max_size[cross];
            let flow_axis = child.get_flow_axis();
            let child_mode = cl.align.get_mode(cross).unwrap_or(cross_mode);
            let stretching = child_mode == FlexAlign::Stretch;
            let used_main = state.slots[i].item.target;
            let measured_cross = state.slots[i].measured[cross];

            let used_cross = if stretching && !cross_scroll {
                container_cross.clamp(min_cross, max_cross)
            } else {
                let hypothetical_cross = if let Some(explicit) = explicit_pref_cross {
                    explicit
                } else if flow_axis == main {
                    measured_cross
                } else {
                    let mut size_in = Vec2::of(0u16);
                    size_in[main] = used_main;
                    size_in[cross] = container_cross;
                    let out = flow_child_measure(&**child, size_in);
                    out[cross]
                };
                if stretching {
                    container_cross.max(hypothetical_cross).clamp(min_cross, max_cross)
                } else {
                    hypothetical_cross.min(container_cross).clamp(min_cross, max_cross)
                }
            };

            let mut commit = Vec2::of(0u16);
            commit[main] = used_main;
            commit[cross] = used_cross;
            state.slots[i].commit = commit;
        }
    }

    fn get_wrap(&self) -> Option<&WrapConfig> {
        self.wrap.as_deref()
    }

    fn get_wrap_mut(&mut self) -> &mut WrapConfig {
        self.wrap.get_or_insert_with(Default::default)
    }

    fn line_range(&self, line: usize) -> std::ops::Range<usize> {
        let cfg = self.get_wrap().expect("line_range requires wrap config");
        let start = cfg.line_starts[line];
        let end = if line + 1 < cfg.line_starts.len() {
            cfg.line_starts[line + 1]
        } else {
            self.children.len()
        };
        start..end
    }

    fn wrap_resolve(&self, allocated: Vec2<u16>, scratch: &mut WrapScratch) -> Vec2<u16> {
        scratch.items.clear();
        scratch.cross_sizes.clear();
        scratch.line_cross_sizes.clear();
        scratch.line_starts.clear();

        let borders = self.get_border_cells() * 2;
        let viewport = Axis2D::map(|a| {
            let gutter = self.get_scrollbar_gutter(a.flip());
            allocated[a].saturating_sub(gutter).saturating_sub(borders)
        });
        let pad = self.get_effective_pad();
        let inner = Axis2D::map(|a| viewport[a].saturating_sub(pad[a]));

        let main_axis = self.orientation;
        let cross_axis = main_axis.flip();
        let container_main = inner[main_axis];
        let container_cross = inner[cross_axis];
        let gap_main = self.gap as u16;
        let (gap_cross, balanced) = match self.get_wrap() {
            Some(w) => (w.cross_gap as u16, w.balanced),
            None => (0, false),
        };
        let cross_mode: FlexAlign = self.align.get_place(cross_axis).try_into().unwrap_or(FlexAlign::Start);

        if self.children.is_empty() {
            return Vec2::of(0);
        }

        for child in self.children.iter() {
            let cl = child.get_layout();
            let pref_main = cl.constraints.preferred_size[main_axis];
            let min_main = cl.constraints.min_size[main_axis];
            let max_main = cl.constraints.max_size[main_axis];
            let pref_cross = cl.constraints.preferred_size[cross_axis];
            let flex = child.get_flex();
            let flow_axis = child.get_flow_axis();
            let child_mode = cl.align.get_mode(cross_axis).unwrap_or(cross_mode);
            let stretching = child_mode == FlexAlign::Stretch;

            let (basis, measured_cross, min_main_eff) = if flow_axis == main_axis {
                let pre_cross = if stretching {
                    container_cross
                } else {
                    pref_cross.min(container_cross)
                };
                let mut size_in = Vec2::of(0u16);
                size_in[cross_axis] = pre_cross;
                size_in[main_axis] = if flex > 0 {
                    container_main
                } else {
                    u16::MAX
                };
                let out = flow_child_measure(&**child, size_in);
                let measured_cross = out[cross_axis];
                let measured_basis = if flex > 0 {
                    0u16
                } else {
                    out[main_axis]
                };
                let min_eff = match cl.get_explicit_min(main_axis) {
                    Some(explicit) => explicit,
                    None => out[main_axis],
                };
                (measured_basis, measured_cross, min_eff)
            } else if flex > 0 {
                (0u16, pref_cross, min_main)
            } else {
                (pref_main, pref_cross, min_main)
            };

            let min_main_eff = min_main_eff.min(max_main);
            scratch.items.push(FlexItem::new(basis, min_main_eff, max_main, flex));
            scratch.cross_sizes.push(measured_cross);
        }

        let hypothetical_main = |it: &FlexItem| -> u16 {
            it.basis.clamp(it.min, it.max)
        };

        scratch.line_starts.push(0);
        let mut used = 0u16;
        for (i, item) in scratch.items.iter().enumerate() {
            let h = hypothetical_main(item);
            let needed = if used == 0 {
                h
            } else {
                h.saturating_add(gap_main)
            };
            if used > 0 && used.saturating_add(needed) > container_main {
                scratch.line_starts.push(i);
                used = h;
            } else {
                used = used.saturating_add(needed);
            }
        }
        if balanced && scratch.line_starts.len() > 1 {
            let n = scratch.items.len();
            const INF: u64 = u64::MAX;
            scratch.best.clear();
            scratch.best.resize(n + 1, INF);
            scratch.from.clear();
            scratch.from.resize(n + 1, 0);
            scratch.best[0] = 0;
            for i in 1..=n {
                let mut line_width: u32 = 0;
                for j in (0..i).rev() {
                    let h = hypothetical_main(&scratch.items[j]) as u32;
                    if j + 1 == i {
                        line_width = h;
                    } else {
                        line_width += h + gap_main as u32;
                    }
                    if line_width > container_main as u32 {
                        break;
                    }
                    if scratch.best[j] == INF {
                        continue;
                    }
                    let slack = container_main as u32 - line_width;
                    let cost = (slack as u64) * (slack as u64);
                    let total = scratch.best[j].saturating_add(cost);
                    if total < scratch.best[i] {
                        scratch.best[i] = total;
                        scratch.from[i] = j;
                    }
                }
            }
            if scratch.best[n] != INF {
                scratch.line_starts.clear();
                let mut k = n;
                while k > 0 {
                    let prev = scratch.from[k];
                    scratch.line_starts.push(prev);
                    k = prev;
                }
                scratch.line_starts.reverse();
            }
        }
        let num_lines = scratch.line_starts.len();

        for line in 0..num_lines {
            let start = scratch.line_starts[line];
            let end = if line + 1 < num_lines {
                scratch.line_starts[line + 1]
            } else {
                scratch.items.len()
            };
            if start >= end {
                continue;
            }
            let count = end - start;
            let line_gap_total = (count.saturating_sub(1) as u32) * gap_main as u32;
            flex::resolve(&mut scratch.items[start..end], container_main, line_gap_total);
        }

        for (i, child) in self.children.iter().enumerate() {
            let cl = child.get_layout();
            let min_cross = cl.constraints.min_size[cross_axis];
            let max_cross = cl.constraints.max_size[cross_axis];
            let explicit_pref_cross = cl.get_explicit_pref(cross_axis);
            let flow_axis = child.get_flow_axis();
            let used_main = scratch.items[i].target;

            let raw = if let Some(v) = explicit_pref_cross {
                v
            } else if flow_axis == main_axis {
                scratch.cross_sizes[i]
            } else {
                let mut size_in = Vec2::of(0u16);
                size_in[main_axis] = used_main;
                size_in[cross_axis] = container_cross;
                let out = flow_child_measure(&**child, size_in);
                out[cross_axis]
            };
            scratch.cross_sizes[i] = raw.clamp(min_cross, max_cross);
        }

        for line in 0..num_lines {
            let start = scratch.line_starts[line];
            let end = if line + 1 < num_lines {
                scratch.line_starts[line + 1]
            } else {
                scratch.items.len()
            };
            let line_cross = (start..end).map(|i| scratch.cross_sizes[i]).max().unwrap_or(0);
            scratch.line_cross_sizes.push(line_cross);
        }

        let mut max_main_used = 0u32;
        for line in 0..num_lines {
            let start = scratch.line_starts[line];
            let end = if line + 1 < num_lines {
                scratch.line_starts[line + 1]
            } else {
                scratch.items.len()
            };
            if start >= end {
                continue;
            }
            let line_cross = scratch.line_cross_sizes[line];
            let count = end - start;
            let line_gap_total = (count.saturating_sub(1) as u32) * gap_main as u32;
            let mut line_main: u32 = 0;
            for i in start..end {
                let cl = self.children[i].get_layout();
                let min_cross = cl.constraints.min_size[cross_axis];
                let max_cross = cl.constraints.max_size[cross_axis];
                let child_mode = cl.align.get_mode(cross_axis).unwrap_or(cross_mode);
                let stretching = child_mode == FlexAlign::Stretch;
                let used_main = scratch.items[i].target;
                let used_cross = if stretching {
                    line_cross.clamp(min_cross, max_cross)
                } else {
                    let upper = max_cross.min(line_cross);
                    scratch.cross_sizes[i].min(upper).max(min_cross)
                };
                scratch.cross_sizes[i] = used_cross;
                line_main += used_main as u32;
            }
            line_main += line_gap_total;
            max_main_used = max_main_used.max(line_main);
        }

        let total_cross: u32 = scratch.line_cross_sizes.iter().map(|&v| v as u32).sum::<u32>()
            + (num_lines.saturating_sub(1) as u32) * gap_cross as u32;

        let mut out = Vec2::of(0u16);
        out[main_axis] = max_main_used.min(u16::MAX as u32) as u16;
        out[cross_axis] = total_cross.min(u16::MAX as u32) as u16;
        out
    }

    fn wrap_commit_layout(&mut self, scratch: &WrapScratch) {
        let main_axis = self.orientation;
        let cross_axis = main_axis.flip();
        for (i, child) in self.children.iter_mut().enumerate() {
            let mut commit = Vec2::of(0u16);
            commit[main_axis] = scratch.items[i].target;
            commit[cross_axis] = scratch.cross_sizes[i];
            flow_child(&mut **child, commit);
        }
    }

    fn wrap_commit_measure(&self, scratch: &WrapScratch) {
        let main_axis = self.orientation;
        let cross_axis = main_axis.flip();
        for (i, child) in self.children.iter().enumerate() {
            let mut commit = Vec2::of(0u16);
            commit[main_axis] = scratch.items[i].target;
            commit[cross_axis] = scratch.cross_sizes[i];
            flow_child_measure(&**child, commit);
        }
    }

    fn wrap_layout_position(&mut self) {
        let scroll = self.get_scroll_offset();
        let base = self.get_inner_content_pos() - scroll;
        let main_axis = self.orientation;
        let cross_axis = main_axis.flip();
        let inner = self.get_inner_content_size();
        let container_main = inner[main_axis] as i32;
        let gap_main = self.gap as i32;
        let gap_cross = self.get_wrap().map(|w| w.cross_gap as i32).unwrap_or(0);
        let place_main = self.align.get_place(main_axis);
        let place_cross = self.align.get_place(cross_axis);
        let main_mode: FlexAlign = place_main.try_into().unwrap_or(FlexAlign::Start);
        let cross_mode: FlexAlign = place_cross.try_into().unwrap_or(FlexAlign::Start);
        let num_lines = self.get_wrap().map(|w| w.line_starts.len()).unwrap_or(0);
        if num_lines == 0 {
            return;
        }

        let mut line_cross_sizes = Vec::with_capacity(num_lines);
        for line in 0..num_lines {
            let range = self.line_range(line);
            let line_cross = range
                .map(|i| self.children[i].get_outer_size()[cross_axis])
                .max()
                .unwrap_or(0);
            line_cross_sizes.push(line_cross);
        }

        let mut line_offsets = Vec::with_capacity(num_lines);
        let mut cross_cursor = 0i32;
        for line in 0..num_lines {
            line_offsets.push(cross_cursor);
            cross_cursor += line_cross_sizes[line] as i32 + gap_cross;
        }

        for line in 0..num_lines {
            let range = self.line_range(line);
            let line_cross_size = line_cross_sizes[line];
            let cross_offset = line_offsets[line];
            let n = (range.end - range.start) as i32;
            let line_main_used: i32 = self.line_range(line)
                .map(|i| self.children[i].get_outer_size()[main_axis] as i32)
                .sum::<i32>()
                + (n - 1).max(0) * gap_main;
            let slack_main = (container_main - line_main_used).max(0);

            let mut main_offset = 0i32;
            for (ii, i) in range.enumerate() {
                let ii = ii as i32;
                let pre_gap = match place_main {
                    Place::Evenly if n >= 1 => {
                        slack_main * (ii + 1) / (n + 1) - slack_main * ii / (n + 1)
                    }
                    Place::Apart if n >= 2 => {
                        if ii == 0 {
                            0
                        } else {
                            slack_main * ii / (n - 1) - slack_main * (ii - 1) / (n - 1)
                        }
                    }
                    _ => match main_mode {
                        FlexAlign::Center if ii == 0 => slack_main / 2,
                        FlexAlign::End if ii == 0 => slack_main,
                        _ => 0,
                    },
                };
                main_offset += pre_gap;

                let child_mode = self.children[i]
                    .get_layout()
                    .align
                    .get_mode(cross_axis)
                    .unwrap_or(cross_mode);
                let slack_cross = line_cross_size as i32
                    - self.children[i].get_outer_size()[cross_axis] as i32;
                let cross_fit_offset = match child_mode {
                    FlexAlign::Start | FlexAlign::Stretch => 0,
                    FlexAlign::Center => slack_cross / 2,
                    FlexAlign::End => slack_cross,
                };
                let mut pos = Vec2::of(0i32);
                pos[main_axis] = main_offset;
                pos[cross_axis] = cross_offset + cross_fit_offset;
                let margin_before = self.children[i].get_layout().get_margin_before().map(|v| v as i32);
                self.children[i].set_pos(base + pos + margin_before);
                let sz = self.children[i].get_outer_size();
                main_offset += sz[main_axis] as i32 + gap_main;
                self.children[i].layout_position();
            }
        }
    }

    fn commit_layout(&mut self, state: &FlexState) {
        for (i, child) in self.children.iter_mut().enumerate() {
            flow_child(&mut **child, state.slots[i].commit);
        }
    }

    fn commit_measure(&self, state: &FlexState) {
        for (i, child) in self.children.iter().enumerate() {
            flow_child_measure(&**child, state.slots[i].commit);
        }
    }

    fn get_flex_total(&self, base_size: impl Fn(&Layout) -> Vec2<u16>) -> Vec2<u16> {
        let margin = self.layout.get_margin_total();
        if self.children.is_empty() {
            let slot = self.layout.constraints.min_size;
            return Axis2D::map(|a| slot[a].saturating_sub(margin[a]));
        }
        let flow = self.get_children_min_size(base_size);
        Axis2D::map(|a| {
            if self.is_scroll_enabled(a) && self.layout.get_explicit_max(a).is_none() {
                self.layout.constraints.min_size[a].saturating_sub(margin[a])
            } else {
                flow[a].saturating_add(self.get_scrollbar_gutter(a.flip()))
            }
        })
    }

    fn render_children(&self, child_ctx: &mut crate::render::RenderContext) {
        let a = self.orientation;
        let physical_start = child_ctx.position[a] as i32;
        let physical_end = physical_start + child_ctx.physical_size[a] as i32;
        let anchor = child_ctx.anchor;
        let monotonic = !self.is_wrapping();
        for child in self.children.iter() {
            let child_pos = child.get_pos();
            let slot_size_a = child.get_rect_size()[a] as i32;
            if monotonic && !child.get_layout().is_overflowing() {
                if child_pos[a] + slot_size_a <= physical_start {
                    continue;
                } else if child_pos[a] >= physical_end {
                    break;
                }
            }
            child_ctx.render_child(&**child, child_pos - anchor);
        }
    }

}

impl ChromeHost for Pane {
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

impl Widget for Pane {
    fn get_layout(&self) -> &Layout {
        &self.layout
    }

    fn get_layout_mut(&mut self) -> &mut Layout {
        &mut self.layout
    }

    fn get_name(&self) -> &'static str {
        "Pane"
    }

    fn get_scroll_clip_range(&self) -> Vec2<Option<(i32, i32)>> {
        if !self.is_scrolling() {
            return Vec2::of(None);
        }
        let border = self.get_border_offset();
        let vp_pos = self.layout.rect.pos + border;
        let vp_size = self.get_viewport_size();
        Axis2D::map(|a| Some((vp_pos[a], vp_pos[a] + vp_size[a] as i32)))
    }

    fn measure_constraints(&mut self) -> Constraints {
        self.each_child_mut(&mut constrain_child, Sign::Positive);
        let wrapping = self.is_wrapping();
        let min_size = Axis2D::map(|a| {
            let chrome = self.get_axis_overhead(a);
            let children_min = if wrapping {
                self.children.iter()
                    .map(|c| c.get_layout().constraints.min_size[a])
                    .max()
                    .unwrap_or(0)
            } else {
                self.get_children_min_along(a)
            };
            let outer_min = children_min.saturating_add(chrome);
            if self.is_scroll_enabled(a) && self.layout.get_explicit_max(a).is_none() {
                0
            } else {
                outer_min
            }
        });
        let orientation = self.orientation;
        let preferred_size = Axis2D::map(|a| {
            let chrome = self.get_axis_overhead(a);
            let content_pref: u32 = if self.children.is_empty() {
                0
            } else if a == orientation {
                let gap_total = self.get_gap_total();
                self.children.iter()
                    .map(|c| c.get_layout().constraints.preferred_size[a] as u32)
                    .sum::<u32>()
                    .saturating_add(gap_total)
            } else {
                self.children.iter()
                    .map(|c| c.get_layout().constraints.preferred_size[a] as u32)
                    .max()
                    .unwrap_or(0)
            };
            content_pref.saturating_add(chrome as u32).min(u16::MAX as u32) as u16
        });
        Constraints { min_size, max_size: Vec2::of(u16::MAX), preferred_size }
    }

    fn layout_position(&mut self) {
        self.clamp_scroll();
        if self.is_wrapping() {
            self.wrap_layout_position();
            return;
        }
        let scroll = self.get_scroll_offset();
        let base = self.get_inner_content_pos() - scroll;
        let orientation = self.orientation;
        let gap = self.gap;

        let cross = orientation.flip();
        let inner = self.get_inner_content_size();
        let cross_size = inner[cross] as i32;
        let place_main = self.align.get_place(orientation);
        let main_mode: FlexAlign = place_main.try_into().unwrap_or(FlexAlign::Start);
        let cross_mode: FlexAlign = self.align.get_place(cross).try_into().unwrap_or(FlexAlign::Start);

        let n = self.children.len() as i32;
        let main_size = inner[orientation] as i32;
        let main_used: i32 = self.children.iter()
            .map(|c| c.get_outer_size()[orientation] as i32)
            .sum::<i32>()
            + (n - 1).max(0) * gap as i32;
        let slack_main = (main_size - main_used).max(0);

        let mut offset = Vec2::of(0i32);
        for (i, child) in self.children.iter_mut().enumerate() {
            let ii = i as i32;
            let pre_gap = match place_main {
                Place::Evenly if n >= 1 => {
                    slack_main * (ii + 1) / (n + 1) - slack_main * ii / (n + 1)
                }
                Place::Apart if n >= 2 => {
                    if i == 0 {
                        0
                    } else {
                        slack_main * ii / (n - 1) - slack_main * (ii - 1) / (n - 1)
                    }
                }
                _ => match main_mode {
                    FlexAlign::Center if i == 0 => slack_main / 2,
                    FlexAlign::End if i == 0 => slack_main,
                    _ => 0,
                },
            };
            offset[orientation] += pre_gap;
            let mut pos = base + offset;
            let child_mode = child.get_layout().align.get_mode(cross).unwrap_or(cross_mode);
            let child_size = child.get_outer_size();
            let slack = (cross_size - child_size[cross] as i32).max(0);
            match child_mode {
                FlexAlign::Start | FlexAlign::Stretch => {}
                FlexAlign::Center => pos[cross] += slack / 2,
                FlexAlign::End => pos[cross] += slack,
            }
            let margin_before = child.get_layout().get_margin_before().map(|v| v as i32);
            child.set_pos(pos + margin_before);
            offset[orientation] += child_size[orientation] as i32 + gap as i32;
            child.layout_position();
        }
    }

    fn render(&self, mut ctx: crate::render::RenderContext) {
        ctx.set_style(self.layout.style);
        ctx.clear();

        if let Some(c) = self.get_chrome() {
            c.render(&mut ctx);
        }

        let border = self.get_border_offset();
        let viewport = self.get_viewport_size();
        let y_sb_gutter = self.get_scrollbar_gutter(Axis2D::Y) as i32;
        let x_sb_gutter = self.get_scrollbar_gutter(Axis2D::X) as i32;

        ctx.move_to(border);
        ctx.set_style(self.layout.style);

        let fract = self
            .scroll
            .as_deref()
            .map(|sc| sc.subcell_scroll)
            .unwrap_or(Vec2::of(0.0));
        let has_subcell = crate::runtime::is_gui() && fract != Vec2::of(0.0);

        if has_subcell {
            #[cfg(feature = "gui")]
            {
                let cell_px = crate::runtime::get_terminal_info().and_then(|i| i.cell_px).unwrap_or(Vec2::of(1));
                let subcell = Axis2D::map(|a| -((fract[a] * cell_px[a] as f32).round() as i32));
                let a = self.orientation;
                let cross = a.flip();
                let mut content_size = viewport;
                let mut content_offset = Vec2::of(0i32);
                if subcell[a] != 0 {
                    content_size[a] = content_size[a].saturating_add(2);
                    content_offset[a] = -1;
                }
                if subcell[cross] != 0 {
                    content_size[cross] = content_size[cross].saturating_add(2);
                    content_offset[cross] = -1;
                }
                ctx.queue_offset_region(
                    self,
                    viewport,
                    content_size,
                    content_offset,
                    subcell,
                    |this: &Self, mut child_ctx| {
                        this.render_children(&mut child_ctx);
                    },
                );
            }
        } else {
            let mut child_ctx = if self.scroll.is_some() {
                ctx.viewport(viewport)
            } else {
                ctx.region(viewport)
            };
            self.render_children(&mut child_ctx);
            drop(child_ctx);
        }

        if let Some(sc) = &self.scroll {
            let thumb = sc.style.get_resolved_thumb();
            let visible = Axis2D::map(|a| sc.scrollbar[a].is_visible());
            let both_visible = visible[Axis2D::X] && visible[Axis2D::Y];
            let (extend, share_corner) = thumb.corner_extension(both_visible);
            let half = Axis2D::map(|a| if extend[a] { 0.5 } else { 0.0 });
            let extra = Axis2D::map(|a| if extend[a] { 1 } else { 0 });
            let gutter = Vec2::new(x_sb_gutter, y_sb_gutter);

            let mut render_axis = |axis: Axis2D| {
                if !visible[axis] {
                    return;
                }
                let inset_before = self.get_inset_before(axis);
                let inset_after = self.get_inset_after(axis);
                let total = viewport[axis] + extra[axis];
                if inset_before + inset_after >= total {
                    return;
                }
                let new_len = total - inset_before - inset_after;
                let cross = axis.flip();
                let mut origin = Vec2::of(0i32);
                origin[axis] = inset_before as i32;
                origin[cross] = viewport[cross] as i32 + gutter[axis] - 1;
                ctx.move_to(border + origin);
                let mut region = Vec2::of(1u16);
                region[axis] = new_len;
                let size = viewport[axis] as f32 + half[axis] - inset_before as f32 - inset_after as f32;

                sc.scrollbar[axis].render_smooth(
                    &mut ctx,
                    self,
                    axis,
                    region,
                    size,
                    &sc.style,
                    move |this: &Self| {
                        let sc = this.scroll.as_ref()?;
                        Some((&sc.style, &sc.scrollbar[axis]))
                    },
                );
            };
            render_axis(Axis2D::Y);
            render_axis(Axis2D::X);

            if share_corner
                && self.get_inset_after(Axis2D::Y) == 0
                && self.get_inset_after(Axis2D::X) == 0
            {
                if let ScrollbarThumb::Border(b) = thumb {
                    let reaches = Axis2D::map(|a| {
                        thumb.has_half_cell(a) && {
                            let view = viewport[a] as f32 + half[a]
                                - self.get_inset_before(a) as f32
                                - self.get_inset_after(a) as f32;
                            sc.scrollbar[a].thumb_reaches_corner_half(view, thumb.get_subpixels(a) as f32)
                        }
                    });
                    let x_in = reaches[Axis2D::X];
                    let y_in = reaches[Axis2D::Y];
                    if x_in || y_in {
                        ctx.move_to(border + Vec2::new(viewport.x as i32 + y_sb_gutter - 1, viewport.y as i32 + x_sb_gutter - 1));
                        ctx.set_style(sc.style.get_resolved().thumb_style);
                        write!(ctx, "{}", b.get_arms(x_in, false, y_in, false));
                    }
                }
            }
        }
    }

    fn find_descendant(
        &self,
        predicate: &dyn Fn(&dyn Widget) -> bool,
        mut path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        for child in self.children.iter() {
            let grandchild = child
                .find_descendant(predicate, path.as_mut().map(|p| &mut **p));
            if grandchild.is_some() {
                if let Some(p) = &mut path {
                    p.push(child.get_id());
                }
                return grandchild;
            }
            if predicate(&**child) {
                if let Some(p) = &mut path {
                    p.push(child.get_id());
                }
                return Some(child.get_id());
            }
        }
        None
    }

    fn on_input(&mut self, queue: &mut InputQueue) -> InputResult {
        let Some(event) = queue.peek() else {
            return InputResult::Rejected;
        };
        match &event.chord {
            chord!(LeftClick) => {
                let result = self.handle_scrollbar_input(&event.chord, event.pos, |sb, a, local, viewport, has_both, inset_before, inset_after| {
                    if !sb.is_visible() {
                        return false;
                    }
                    let start = inset_before as i32;
                    if a == Axis2D::Y {
                        let has_both_extra = if has_both {
                            1
                        } else {
                            0
                        };
                        let y_limit = viewport.y as i32 + has_both_extra - inset_after as i32;
                        local.x >= viewport.x as i32 && local.y >= start && local.y < y_limit
                    } else {
                        let x_limit = viewport.x as i32 - inset_after as i32;
                        local.y >= viewport.y as i32 && local.x >= start && local.x < x_limit
                    }
                });
                if result {
                    queue.next();
                    return InputResult::Handled;
                }
                InputResult::Rejected
            }
            chord!(LeftDrag) | chord!(LeftRelease) => {
                let result = self.handle_scrollbar_input(&event.chord, event.pos, |sb, _, _, _, _, _, _| {
                    sb.is_dragging()
                });
                if result {
                    queue.next();
                    return InputResult::Handled;
                }
                InputResult::Rejected
            }
            chord!(MouseSmoothScroll(direction, delta)) => {
                let direction = *direction;
                let delta = *delta;
                let a = direction.axis();
                if !self.is_scroll_enabled(a) {
                    return InputResult::Rejected;
                }
                if delta == 0.0 {
                    return InputResult::Rejected;
                }
                queue.next();
                let max = self.get_max_scroll(a);
                let sc = self.get_scroll_cfg_mut();
                let old_scroll = sc.scroll[a];
                let old_sub = sc.subcell_scroll[a];
                let cur_total = sc.scroll[a] as f64 + sc.subcell_scroll[a] as f64;
                let new_total = if direction.screen_sign() == Sign::Positive {
                    (cur_total + delta as f64).min(max as f64)
                } else {
                    (cur_total - delta as f64).max(0.0)
                };
                sc.scroll[a] = (new_total as u64).min(max as u64) as u32;
                sc.subcell_scroll[a] = (new_total - sc.scroll[a] as f64) as f32;
                let changed = sc.scroll[a] != old_scroll || sc.subcell_scroll[a] != old_sub;
                if changed {
                    self.dirty_paint();
                    self.sync_scrollbars();
                    tuie::emit(self.get_id(), ScrollEvent);
                }
                InputResult::Handled
            }
            chord!(MouseScroll(direction)) => {
                let a = direction.axis();
                if !self.is_scroll_enabled(a) {
                    return InputResult::Rejected;
                }
                queue.next();
                let max = self.get_max_scroll(a);
                let sc = self.get_scroll_cfg_mut();
                let old_scroll = sc.scroll[a];
                if direction.screen_sign() == Sign::Positive {
                    sc.scroll[a] = (sc.scroll[a] + 1).min(max);
                } else {
                    sc.scroll[a] = sc.scroll[a].saturating_sub(1);
                }
                if sc.scroll[a] != old_scroll {
                    self.dirty_paint();
                    self.sync_scrollbars();
                    tuie::emit(self.get_id(), ScrollEvent);
                }
                InputResult::Handled
            }
            _ => InputResult::Rejected,
        }
    }

    fn can_scroll(&self, direction: Direction2D) -> bool {
        let a = direction.axis();
        if !self.is_scroll_enabled(a) {
            return false;
        }
        let Some(sc) = self.get_scroll_cfg() else {
            return false;
        };
        if direction.screen_sign() == Sign::Positive {
            sc.scroll[a] < self.get_max_scroll(a)
        } else {
            sc.scroll[a] > 0 || sc.subcell_scroll[a] > 0.0
        }
    }

    fn each_child(
        &self,
        f: &mut dyn FnMut(&dyn Widget),
        direction: Sign,
    ) {
        for child in self.children.iter().direction(direction) {
            f(&**child);
        }
    }

    fn each_child_mut(
        &mut self,
        f: &mut dyn FnMut(&mut dyn Widget),
        direction: Sign,
    ) {
        for child in self.children.iter_mut().direction(direction) {
            f(&mut **child);
        }
    }

    fn layout_flow(&mut self, allocated: Vec2<u16>) -> Vec2<u16> {
        if self.is_wrapping() {
            let inner_size = WRAP_POOL.with(|p| {
                let mut scratch = p.acquire();
                let size = self.wrap_resolve(allocated, &mut scratch);
                self.wrap_commit_layout(&scratch);
                let cfg = self.get_wrap_mut();
                cfg.line_starts.clear();
                cfg.line_starts.extend_from_slice(&scratch.line_starts);
                cfg.resolved_size = size;
                size
            });
            self.sync_scrollbars();
            let chrome_overhead = Axis2D::map(|a| self.get_axis_overhead(a));
            return Axis2D::map(|a| inner_size[a].saturating_add(chrome_overhead[a]));
        }
        FLEX_POOL.with(|p| {
            let mut state = p.acquire();
            self.flex_resolve(allocated, &mut state);
            self.commit_layout(&state);
        });
        self.sync_scrollbars();
        self.get_flex_total(get_flow_output_size_layout)
    }

    fn layout_measure(&self, allocated: Vec2<u16>) -> Vec2<u16> {
        if self.is_wrapping() {
            let inner_size = WRAP_POOL.with(|p| {
                let mut scratch = p.acquire();
                let size = self.wrap_resolve(allocated, &mut scratch);
                self.wrap_commit_measure(&scratch);
                size
            });
            let chrome_overhead = Axis2D::map(|a| self.get_axis_overhead(a));
            return Axis2D::map(|a| inner_size[a].saturating_add(chrome_overhead[a]));
        }
        FLEX_POOL.with(|p| {
            let mut state = p.acquire();
            self.flex_resolve(allocated, &mut state);
            self.commit_measure(&state);
        });
        self.get_flex_total(get_flow_output_size_measure)
    }

    fn after_layout(&mut self) {
        if self.resolve_bar_visibility() {
            self.dirty_layout();
        }
    }

    fn reveal(
        &mut self,
        _child: Option<WidgetId>,
        revelation: &mut Revelation,
        scroll_align: Vec2<Option<Align>>,
    ) {
        let border = self.get_border_offset();
        let viewport = self.get_viewport_size();
        let old_scroll = self.get_scroll_val();
        let mut new_scroll = old_scroll;
        let scrolloff = tuie::config::get().scrolloff;

        for a in [Axis2D::X, Axis2D::Y] {
            if !self.is_scroll_enabled(a) {
                continue;
            }
            let inset_before = self.get_inset_before(a) as i32;
            let inset_after = self.get_inset_after(a) as i32;
            let viewport_axis = viewport[a] as i32;
            let safe_viewport = (viewport_axis - inset_before - inset_after).max(0);
            let d = crate::widget::resolve_revelation_axis(
                revelation.get_rects().iter()
                    .map(|r| (r.pos[a] - border[a] - inset_before, r.size[a] as i32)),
                safe_viewport,
                scroll_align[a],
                scrolloff,
            );
            let target = old_scroll[a] as i32 + d;
            let child_size = self.get_padded_content_size()[a].min(i32::MAX as u32) as i32;
            let max_scroll = (child_size - viewport_axis).max(0);
            new_scroll[a] = target.clamp(0, max_scroll) as u32;
        }

        let applied = Axis2D::map(|a| new_scroll[a] as i32 - old_scroll[a] as i32);
        revelation.translate(Vec2 { x: -applied[Axis2D::X], y: -applied[Axis2D::Y] });

        for a in [Axis2D::X, Axis2D::Y] {
            let lo = border[a];
            let hi = lo + viewport[a] as i32;
            revelation.clip_axis(a, lo, hi);
        }

        if new_scroll != old_scroll {
            self.get_scroll_cfg_mut().scroll = new_scroll;
            self.dirty_paint();
            self.sync_scrollbars();
            tuie::emit(self.get_id(), ScrollEvent);
        }
    }

    fn descendant_at_pos(
        &self,
        pos: Vec2<f32>,
        mut path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        if !self.pos_in_viewport(pos) {
            return None;
        }

        let pos = pos - self.subcell_offset();

        for child in self.children.iter() {
            if Self::child_contains(&**child, pos) {
                let descendant = child.descendant_at_pos(
                    pos,
                    path.as_mut().map(|p| &mut **p),
                );
                if let Some(r) = descendant {
                    if let Some(p) = &mut path {
                        p.push(child.get_id());
                    }
                    return Some(r);
                }
                if let Some(p) = &mut path {
                    p.push(child.get_id());
                }
                return Some(child.get_id());
            }
        }
        None
    }

    fn get_focus_target(&self) -> Option<WidgetId> {
        if self.children.len() != 1 {
            return None;
        }
        if !self.is_bordered() {
            return None;
        }
        self.children[0].get_focus_target()
    }

    fn find_descendant_at_pos(
        &self,
        pos: Vec2<f32>,
        predicate: &dyn Fn(&dyn Widget) -> bool,
        mut path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        if !self.pos_in_viewport(pos) {
            return None;
        }

        let pos = pos - self.subcell_offset();

        for child in self.children.iter() {
            if Self::child_contains(&**child, pos) {
                let grandchild = child.find_descendant_at_pos(
                    pos,
                    predicate,
                    path.as_mut().map(|p| &mut **p),
                );
                if grandchild.is_some() {
                    if let Some(p) = &mut path {
                        p.push(child.get_id());
                    }
                    return grandchild;
                } else if predicate(&**child) {
                    if let Some(p) = &mut path {
                        p.push(child.get_id());
                    }
                    return Some(child.get_id());
                }
            }
        }
        None
    }

    fn get_cursor(
        &self,
        selected: Option<WidgetId>,
    ) -> Option<(CursorShape, Vec2<i32>)> {
        let selected = selected?;
        let smooth = self.scroll.as_deref();
        let extra = Axis2D::map(|a| match smooth {
            Some(sc) if sc.subcell_scroll[a] == 0.0 => 0i32,
            _ => 1i32,
        });
        for child in self.children.iter() {
            if let Some((style, pos)) = self.layout.get_child_cursor(&**child, selected) {
                let border = self.get_border_offset();
                let viewport = self.get_viewport_size().map(|v| v as i32);
                if pos.x < border.x - extra.x || pos.x >= border.x + viewport.x + extra.x {
                    return None;
                }
                if pos.y < border.y - extra.y || pos.y >= border.y + viewport.y + extra.y {
                    return None;
                }
                return Some((style, pos));
            }
        }
        None
    }
}

impl Pane {
    /// Creates an empty vertical pane.
    pub fn new() -> Box<Self> {
        Box::new(Self {
            layout: Layout::new(),
            children: Vec::new(),
            orientation: Axis2D::Y,
            align: AlignSpec::default(),
            gap: 0,
            chrome: None,
            scroll: None,
            wrap: None,
            insets: Spacing::new(),
        })
    }

    /// Appends a child widget.
    pub fn add_child(&mut self, widget: Box<dyn Widget>) {
        self.children.push(widget);
        self.dirty_layout();
    }

    /// Appends a fixed-size array of children.
    pub fn children<const N: usize>(
        mut self: Box<Self>,
        children: [Box<dyn Widget>; N],
    ) -> Box<Self> {
        self.children.extend(children);
        self.dirty_layout();
        self
    }

    /// Appends a child widget.
    pub fn child(mut self: Box<Self>, widget: Box<dyn Widget>) -> Box<Self> {
        self.add_child(widget);
        self
    }

    /// Removes the child with the given id and downcasts it to `T`.
    pub fn remove<T: AnyWidget + ?Sized>(&mut self, id: WidgetId<T>) -> Option<Box<T>> {
        if let Some(idx) = self.children.iter().position(|c| c.get_id() == id) {
            let child = self.children.remove(idx);
            self.dirty_layout();
            T::downcast_box(child)
        } else {
            None
        }
    }

    /// Inserts `widget` immediately before the child with id `before`.
    pub fn insert_before(&mut self, before: WidgetId, widget: Box<dyn Widget>) {
        if let Some(idx) = self.children.iter().position(|c| c.get_id() == before) {
            self.children.insert(idx, widget);
            self.dirty_layout();
        }
    }

    /// Inserts `widget` immediately after the child with id `after`.
    pub fn insert_after(&mut self, after: WidgetId, widget: Box<dyn Widget>) {
        if let Some(idx) = self.children.iter().position(|c| c.get_id() == after) {
            self.children.insert(idx + 1, widget);
            self.dirty_layout();
        }
    }

    /// Removes all children.
    pub fn clear(&mut self) {
        self.children.clear();
        self.dirty_layout();
    }

    crate::layout_field! {
        /// The axis children are laid out along.
        orientation: Axis2D
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

    /// Returns `true` when the orientation is [`Axis2D::X`].
    pub fn is_horizontal(&self) -> bool {
        self.orientation == Axis2D::X
    }

    /// Returns `true` when the orientation is [`Axis2D::Y`].
    pub fn is_vertical(&self) -> bool {
        self.orientation == Axis2D::Y
    }

    /// Sets how the pane positions children along the x axis.
    pub fn x_place(mut self: Box<Self>, p: Place) -> Box<Self> {
        self.set_x_place(p);
        self
    }

    /// Sets how the pane positions children along the y axis.
    pub fn y_place(mut self: Box<Self>, p: Place) -> Box<Self> {
        self.set_y_place(p);
        self
    }

    /// Sets how the pane positions children along the x axis.
    pub fn set_x_place(&mut self, p: Place) {
        let new_align = self.align.place_x(p);
        if self.align == new_align {
            return;
        }
        self.align = new_align;
        self.dirty_layout();
    }

    /// Sets how the pane positions children along the y axis.
    pub fn set_y_place(&mut self, p: Place) {
        let new_align = self.align.place_y(p);
        if self.align == new_align {
            return;
        }
        self.align = new_align;
        self.dirty_layout();
    }

    /// Returns how the pane positions children along the x axis.
    pub fn get_x_place(&self) -> Place {
        self.align.get_place_x()
    }

    /// Returns how the pane positions children along the y axis.
    pub fn get_y_place(&self) -> Place {
        self.align.get_place_y()
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

    crate::style_field! {
        /// The edge cells reserved to prevent the scrollbar from drawing over them.
        insets: Spacing => insets
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

    /// Sets the title at the given edge and alignment.
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

    /// Sets or clears the title at the given edge and alignment.
    pub fn set_title_at(
        &mut self,
        edge: VerticalEdge,
        align: Align,
        title: Option<String>,
    ) {
        self.get_chrome_mut().set_title_at(edge, align, title);
        self.dirty_paint();
    }

    /// Sets or clears the top-left title.
    pub fn set_title(&mut self, title: Option<String>) {
        self.set_title_at(VerticalEdge::Top, Align::Start, title);
    }

    /// Returns the top-left title text, if any.
    pub fn get_title(&self) -> Option<&str> {
        self.get_title_at(VerticalEdge::Top, Align::Start)
    }

    /// Returns the title text at the given edge and alignment.
    pub fn get_title_at(&self, edge: VerticalEdge, align: Align) -> Option<&str> {
        self.get_chrome()?.get_title_at(edge, align)
    }

    crate::layout_field! {
        /// The number of cells between children.
        gap: u8
    }

    crate::layout_field! {
        /// Whether overflowing children wrap onto a new line.
        wrap as is_wrapping: bool => wrap?
    }

    crate::layout_field! {
        /// Whether wrapped children are distributed evenly across lines.
        balanced: bool => wrap?.balanced
    }

    crate::layout_field! {
        /// The number of cells between wrapped lines.
        cross_gap: u8 => wrap?.cross_gap
    }

    /// Sets the vertical scrolling mode.
    pub fn y_scroll(mut self: Box<Self>, mode: Scrollbar) -> Box<Self> {
        self.set_y_scroll(Some(mode));
        self
    }

    /// Sets the horizontal scrolling mode.
    pub fn x_scroll(mut self: Box<Self>, mode: Scrollbar) -> Box<Self> {
        self.set_x_scroll(Some(mode));
        self
    }

    /// Sets the optional vertical scrolling mode.
    pub fn y_scroll_opt(mut self: Box<Self>, mode: Option<Scrollbar>) -> Box<Self> {
        self.set_y_scroll(mode);
        self
    }

    /// Sets the optional horizontal scrolling mode.
    pub fn x_scroll_opt(mut self: Box<Self>, mode: Option<Scrollbar>) -> Box<Self> {
        self.set_x_scroll(mode);
        self
    }

    /// Sets the vertical scrolling mode.
    pub fn set_y_scroll(&mut self, mode: Option<Scrollbar>) {
        let sc = self.get_scroll_cfg_mut();
        if sc.mode.y != mode {
            sc.mode.y = mode;
            self.dirty_layout();
        }
    }

    /// Sets the horizontal scrolling mode.
    pub fn set_x_scroll(&mut self, mode: Option<Scrollbar>) {
        let sc = self.get_scroll_cfg_mut();
        if sc.mode.x != mode {
            sc.mode.x = mode;
            self.dirty_layout();
        }
    }

    /// Returns the current vertical scrolling mode.
    pub fn get_y_scroll(&self) -> Option<Scrollbar> {
        self.get_scroll_cfg().and_then(|sc| sc.mode.y)
    }

    /// Returns the current horizontal scrolling mode.
    pub fn get_x_scroll(&self) -> Option<Scrollbar> {
        self.get_scroll_cfg().and_then(|sc| sc.mode.x)
    }

    /// Sets the scrollbar style.
    pub fn set_scrollbar_style(&mut self, style: ScrollbarStyle) {
        self.get_scroll_cfg_mut().style = style;
        self.dirty_paint();
    }

    /// Returns the scrollbar style.
    pub fn get_scrollbar_style(&self) -> ScrollbarStyle {
        self.get_scroll_cfg()
            .map(|sc| sc.style.clone())
            .unwrap_or_else(ScrollbarStyle::new)
    }

    /// Sets the scrollbar style.
    pub fn scrollbar_style(mut self: Box<Self>, style: ScrollbarStyle) -> Box<Self> {
        self.set_scrollbar_style(style);
        self
    }

    /// Returns the scroll position on `axis` as a fraction in `[0.0, 1.0]`.
    pub fn get_scroll_progress(&self, axis: Axis2D) -> f32 {
        if !self.is_scroll_enabled(axis) {
            return 0.0;
        }
        let max = self.get_max_scroll(axis);
        let sc = self.get_scroll_cfg().unwrap();
        ScrollbarState::progress_from_subcell(sc.scroll[axis], sc.subcell_scroll[axis], max)
    }

    /// Sets the scroll position on `axis` from a `[0.0, 1.0]` fraction.
    pub fn set_scroll_progress(&mut self, axis: Axis2D, progress: f32) {
        if !self.is_scroll_enabled(axis) {
            return;
        }
        let max = self.get_max_scroll(axis);
        let (new_scroll, new_subcell) = ScrollbarState::subcell_from_progress(progress, max);
        let sc = self.get_scroll_cfg_mut();
        let changed = new_scroll != sc.scroll[axis]
            || new_subcell != sc.subcell_scroll[axis];
        if changed {
            sc.scroll[axis] = new_scroll;
            sc.subcell_scroll[axis] = new_subcell;
            self.dirty_paint();
            tuie::emit(self.get_id(), ScrollEvent);
        }
    }

    /// Returns the ratio of viewport size to content size on `axis`, clamped to `[0.0, 1.0]`.
    pub fn get_scroll_ratio(&self, axis: Axis2D) -> f32 {
        let vp = self.get_viewport_size()[axis] as f32;
        let content = self.get_padded_content_size()[axis] as f32;
        if content <= 0.0 {
            1.0
        } else {
            (vp / content).min(1.0)
        }
    }

    /// Scrolls the content by `delta` cells along the main axis.
    pub fn scroll_by(&mut self, delta: i32) {
        if delta == 0 {
            return;
        }
        let a = self.orientation;
        if !self.is_scroll_enabled(a) {
            return;
        }
        let max = self.get_max_scroll(a);
        let sc = self.get_scroll_cfg_mut();
        let old_scroll = sc.scroll[a];
        let old_sub = sc.subcell_scroll[a];
        if delta > 0 {
            sc.scroll[a] = (sc.scroll[a] + delta as u32).min(max);
        } else {
            sc.scroll[a] = sc.scroll[a].saturating_sub((-delta) as u32);
        }
        sc.subcell_scroll[a] = 0.0;
        if sc.scroll[a] != old_scroll || sc.subcell_scroll[a] != old_sub {
            self.dirty_paint();
            self.sync_scrollbars();
            tuie::emit(self.get_id(), ScrollEvent);
        }
    }

    /// Returns the inner content area size.
    pub fn get_viewport_size(&self) -> Vec2<u16> {
        let content_size = self.layout.rect.size;
        let borders = self.get_border_cells() * 2;
        Axis2D::map(|a| {
            let gutter = self.get_scrollbar_gutter(a.flip());
            content_size[a].saturating_sub(gutter).saturating_sub(borders)
        })
    }

    /// Returns the total extent of all children.
    pub fn get_content_size(&self) -> Vec2<u32> {
        self.get_children_extent()
    }
}
