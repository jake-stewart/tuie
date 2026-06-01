//! Recursively splittable pane container.

use crate::prelude::*;
use crate::render::border::junction;
use crate::widget::flex::{self, FlexItem};
use crate::widget::{get_flow_output_size_layout, get_flow_output_size_measure};
use chord_macro::chord;
use sign::Directional;
use std::cell::{Cell, RefCell};

type BaseSize = fn(&Layout) -> Vec2<u16>;

thread_local! {
    static EDGE_POOL: RefCell<Vec<Vec<CrossEdge>>> = RefCell::new(Vec::new());
}

/// Leaf pane inside a [`Split`] holding a single widget.
pub struct SplitLeaf {
    widget: Box<dyn Widget>,
    bordered: bool,
    border: Option<&'static Border>,
    border_style: Style,
    title: Option<String>,
    draggable: bool,
}

impl SplitLeaf {
    /// Returns a shared reference to the contained widget.
    pub fn get_widget(&self) -> &dyn Widget {
        &*self.widget
    }

    /// Returns a mutable reference to the contained widget.
    pub fn get_widget_mut(&mut self) -> &mut dyn Widget {
        &mut *self.widget
    }

    /// Sets the border glyph set.
    pub fn set_border(&mut self, border: Option<&'static Border>) {
        self.border = border;
    }

    /// Sets the border style for this leaf.
    pub fn set_border_style(&mut self, style: Style) {
        self.border_style = style;
    }
}

enum SplitNode {
    Leaf(SplitLeaf),
    Container {
        orientation: Axis2D,
        children: Vec<SplitChild>,
        flex: u8,
    },
}

struct SplitChild {
    node: SplitNode,
    size: Cell<Vec2<u16>>,
    dragged: Vec2<Option<u16>>,
}

impl SplitChild {
    fn size_along(&self, axis: Axis2D) -> u16 {
        self.size.get()[axis]
    }

    fn set_size_along(&self, axis: Axis2D, val: u16) {
        let mut s = self.size.get();
        s[axis] = val;
        self.size.set(s);
    }

    fn adjust_size(&self, axis: Axis2D, change: i32) {
        let mut s = self.size.get();
        let cur = s[axis] as i32;
        s[axis] = (cur + change).max(0) as u16;
        self.size.set(s);
    }

    fn commit_dragged_along(&mut self, axis: Axis2D) {
        self.dragged[axis] = Some(self.size.get()[axis]);
    }

    fn all_borderless(&self) -> bool {
        match &self.node {
            SplitNode::Leaf(leaf) => !leaf.bordered,
            SplitNode::Container { children, .. } => {
                children.iter().all(Self::all_borderless)
            }
        }
    }

    fn any_draggable(&self) -> bool {
        match &self.node {
            SplitNode::Leaf(leaf) => leaf.draggable,
            SplitNode::Container { children, .. } => {
                children.iter().any(Self::any_draggable)
            }
        }
    }

    fn effective_min(&self, axis: Axis2D, gap: u16, padding: Spacing, base_size: BaseSize) -> u16 {
        if let SplitNode::Leaf(leaf) = &self.node {
            let layout = leaf.widget.get_layout();
            let mut floor = layout.constraints.min_size[axis];
            if leaf.widget.get_flow_axis() == axis {
                floor = floor.max(base_size(layout)[axis]);
            }
            floor = floor.max(Split::MINIMUM);
            return floor.saturating_add(padding.get_total()[axis]);
        }

        let SplitNode::Container { orientation, children, .. } = &self.node else {
            return Split::MINIMUM;
        };

        if *orientation != axis {
            return children
                .iter()
                .map(|c| c.effective_min(axis, gap, padding, base_size))
                .max()
                .unwrap_or(Split::MINIMUM);
        }

        let mut sum: u16 = 0;
        for c in children.iter() {
            sum = sum.saturating_add(c.effective_min(axis, gap, padding, base_size));
        }
        sum.saturating_add(total_gap(children, gap))
    }

    fn effective_max(&self, axis: Axis2D, gap: u16, padding: Spacing) -> u16 {
        if let SplitNode::Leaf(leaf) = &self.node {
            let cap = leaf.widget.get_layout().constraints.max_size[axis];
            return cap.saturating_add(padding.get_total()[axis]);
        }

        let SplitNode::Container { orientation, children, .. } = &self.node else {
            return u16::MAX;
        };

        if *orientation != axis {
            return children
                .iter()
                .map(|c| c.effective_max(axis, gap, padding))
                .min()
                .unwrap_or(u16::MAX);
        }

        let mut sum: u16 = 0;
        for c in children.iter() {
            sum = sum.saturating_add(c.effective_max(axis, gap, padding));
        }
        sum.saturating_add(total_gap(children, gap))
    }

    fn effective_preferred(&self, axis: Axis2D, gap: u16, padding: Spacing, base_size: BaseSize) -> u16 {
        if let Some(locked) = self.dragged[axis] {
            return locked;
        }

        if let SplitNode::Leaf(leaf) = &self.node {
            let pref = leaf.widget.get_layout().constraints.preferred_size[axis];
            if pref == u16::MAX {
                return u16::MAX;
            }
            return pref.saturating_add(padding.get_total()[axis]);
        }

        let SplitNode::Container { orientation, children, .. } = &self.node else {
            return u16::MAX;
        };

        let same_axis = *orientation == axis;
        let mut had_pref = false;
        let mut combined: u16 = 0;

        for c in children.iter() {
            let raw = c.effective_preferred(axis, gap, padding, base_size);
            let value = if raw == u16::MAX {
                c.effective_min(axis, gap, padding, base_size)
            } else {
                had_pref = true;
                raw
            };
            if same_axis {
                combined = combined.saturating_add(value);
            } else if value > combined {
                combined = value;
            }
        }

        if !had_pref {
            return u16::MAX;
        }
        if same_axis {
            combined.saturating_add(total_gap(children, gap))
        } else {
            combined
        }
    }

    fn effective_flex(&self) -> u8 {
        match &self.node {
            SplitNode::Leaf(leaf) => leaf.widget.get_flex(),
            SplitNode::Container { flex, .. } => *flex,
        }
    }

    fn shrink_room(&self, axis: Axis2D, gap: u16, padding: Spacing, base_size: BaseSize) -> u16 {
        self.size_along(axis)
            .saturating_sub(self.effective_min(axis, gap, padding, base_size))
    }

    fn grow_room(&self, axis: Axis2D, gap: u16, padding: Spacing) -> u16 {
        self.effective_max(axis, gap, padding)
            .saturating_sub(self.size_along(axis))
    }

    fn preferred_or_min(&self, axis: Axis2D, gap: u16, padding: Spacing, base_size: BaseSize) -> u16 {
        let preferred = self.effective_preferred(axis, gap, padding, base_size);
        let min = self.effective_min(axis, gap, padding, base_size);
        if preferred == u16::MAX {
            min
        } else {
            preferred.max(min)
        }
    }

    fn resize_adjust(&self, axis: Axis2D, change: i32, gap: u16, padding: Spacing, base_size: BaseSize) {
        self.adjust_size(axis, change);

        let SplitNode::Container { orientation, children, .. } = &self.node else {
            return;
        };
        let n = children.len();
        if n == 0 {
            return;
        }

        let deltas: Vec<i32> = if *orientation != axis {
            vec![change; n]
        } else {
            let Some(d) = self.same_axis_deltas(axis, gap, padding, base_size) else {
                return;
            };
            d
        };

        for (child, &delta) in children.iter().zip(deltas.iter()) {
            if delta != 0 {
                child.resize_adjust(axis, delta, gap, padding, base_size);
            }
        }
    }

    fn flex_distribute(&self, gap: u16, padding: Spacing, base_size: BaseSize) {
        let SplitNode::Container { orientation, children, .. } = &self.node else {
            return;
        };
        let a = *orientation;
        let ca = a.flip();
        let cell_main = self.size_along(a);
        let cell_cross = self.size_along(ca);
        if children.is_empty() {
            return;
        }

        for child in children.iter() {
            child.set_size_along(ca, cell_cross);
        }

        let divider_space = total_gap(children, gap);

        let mut items: Vec<FlexItem> = Vec::with_capacity(children.len());
        for child in children.iter() {
            let min_main = child.effective_min(a, gap, padding, base_size);
            let max_main = child.effective_max(a, gap, padding);
            let basis = child.preferred_or_min(a, gap, padding, base_size);
            let min_main_eff = min_main.min(max_main);
            items.push(FlexItem::new(basis, min_main_eff, max_main, child.effective_flex()));
        }

        flex::resolve(&mut items, cell_main, divider_space as u32);

        let total_target: i64 = items.iter().map(|i| i.target as i64).sum();
        let limit = (cell_main as i64).saturating_sub(divider_space as i64).max(0);
        if total_target > limit {
            let need = total_target - limit;
            let mut floor_total: i64 = 0;
            let mut rems: Vec<(usize, i64)> = Vec::with_capacity(items.len());
            for (i, item) in items.iter_mut().enumerate() {
                let weight = item.target as i64;
                if weight == 0 {
                    continue;
                }
                let num = weight * need;
                let share = num / total_target;
                item.target = item.target.saturating_sub(share as u16);
                floor_total += share;
                rems.push((i, num - share * total_target));
            }
            let mut leftover = need - floor_total;
            rems.sort_unstable_by(|a, b| b.1.cmp(&a.1));
            for &(idx, _) in &rems {
                if leftover == 0 {
                    break;
                }
                items[idx].target = items[idx].target.saturating_sub(1);
                leftover -= 1;
            }
        }

        for (i, item) in items.iter().enumerate() {
            children[i].set_size_along(a, item.target);
        }

        for child in children.iter() {
            child.flex_distribute(gap, padding, base_size);
        }
    }

    fn needs_initial_layout(&self) -> bool {
        match &self.node {
            SplitNode::Leaf(_) => {
                let s = self.size.get();
                s.x == 0 && s.y == 0
            }
            SplitNode::Container { children, .. } => {
                children.iter().any(Self::needs_initial_layout)
            }
        }
    }

    fn reinforce_mins(&self, gap: u16, padding: Spacing, base_size: BaseSize) {
        let SplitNode::Container { orientation, children, .. } = &self.node else {
            return;
        };
        let a = *orientation;
        let ca = a.flip();
        let cell_cross = self.size_along(ca);

        for child in children.iter() {
            child.set_size_along(ca, cell_cross);
        }

        loop {
            let needy = children.iter().enumerate().find_map(|(i, c)| {
                let deficit =
                    c.effective_min(a, gap, padding, base_size).saturating_sub(c.size_along(a));
                if deficit > 0 {
                    Some((i, deficit as i32))
                } else {
                    None
                }
            });
            let Some((needy_idx, mut deficit)) = needy else {
                break;
            };
            let mut made_progress = false;
            for i in 0..children.len() {
                if i == needy_idx || deficit <= 0 {
                    continue;
                }
                let spare = children[i].shrink_room(a, gap, padding, base_size) as i32;
                if spare > 0 {
                    let give = spare.min(deficit);
                    children[i].resize_adjust(a, -give, gap, padding, base_size);
                    children[needy_idx].resize_adjust(a, give, gap, padding, base_size);
                    deficit -= give;
                    made_progress = true;
                }
            }
            if !made_progress {
                break;
            }
        }

        for child in children.iter() {
            child.reinforce_mins(gap, padding, base_size);
        }
    }

    fn resize_root(&mut self, new_size: Vec2<u16>, gap: u16, padding: Spacing) {
        let base_size: BaseSize = get_flow_output_size_layout;
        if self.needs_initial_layout() {
            self.size.set(new_size);
            self.flex_distribute(gap, padding, base_size);
        } else {
            for axis in [Axis2D::X, Axis2D::Y] {
                let change =
                    new_size[axis] as i32 - self.size_along(axis) as i32;
                if change != 0 {
                    self.resize_adjust(axis, change, gap, padding, base_size);
                    self.relayout_leaves(padding);
                }
            }
            self.relayout_leaves(padding);
            self.reinforce_mins(gap, padding, base_size);
        }
    }

    fn resize_root_measure(&self, new_size: Vec2<u16>, gap: u16, padding: Spacing) {
        let base_size: BaseSize = get_flow_output_size_measure;
        if self.needs_initial_layout() {
            self.size.set(new_size);
            self.flex_distribute(gap, padding, base_size);
        } else {
            for axis in [Axis2D::X, Axis2D::Y] {
                let change =
                    new_size[axis] as i32 - self.size_along(axis) as i32;
                if change != 0 {
                    self.resize_adjust(axis, change, gap, padding, base_size);
                    self.reflow_leaves_measure(padding);
                }
            }
            self.reflow_leaves_measure(padding);
            self.reinforce_mins(gap, padding, base_size);
        }
    }

    fn position_children_mut(&mut self, pos: Vec2<i32>, gap: u16, padding: Spacing) {
        match &mut self.node {
            SplitNode::Leaf(leaf) => {
                let margin_before = leaf.widget.get_layout().get_margin_before().map(|v| v as i32);
                leaf.widget.set_pos(pos + Vec2::new(
                    padding.get_before(Axis2D::X) as i32,
                    padding.get_before(Axis2D::Y) as i32,
                ) + margin_before);
            }
            SplitNode::Container {
                orientation,
                children,
                ..
            } => {
                let a = *orientation;
                let mut offset = pos;
                for i in 0..children.len() {
                    let size_along = children[i].size_along(a);
                    children[i].position_children_mut(offset, gap, padding);
                    let pair_gap = if i + 1 < children.len() {
                        gap_between(&children[i], &children[i + 1], gap)
                    } else {
                        0
                    };
                    offset[a] += size_along as i32 + pair_gap as i32;
                }
            }
        }
    }

    fn try_shrink(
        &mut self,
        axis: Axis2D,
        needed: u16,
        gap: u16,
        padding: Spacing,
        allow_cross_growth: bool,
    ) -> Option<u16> {
        let base_size: BaseSize = get_flow_output_size_layout;
        let room = self.shrink_room(axis, gap, padding, base_size);
        if room == 0 {
            return None;
        }
        let take = room.min(needed);
        self.resize_adjust(axis, -(take as i32), gap, padding, base_size);
        self.relayout_leaves(padding);
        if !allow_cross_growth {
            let cross = axis.flip();
            if self.effective_min(cross, gap, padding, base_size) > self.size_along(cross) {
                self.resize_adjust(axis, take as i32, gap, padding, base_size);
                self.relayout_leaves(padding);
                return None;
            }
        }
        Some(take)
    }

    fn relayout_leaves(&mut self, padding: Spacing) {
        self.set_leaf_sizes_mut(padding);
        self.reflow_leaves_mut(padding);
    }

    fn leaf_allocated(&self, padding: Spacing) -> Vec2<u16> {
        let pad = padding.get_total();
        let s = self.size.get();
        Vec2::new(s.x.saturating_sub(pad.x), s.y.saturating_sub(pad.y))
    }

    fn reflow_leaves_mut(&mut self, padding: Spacing) {
        let allocated = self.leaf_allocated(padding);
        match &mut self.node {
            SplitNode::Leaf(leaf) => {
                flow_child(&mut *leaf.widget, allocated);
            }
            SplitNode::Container { children, .. } => {
                for c in children.iter_mut() {
                    c.reflow_leaves_mut(padding);
                }
            }
        }
    }

    fn reflow_leaves_measure(&self, padding: Spacing) {
        let allocated = self.leaf_allocated(padding);
        match &self.node {
            SplitNode::Leaf(leaf) => {
                flow_child_measure(&*leaf.widget, allocated);
            }
            SplitNode::Container { children, .. } => {
                for c in children {
                    c.reflow_leaves_measure(padding);
                }
            }
        }
    }

    fn split(
        &mut self,
        target: WidgetId,
        new_leaf: SplitLeaf,
        split_axis: Axis2D,
        direction: Sign,
        gap: u16,
    ) -> bool {
        if let SplitNode::Leaf(ref leaf) = self.node {
            if leaf.widget.get_id() == target {
                let parent_size = self.size.get();
                let (existing_size, new_size) = halve_with_gap(parent_size[split_axis], gap);

                let old_node = std::mem::replace(
                    &mut self.node,
                    SplitNode::Container {
                        orientation: split_axis,
                        children: Vec::new(),
                        flex: 1,
                    },
                );

                let existing_child = SplitChild {
                    node: old_node,
                    size: Cell::new(parent_size),
                    dragged: Vec2::of(None),
                };
                existing_child.set_size_along(split_axis, existing_size);
                let new_child = SplitChild {
                    node: SplitNode::Leaf(new_leaf),
                    size: Cell::new(parent_size),
                    dragged: Vec2::of(None),
                };
                new_child.set_size_along(split_axis, new_size);

                let children = if direction.is_positive() {
                    vec![existing_child, new_child]
                } else {
                    vec![new_child, existing_child]
                };
                self.node = SplitNode::Container {
                    orientation: split_axis,
                    children,
                    flex: 1,
                };
                return true;
            }
        }
        self.node.split(target, new_leaf, split_axis, direction, gap)
    }

    fn set_leaf_sizes_mut(&mut self, padding: Spacing) {
        let allocated = self.leaf_allocated(padding);
        match &mut self.node {
            SplitNode::Leaf(leaf) => {
                let margin = leaf.widget.get_layout().get_margin_total();
                leaf.widget.set_rect_size(Axis2D::map(|a| allocated[a].saturating_sub(margin[a])));
            }
            SplitNode::Container { children, .. } => {
                for c in children.iter_mut() {
                    c.set_leaf_sizes_mut(padding);
                }
            }
        }
    }

    fn compute_min_size(&self, gap: u16, padding: Spacing) -> Vec2<u16> {
        Axis2D::map(|a| self.effective_min(a, gap, padding, get_flow_output_size_layout))
    }

    fn clear_dragged(&mut self) {
        self.dragged = Vec2::of(None);
        if let SplitNode::Container { children, .. } = &mut self.node {
            for c in children.iter_mut() {
                c.clear_dragged();
            }
        }
    }

    fn snapshot_sizes(&self, out: &mut Vec<(Vec2<u16>, Vec2<Option<u16>>)>) {
        out.push((self.size.get(), self.dragged));
        if let SplitNode::Container { children, .. } = &self.node {
            for c in children {
                c.snapshot_sizes(out);
            }
        }
    }

    fn restore_sizes(&mut self, snap: &[(Vec2<u16>, Vec2<Option<u16>>)], idx: &mut usize) {
        self.size.set(snap[*idx].0);
        self.dragged = snap[*idx].1;
        *idx += 1;
        if let SplitNode::Container { children, .. } = &mut self.node {
            for c in children.iter_mut() {
                c.restore_sizes(snap, idx);
            }
        }
    }

    fn divider_abs_pos(
        &self,
        path: &[usize],
        divider_idx: usize,
        axis: Axis2D,
        inner_offset: Vec2<i32>,
        gap: u16,
    ) -> Option<i32> {
        let mut offset = inner_offset[axis];
        let mut node = &self.node;
        for &idx in path {
            let SplitNode::Container {
                orientation,
                children,
                ..
            } = node
            else {
                return None;
            };
            let a = *orientation;
            for i in 0..idx {
                if a == axis {
                    let pair_gap = children
                        .get(i + 1)
                        .map(|next| gap_between(&children[i], next, gap))
                        .unwrap_or(0);
                    offset +=
                        children[i].size_along(a) as i32 + pair_gap as i32;
                }
            }
            node = &children[idx].node;
        }
        let SplitNode::Container {
            orientation,
            children,
            ..
        } = node
        else {
            return None;
        };
        debug_assert_eq!(*orientation, axis);
        for i in 0..=divider_idx {
            offset += children[i].size_along(axis) as i32;
            if i < divider_idx {
                let pair_gap = gap_between(&children[i], &children[i + 1], gap);
                offset += pair_gap as i32;
            }
        }
        Some(offset)
    }

    fn collect_edge_styles(
        &self,
        edge_axis: Axis2D,
        at_end: bool,
        offset: i32,
        gap: u16,
        out: &mut Vec<CrossEdge>,
    ) {
        self.node.collect_edge_styles(
            edge_axis,
            at_end,
            offset,
            gap,
            out,
        );
    }

    fn push_to_root(&mut self, new_node: SplitNode, axis: Axis2D, direction: Sign, gap: u16) {
        let total_size = self.size.get();

        let same_orientation = matches!(
            &self.node,
            SplitNode::Container { orientation, .. } if *orientation == axis
        );

        if same_orientation {
            let SplitNode::Container { children, .. } = &mut self.node else {
                unreachable!()
            };
            let sibling_idx = if direction.is_positive() { children.len() - 1 } else { 0 };
            let (sibling_new, new_size) = halve_with_gap(children[sibling_idx].size_along(axis), gap);
            children[sibling_idx].set_size_along(axis, sibling_new);
            let new_child = SplitChild { node: new_node, size: Cell::new(total_size), dragged: Vec2::of(None) };
            new_child.set_size_along(axis, new_size);
            if direction.is_positive() {
                children.push(new_child);
            } else {
                children.insert(0, new_child);
            }
        } else {
            let (old_size, new_size) = halve_with_gap(total_size[axis], gap);

            let old_node = std::mem::replace(&mut self.node, SplitNode::Container {
                orientation: axis,
                children: Vec::new(),
                flex: 1,
            });

            let child_old = SplitChild { node: old_node, size: Cell::new(total_size), dragged: Vec2::of(None) };
            child_old.set_size_along(axis, old_size);
            let child_new = SplitChild { node: new_node, size: Cell::new(total_size), dragged: Vec2::of(None) };
            child_new.set_size_along(axis, new_size);

            let children = if direction.is_positive() {
                vec![child_old, child_new]
            } else {
                vec![child_new, child_old]
            };
            self.node = SplitNode::Container {
                orientation: axis,
                children,
                flex: 1,
            };
        }
    }

    /// Removes child at `idx` and donates its space to a neighbor.
    fn remove_and_donate(
        children: &mut Vec<SplitChild>,
        idx: usize,
        axis: Axis2D,
        gap: u16,
        padding: Spacing,
    ) -> Option<SplitLeaf> {
        let recipient = match (idx, children.len()) {
            (_, 1) => None,
            (0, _) => Some(1),
            (i, _) => Some(i - 1),
        };

        if let Some(rcv) = recipient {
            let (lo, hi) = if idx < rcv { (idx, rcv) } else { (rcv, idx) };
            let bridge = gap_between(&children[lo], &children[hi], gap);
            let donation = children[idx].size_along(axis) as i32 + bridge as i32;
            children[rcv].resize_adjust(axis, donation, gap, padding, get_flow_output_size_layout);
            children[rcv].dragged[axis] = None;
        }

        match children.remove(idx).node {
            SplitNode::Leaf(l) => Some(l),
            _ => None,
        }
    }

    fn same_axis_deltas(
        &self,
        axis: Axis2D,
        gap: u16,
        padding: Spacing,
        base_size: BaseSize,
    ) -> Option<Vec<i32>> {
        let SplitNode::Container { children, .. } = &self.node else {
            return None;
        };
        let parent_size = self.size_along(axis);
        let n = children.len();
        let gap_space = total_gap(children, gap);
        let target = parent_size.saturating_sub(gap_space);
        let current_total: u16 = children.iter().map(|c| c.size_along(axis)).sum();
        let net = target as i32 - current_total as i32;

        if net == 0 {
            return None;
        }

        let growing = net > 0;
        let any_flex = children.iter().any(|c| c.effective_flex() > 0);
        if growing && !any_flex {
            return None;
        }

        let mut deltas = vec![0i32; n];
        let mut budget = net.unsigned_abs() as u16;

        let room_of = |c: &SplitChild, already: u16| -> u16 {
            let cap = if growing {
                c.grow_room(axis, gap, padding)
            } else {
                c.shrink_room(axis, gap, padding, base_size)
            };
            cap.saturating_sub(already)
        };

        for (i, c) in children.iter().enumerate() {
            if budget == 0 {
                break;
            }
            let pref = c.effective_preferred(axis, gap, padding, base_size);
            if pref == u16::MAX {
                continue;
            }
            let cur = c.size_along(axis);
            let toward_pref = if growing {
                pref.saturating_sub(cur)
            } else {
                cur.saturating_sub(pref)
            };
            if toward_pref == 0 {
                continue;
            }
            let take = budget.min(toward_pref).min(room_of(c, 0));
            if take == 0 {
                continue;
            }
            deltas[i] = if growing { take as i32 } else { -(take as i32) };
            budget -= take;
        }

        while budget > 0 {
            let mut weights = Vec::with_capacity(n);
            let mut rooms = Vec::with_capacity(n);
            let mut weight_sum: u32 = 0;

            for (i, c) in children.iter().enumerate() {
                let already = deltas[i].abs() as u16;
                let room = room_of(c, already);
                let flex = c.effective_flex();
                let w: u32 = if room == 0 {
                    0
                } else if growing {
                    if any_flex && flex == 0 {
                        0
                    } else {
                        flex.max(1) as u32
                    }
                } else {
                    (c.size_along(axis) as u32).max(1)
                };
                weights.push(w);
                rooms.push(room);
                weight_sum = weight_sum.saturating_add(w);
            }

            if weight_sum == 0 {
                break;
            }

            let amount = budget as u64;
            let mut shares = vec![0u64; n];
            let mut placed: u64 = 0;
            let mut remainders: Vec<(usize, u64)> = Vec::with_capacity(n);

            for i in 0..n {
                if weights[i] == 0 {
                    continue;
                }
                let numerator = weights[i] as u64 * amount;
                let floor = numerator / weight_sum as u64;
                let capped = floor.min(rooms[i] as u64);
                shares[i] = capped;
                placed += capped;
                if capped == floor {
                    remainders.push((i, numerator - floor * weight_sum as u64));
                }
            }

            let mut spare = amount - placed;
            remainders.sort_unstable_by(|a, b| b.1.cmp(&a.1));
            for &(idx, _) in &remainders {
                if spare == 0 {
                    break;
                }
                if shares[idx] < rooms[idx] as u64 {
                    shares[idx] += 1;
                    spare -= 1;
                }
            }
            if spare > 0 {
                for i in 0..n {
                    if spare == 0 {
                        break;
                    }
                    let cap = rooms[i] as u64;
                    if shares[i] < cap {
                        let give = (cap - shares[i]).min(spare);
                        shares[i] += give;
                        spare -= give;
                    }
                }
            }

            let mut consumed: u64 = 0;
            for i in 0..n {
                if shares[i] == 0 {
                    continue;
                }
                let s = shares[i] as i32;
                deltas[i] += if growing { s } else { -s };
                consumed += shares[i];
            }
            if consumed == 0 {
                break;
            }
            budget -= consumed as u16;
        }

        if budget > 0 && !growing {
            let total: u64 = children.iter().map(|c| c.size_along(axis) as u64).sum();
            if total > 0 {
                let amount = budget as u64;
                let mut placed: u64 = 0;
                let mut rems: Vec<(usize, u64)> = Vec::with_capacity(n);
                for (i, c) in children.iter().enumerate() {
                    let cur = c.size_along(axis) as u64;
                    let num = cur * amount;
                    let floor = num / total;
                    deltas[i] -= floor as i32;
                    placed += floor;
                    rems.push((i, num - floor * total));
                }
                let mut spare = amount - placed;
                rems.sort_unstable_by(|a, b| b.1.cmp(&a.1));
                for &(idx, _) in &rems {
                    if spare == 0 {
                        break;
                    }
                    deltas[idx] -= 1;
                    spare -= 1;
                }
            }
        }

        Some(deltas)
    }

    /// Grows the pane below `divider_idx` by shrinking a pane above it.
    fn grow_at_divider(
        children: &mut Vec<SplitChild>,
        divider_idx: usize,
        axis: Axis2D,
        needed: u16,
        gap: u16,
        padding: Spacing,
        allow_cross_growth: bool,
    ) -> u16 {
        let Some((grow_idx, grow_room)) = (0..=divider_idx).rev()
            .map(|i| (i, children[i].grow_room(axis, gap, padding)))
            .find(|&(_, room)| room > 0)
        else {
            return 0;
        };
        let needed_capped = needed.min(grow_room);
        if needed_capped == 0 {
            return 0;
        }

        for idx in divider_idx + 1..children.len() {
            if let Some(take) = children[idx].try_shrink(axis, needed_capped, gap, padding, allow_cross_growth) {
                children[grow_idx].resize_adjust(axis, take as i32, gap, padding, get_flow_output_size_layout);
                return take;
            }
        }
        0
    }

    /// Grows the pane above `divider_idx` by shrinking a pane below it.
    fn shrink_at_divider(
        children: &mut Vec<SplitChild>,
        divider_idx: usize,
        axis: Axis2D,
        needed: u16,
        gap: u16,
        padding: Spacing,
        allow_cross_growth: bool,
    ) -> u16 {
        let Some((add_idx, grow_room)) = (divider_idx + 1..children.len())
            .map(|i| (i, children[i].grow_room(axis, gap, padding)))
            .find(|&(_, room)| room > 0)
        else {
            return 0;
        };
        let needed_capped = needed.min(grow_room);
        if needed_capped == 0 {
            return 0;
        }

        for idx in (0..=divider_idx).rev() {
            if let Some(take) = children[idx].try_shrink(axis, needed_capped, gap, padding, allow_cross_growth) {
                children[add_idx].resize_adjust(axis, take as i32, gap, padding, get_flow_output_size_layout);
                return take;
            }
        }
        0
    }

    /// Returns whether `pos` hits the boundary between this child and `next_child`.
    fn hits_boundary(
        &self,
        child_offset: Vec2<i32>,
        next_child: Option<&SplitChild>,
        axis: Axis2D,
        pair_gap: u16,
        pos: Vec2<f32>,
    ) -> bool {
        let ca = axis.flip();
        let boundary = child_offset[axis] + self.size_along(axis) as i32;
        let cross_start = child_offset[ca];
        let cross_end = cross_start + self.size_along(ca) as i32;
        let pos_along = pos[axis].floor() as i32;
        let pos_cross = pos[ca].floor() as i32;
        let along_hit = if pair_gap > 0 {
            pos_along == boundary
        } else {
            let left_draggable = self.any_draggable();
            let right_draggable = next_child.is_some_and(|c| c.any_draggable());
            match (left_draggable, right_draggable) {
                (true, true) => pos_along >= boundary - 1 && pos_along <= boundary,
                (true, false) => pos_along == boundary - 1,
                (false, true) => pos_along == boundary,
                (false, false) => false,
            }
        };
        along_hit && pos_cross >= cross_start - 1 && pos_cross <= cross_end
    }

    fn move_divider(
        children: &mut Vec<SplitChild>,
        divider_idx: usize,
        axis: Axis2D,
        change: i32,
        gap: u16,
        padding: Spacing,
        allow_cross_growth: bool,
    ) -> u16 {
        let total = change.unsigned_abs() as u16;
        let mut remaining = total;
        while remaining > 0 {
            let taken = if change > 0 {
                Self::grow_at_divider(children, divider_idx, axis, remaining, gap, padding, allow_cross_growth)
            } else {
                Self::shrink_at_divider(children, divider_idx, axis, remaining, gap, padding, allow_cross_growth)
            };
            remaining -= taken;
            if taken == 0 {
                break;
            }
        }
        total - remaining
    }

    fn move_divider_propagate(
        &mut self,
        path: &[usize],
        divider_idx: usize,
        axis: Axis2D,
        delta: i32,
        gap: u16,
        padding: Spacing,
        allow_cross_growth: bool,
    ) {
        if delta == 0 {
            return;
        }

        let moved = {
            let Some(children) = self.node.find_container_at_path(path) else {
                return;
            };
            let m = Self::move_divider(children, divider_idx, axis, delta, gap, padding, allow_cross_growth);
            for c in children.iter_mut() {
                c.commit_dragged_along(axis);
            }
            m
        };
        let mut remaining = delta.unsigned_abs() as u16 - moved;
        if remaining == 0 {
            return;
        }

        for level in (0..path.len()).rev() {
            if remaining == 0 {
                break;
            }

            let orientation = {
                let mut node = &self.node;
                let mut found = None;
                for (depth, &idx) in path.iter().enumerate() {
                    let SplitNode::Container { orientation, children, .. } = node else {
                        break;
                    };
                    if depth == level {
                        found = Some(*orientation);
                        break;
                    }
                    node = &children[idx].node;
                }
                found
            };
            if orientation != Some(axis) {
                continue;
            }

            let parent_path = &path[..level];
            let ci = path[level];

            let taken = {
                let Some(parent) = self.node.find_container_at_path(parent_path) else {
                    continue;
                };
                let mut total = 0u16;
                if delta > 0 {
                    for idx in ci + 1..parent.len() {
                        let room = parent[idx].shrink_room(axis, gap, padding, get_flow_output_size_layout);
                        if room == 0 {
                            continue;
                        }
                        let take = room.min(remaining - total);
                        parent[idx].resize_adjust(axis, -(take as i32), gap, padding, get_flow_output_size_layout);
                        parent[ci].resize_adjust(axis, take as i32, gap, padding, get_flow_output_size_layout);
                        total += take;
                        if total >= remaining {
                            break;
                        }
                    }
                } else {
                    for idx in (0..ci).rev() {
                        let room = parent[idx].shrink_room(axis, gap, padding, get_flow_output_size_layout);
                        if room == 0 {
                            continue;
                        }
                        let take = room.min(remaining - total);
                        parent[idx].resize_adjust(axis, -(take as i32), gap, padding, get_flow_output_size_layout);
                        parent[ci].resize_adjust(axis, take as i32, gap, padding, get_flow_output_size_layout);
                        total += take;
                        if total >= remaining {
                            break;
                        }
                    }
                }
                total
            };

            if taken == 0 {
                continue;
            }

            if let Some(parent) = self.node.find_container_at_path(parent_path) {
                for c in parent.iter_mut() {
                    c.commit_dragged_along(axis);
                }
            }

            let placed = {
                let Some(children) = self.node.find_container_at_path(path) else {
                    break;
                };
                let p = Self::move_divider(
                    children,
                    divider_idx,
                    axis,
                    delta.signum() * remaining as i32,
                    gap,
                    padding,
                    allow_cross_growth,
                );
                for c in children.iter_mut() {
                    c.commit_dragged_along(axis);
                }
                p
            };
            remaining = remaining.saturating_sub(placed);
        }
    }
}

impl SplitNode {
    fn is_single_leaf(&self) -> bool {
        match self {
            SplitNode::Leaf(_) => true,
            SplitNode::Container { children, .. } => {
                children.len() == 1 && children[0].node.is_single_leaf()
            }
        }
    }

    fn contains(&self, target: WidgetId) -> bool {
        match self {
            SplitNode::Leaf(leaf) => leaf.widget.get_id() == target,
            SplitNode::Container { children, .. } => {
                children.iter().any(|c| c.node.contains(target))
            }
        }
    }

    fn for_each_leaf(
        &self,
        f: &mut dyn FnMut(&dyn Widget),
        direction: Sign,
    ) {
        match self {
            SplitNode::Leaf(leaf) => f(&*leaf.widget),
            SplitNode::Container { children, .. } => {
                for child in children.iter().direction(direction) {
                    child.node.for_each_leaf(f, direction);
                }
            }
        }
    }

    fn for_each_leaf_mut(
        &mut self,
        f: &mut dyn FnMut(&mut dyn Widget),
        direction: Sign,
    ) {
        match self {
            SplitNode::Leaf(leaf) => f(&mut *leaf.widget),
            SplitNode::Container { children, .. } => {
                for child in children.iter_mut().direction(direction) {
                    child.node.for_each_leaf_mut(f, direction);
                }
            }
        }
    }

    fn leaf_mut(&mut self, target: WidgetId) -> Option<&mut SplitLeaf> {
        match self {
            SplitNode::Leaf(leaf) => {
                if leaf.widget.get_id() == target {
                    Some(leaf)
                } else {
                    None
                }
            }
            SplitNode::Container { children, .. } => {
                for child in children.iter_mut() {
                    if let Some(leaf) = child.node.leaf_mut(target) {
                        return Some(leaf);
                    }
                }
                None
            }
        }
    }

    fn find_leaf<T>(
        &self,
        f: &mut dyn FnMut(&dyn Widget) -> Option<T>,
    ) -> Option<T> {
        match self {
            SplitNode::Leaf(leaf) => f(&*leaf.widget),
            SplitNode::Container { children, .. } => {
                for child in children {
                    if let Some(result) = child.node.find_leaf(f) {
                        return Some(result);
                    }
                }
                None
            }
        }
    }

    fn reposition_leaves_mut(&mut self) {
        match self {
            SplitNode::Leaf(leaf) => leaf.widget.layout_position(),
            SplitNode::Container { children, .. } => {
                for child in children.iter_mut() {
                    child.node.reposition_leaves_mut();
                }
            }
        }
    }

    fn collect_dividers(
        &self,
        offset: Vec2<i32>,
        gap: u16,
        out: &mut Vec<Divider>,
    ) {
        #[derive(Default)]
        struct DividerBufs {
            child_edges: Vec<CrossEdge>,
            next_edges: Vec<CrossEdge>,
            cuts: Vec<i32>,
        }
        thread_local! {
            static DIVIDER_BUFS: RefCell<DividerBufs> = RefCell::new(DividerBufs {
                child_edges: Vec::new(),
                next_edges: Vec::new(),
                cuts: Vec::new(),
            });
        }

        let SplitNode::Container {
            orientation,
            children,
            ..
        } = self
        else {
            return;
        };
        let a = *orientation;
        let ca = a.flip();
        let mut child_offset = offset;

        for (i, child) in children.iter().enumerate() {
            let pair_gap = children
                .get(i + 1)
                .map(|next| gap_between(child, next, gap))
                .unwrap_or(0);
            if pair_gap > 0 {
                let divider_pos = child_offset[a] + child.size_along(a) as i32;
                let cross_start = child_offset[ca];
                let cross_end = cross_start + child.size_along(ca) as i32;

                let next = &children[i + 1];

                DIVIDER_BUFS.with(|cell| {
                    let mut bufs = cell.take();
                    bufs.child_edges.clear();
                    bufs.next_edges.clear();

                    child.collect_edge_styles(
                        ca,
                        true,
                        cross_start,
                        gap,
                        &mut bufs.child_edges,
                    );
                    next.collect_edge_styles(
                        ca,
                        false,
                        cross_start,
                        gap,
                        &mut bufs.next_edges,
                    );

                    let div_idx = out.len();
                    let edges = EDGE_POOL.with(|cell| {
                        cell.borrow_mut().pop().unwrap_or_default()
                    });
                    out.push(Divider {
                        axis: a,
                        pos: divider_pos,
                        start: cross_start,
                        end: cross_end,
                        edges,
                    });
                    merge_cross_edges(
                        &bufs.child_edges,
                        &bufs.next_edges,
                        &mut out[div_idx].edges,
                        &mut bufs.cuts,
                    );

                    cell.replace(bufs);
                });
            }

            child.node.collect_dividers(
                child_offset,
                gap,
                out,
            );

            child_offset[a] += child.size_along(a) as i32 + pair_gap as i32;
        }
    }

    fn render_leaves(
        &self,
        offset: Vec2<i32>,
        gap: u16,
        padding: Spacing,
        ctx: &mut crate::render::RenderContext,
    ) {
        match self {
            SplitNode::Leaf(leaf) => {
                ctx.render_child(&*leaf.widget, offset + Vec2::new(
                    padding.get_before(Axis2D::X) as i32,
                    padding.get_before(Axis2D::Y) as i32,
                ));
            }
            SplitNode::Container {
                orientation,
                children,
                ..
            } => {
                let a = *orientation;
                let mut child_offset = offset;
                for (i, child) in children.iter().enumerate() {
                    child.node.render_leaves(child_offset, gap, padding, ctx);
                    let pair_gap = children
                        .get(i + 1)
                        .map(|next| gap_between(child, next, gap))
                        .unwrap_or(0);
                    child_offset[a] +=
                        child.size_along(a) as i32 + pair_gap as i32;
                }
            }
        }
    }

    fn render_titles(
        &self,
        offset: Vec2<i32>,
        gap: u16,
        base_style: Style,
        ctx: &mut crate::render::RenderContext,
    ) {
        let SplitNode::Container {
            orientation,
            children,
            ..
        } = self
        else {
            return;
        };
        let a = *orientation;
        let mut child_offset = offset;

        for (i, child) in children.iter().enumerate() {
            if let SplitNode::Leaf(leaf) = &child.node {
                if let Some(title) = &leaf.title {
                    let has_row_above = i == 0
                        || a != Axis2D::Y
                        || gap_between(&children[i - 1], child, gap) > 0;
                    if !title.is_empty() && has_row_above {
                        let title_y = child_offset.y - 1;
                        let title_x = child_offset.x;
                        let available = child.size.get()[Axis2D::X] as usize;
                        if title_y >= 0 && available >= 4 {
                            ctx.set_style(base_style.apply(leaf.border_style));
                            ctx.move_to(Vec2::new(title_x + 1, title_y));
                            let mut region = ctx.region(Vec2::new((available - 1) as u16, 1));
                            write!(region, " {} ", title);
                        }
                    }
                }
            }

            child.node.render_titles(
                child_offset,
                gap,
                base_style,
                ctx,
            );
            let pair_gap = children
                .get(i + 1)
                .map(|next| gap_between(child, next, gap))
                .unwrap_or(0);
            child_offset[a] += child.size_along(a) as i32 + pair_gap as i32;
        }
    }

    fn collect_edge_styles(
        &self,
        edge_axis: Axis2D,
        at_end: bool,
        offset: i32,
        gap: u16,
        out: &mut Vec<CrossEdge>,
    ) {
        match self {
            SplitNode::Leaf(leaf) => {
                let size = leaf.widget.get_outer_size();
                let end = offset + size[edge_axis] as i32;
                if end > offset {
                    out.push(CrossEdge {
                        start: offset,
                        end,
                        style: leaf.border_style,
                        border: leaf.border,
                    });
                }
            }
            SplitNode::Container {
                orientation,
                children,
                ..
            } if *orientation == edge_axis => {
                let mut pos = offset;
                for (i, child) in children.iter().enumerate() {
                    child.collect_edge_styles(
                        edge_axis,
                        at_end,
                        pos,
                        gap,
                        out,
                    );
                    let pair_gap = children
                        .get(i + 1)
                        .map(|next| gap_between(child, next, gap))
                        .unwrap_or(0);
                    pos += child.size_along(edge_axis) as i32 + pair_gap as i32;
                }
            }
            SplitNode::Container { children, .. } => {
                let child = if at_end {
                    children.last()
                } else {
                    children.first()
                };
                if let Some(child) = child {
                    child.collect_edge_styles(
                        edge_axis,
                        at_end,
                        offset,
                        gap,
                        out,
                    );
                }
            }
        }
    }

    fn find_container_at_path<'a>(
        self: &'a mut SplitNode,
        path: &[usize],
    ) -> Option<&'a mut Vec<SplitChild>> {
        match self {
            SplitNode::Container { children, .. } => {
                if path.is_empty() {
                    return Some(children);
                }
                children[path[0]].node.find_container_at_path(&path[1..])
            }
            SplitNode::Leaf(_) => None,
        }
    }

    fn find_all_dividers_at_pos(
        &self,
        offset: Vec2<i32>,
        axis: Axis2D,
        mouse: Vec2<i32>,
        gap: u16,
        path: &mut Vec<usize>,
        out: &mut Vec<DragDivider>,
    ) {
        let SplitNode::Container {
            orientation,
            children,
            ..
        } = self
        else {
            return;
        };
        let a = *orientation;
        let mut child_offset = offset;

        for (i, child) in children.iter().enumerate() {
            let pair_gap = children
                .get(i + 1)
                .map(|next| gap_between(child, next, gap))
                .unwrap_or(0);

            if a == axis && i + 1 < children.len() {
                let mouse_f = Axis2D::map(|ax| mouse[ax] as f32 + 0.5);
                if child.hits_boundary(child_offset, children.get(i + 1), a, pair_gap, mouse_f) {
                    let boundary = child_offset[a] + child.size_along(a) as i32;
                    out.push(DragDivider {
                        container_path: path.clone(),
                        divider_idx: i,
                        axis,
                        grab_offset: mouse[a] - boundary,
                    });
                }
            }

            path.push(i);
            child.node.find_all_dividers_at_pos(
                child_offset,
                axis,
                mouse,
                gap,
                path,
                out,
            );
            path.pop();

            child_offset[a] += child.size_along(a) as i32 + pair_gap as i32;
        }
    }

    fn find_descendant_at_pos(
        &self,
        pos: Vec2<f32>,
        offset: Vec2<i32>,
        gap: u16,
        mut path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        match self {
            SplitNode::Leaf(leaf) => {
                let w = &leaf.widget;
                let slot_pos = w.get_pos();
                let slot_size = w.get_rect_size().map(|v| v as i32);
                if Axis2D::all(|a| {
                    pos[a] >= slot_pos[a] as f32 && pos[a] < (slot_pos[a] + slot_size[a]) as f32
                }) {
                    if let Some(r) = w.descendant_at_pos(
                        pos,
                        path.as_mut().map(|p| &mut **p),
                    ) {
                        if let Some(p) = &mut path {
                            p.push(w.get_id());
                        }
                        return Some(r);
                    }
                    if let Some(p) = &mut path {
                        p.push(w.get_id());
                    }
                    return Some(w.get_id());
                }
                None
            }
            SplitNode::Container {
                orientation,
                children,
                ..
            } => {
                let a = *orientation;
                if self.is_on_borderless_boundary(offset, pos, gap) {
                    return None;
                }
                let mut child_offset = offset;
                for (i, child) in children.iter().enumerate() {
                    if let Some(r) = child.node.find_descendant_at_pos(
                        pos,
                        child_offset,
                        gap,
                        path.as_mut().map(|p| &mut **p),
                    ) {
                        return Some(r);
                    }
                    let pair_gap = children
                        .get(i + 1)
                        .map(|next| gap_between(child, next, gap))
                        .unwrap_or(0);
                    child_offset[a] +=
                        child.size_along(a) as i32 + pair_gap as i32;
                }
                None
            }
        }
    }

    fn find_matching_descendant_at_pos(
        &self,
        pos: Vec2<f32>,
        predicate: &dyn Fn(&dyn Widget) -> bool,
        offset: Vec2<i32>,
        gap: u16,
        mut path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        match self {
            SplitNode::Leaf(leaf) => {
                let w = &leaf.widget;
                let slot_pos = w.get_pos();
                let slot_size = w.get_rect_size().map(|v| v as i32);
                if Axis2D::all(|a| {
                    pos[a] >= slot_pos[a] as f32 && pos[a] < (slot_pos[a] + slot_size[a]) as f32
                }) {
                    let grandchild = w.find_descendant_at_pos(
                        pos,
                        predicate,
                        path.as_mut().map(|p| &mut **p),
                    );
                    if grandchild.is_some() {
                        if let Some(p) = &mut path {
                            p.push(w.get_id());
                        }
                        return grandchild;
                    }
                    if predicate(&**w) {
                        if let Some(p) = &mut path {
                            p.push(w.get_id());
                        }
                        return Some(w.get_id());
                    }
                }
                None
            }
            SplitNode::Container {
                orientation,
                children,
                ..
            } => {
                let a = *orientation;
                if self.is_on_borderless_boundary(offset, pos, gap) {
                    return None;
                }
                let mut child_offset = offset;
                for (i, child) in children.iter().enumerate() {
                    if let Some(r) = child.node.find_matching_descendant_at_pos(
                        pos,
                        predicate,
                        child_offset,
                        gap,
                        path.as_mut().map(|p| &mut **p),
                    ) {
                        return Some(r);
                    }
                    let pair_gap = children
                        .get(i + 1)
                        .map(|next| gap_between(child, next, gap))
                        .unwrap_or(0);
                    child_offset[a] +=
                        child.size_along(a) as i32 + pair_gap as i32;
                }
                None
            }
        }
    }

    fn split(
        &mut self,
        target: WidgetId,
        new_leaf: SplitLeaf,
        split_axis: Axis2D,
        direction: Sign,
        gap: u16,
    ) -> bool {
        let SplitNode::Container {
            orientation,
            children,
            ..
        } = self
        else {
            return false;
        };

        let mut target_idx = None;
        for (i, child) in children.iter().enumerate() {
            if let SplitNode::Leaf(leaf) = &child.node {
                if leaf.widget.get_id() == target {
                    target_idx = Some(i);
                    break;
                }
            }
        }

        if let Some(idx) = target_idx {
            if *orientation == split_axis {
                let target_size = children[idx].size.get();
                let (existing_size, new_size) = halve_with_gap(target_size[split_axis], gap);

                children[idx].set_size_along(split_axis, existing_size);
                children[idx].dragged[split_axis] = None;

                let new_child = SplitChild {
                    node: SplitNode::Leaf(new_leaf),
                    size: Cell::new(target_size),
                    dragged: Vec2::of(None),
                };
                new_child.set_size_along(split_axis, new_size);

                let insert_idx = if direction.is_positive() { idx + 1 } else { idx };
                children.insert(insert_idx, new_child);
            } else {
                let target_size = children[idx].size.get();
                let (existing_size, new_size) = halve_with_gap(target_size[split_axis], gap);

                let old_child = children.remove(idx);

                let existing_child = SplitChild {
                    node: old_child.node,
                    size: Cell::new(target_size),
                    dragged: Vec2::of(None),
                };
                existing_child.set_size_along(split_axis, existing_size);

                let new_child = SplitChild {
                    node: SplitNode::Leaf(new_leaf),
                    size: Cell::new(target_size),
                    dragged: Vec2::of(None),
                };
                new_child.set_size_along(split_axis, new_size);

                let inner_children = if direction.is_positive() {
                    vec![existing_child, new_child]
                } else {
                    vec![new_child, existing_child]
                };

                let new_container_child = SplitChild {
                    node: SplitNode::Container {
                        orientation: split_axis,
                        children: inner_children,
                        flex: 1,
                    },
                    size: Cell::new(target_size),
                    dragged: Vec2::of(None),
                };

                children.insert(idx, new_container_child);
            }
            return true;
        }

        let recurse_idx = children.iter().position(|child| {
            child.node.contains(target)
        });
        if let Some(idx) = recurse_idx {
            return children[idx].node.split(
                target,
                new_leaf,
                split_axis,
                direction,
                gap,
            );
        }
        false
    }

    fn close_inner(&mut self, target: WidgetId, gap: u16, padding: Spacing) -> Option<SplitLeaf> {
        let SplitNode::Container { orientation, children, .. } = self else {
            return None;
        };
        let a = *orientation;

        let local_hit = children.iter().position(|c| {
            matches!(&c.node, SplitNode::Leaf(leaf) if leaf.widget.get_id() == target)
        });

        if let Some(idx) = local_hit {
            let leaf = SplitChild::remove_and_donate(children, idx, a, gap, padding);
            self.collapse_singleton();
            return leaf;
        }

        for child in children.iter_mut() {
            let leaf = child.node.close_inner(target, gap, padding);
            if leaf.is_none() {
                continue;
            }
            child.node.collapse_singleton();
            return leaf;
        }
        None
    }

    fn set_container_flex_of(&mut self, target: WidgetId, flex: u8) -> bool {
        let SplitNode::Container { children, .. } = self else {
            return false;
        };
        for child in children.iter_mut() {
            if child.node.contains(target) {
                if let SplitNode::Container { flex: child_flex, .. } = &mut child.node {
                    if *child_flex == flex {
                        return false;
                    }
                    *child_flex = flex;
                    return true;
                }
                return false;
            }
        }
        false
    }

    /// Replaces a single-child container with its child.
    fn collapse_singleton(&mut self) {
        let SplitNode::Container { children, .. } = self else {
            return;
        };
        if children.len() != 1 {
            return;
        }
        let only = children.remove(0);
        *self = only.node;
    }

    /// Returns whether `pos` lands on a zero-gap boundary between two children.
    fn is_on_borderless_boundary(
        &self,
        offset: Vec2<i32>,
        pos: Vec2<f32>,
        gap: u16,
    ) -> bool {
        let SplitNode::Container { orientation, children, .. } = self else {
            return false;
        };
        let axis = *orientation;
        let mut child_offset = offset;
        for (i, child) in children.iter().enumerate() {
            let pair_gap = children
                .get(i + 1)
                .map(|next| gap_between(child, next, gap))
                .unwrap_or(0);
            if pair_gap == 0
                && i + 1 < children.len()
                && child.hits_boundary(child_offset, children.get(i + 1), axis, 0, pos)
            {
                return true;
            }
            child_offset[axis] += child.size_along(axis) as i32 + pair_gap as i32;
        }
        false
    }
}

/// Builder for one entry in a [`SplitPane`].
pub struct SplitPaneChild {
    bordered: bool,
    border: Option<&'static Border>,
    border_style: Style,
    title: Option<String>,
    draggable: bool,
    content: SplitPaneContent,
}

/// Content payload of a [`SplitPaneChild`].
pub enum SplitPaneContent {
    /// A single widget leaf.
    Widget(Box<dyn Widget>),
    /// A nested split container.
    Split {
        orientation: Axis2D,
        flex: u8,
        children: Vec<SplitPaneChild>,
    },
}

impl SplitPaneChild {
    fn into_leaf_data(self) -> SplitLeaf {
        match self.content {
            SplitPaneContent::Widget(w) => SplitLeaf {
                widget: w,
                bordered: self.bordered,
                border: self.border,
                border_style: self.border_style,
                title: self.title,
                draggable: self.draggable,
            },
            SplitPaneContent::Split { .. } => {
                panic!("cannot use a nested split as a leaf");
            }
        }
    }

    fn from_leaf_data(leaf: SplitLeaf) -> Self {
        SplitPaneChild {
            bordered: leaf.bordered,
            border: leaf.border,
            border_style: leaf.border_style,
            title: leaf.title,
            draggable: leaf.draggable,
            content: SplitPaneContent::Widget(leaf.widget),
        }
    }

    #[track_caller]
    fn build(self) -> SplitChild {
        match self.content {
            SplitPaneContent::Widget(w) => {
                SplitChild {
                    node: SplitNode::Leaf(SplitLeaf {
                        widget: w,
                        bordered: self.bordered,
                        border: self.border,
                        border_style: self.border_style,
                        title: self.title,
                        draggable: self.draggable,
                    }),
                    size: Cell::new(Vec2::of(0)),
                    dragged: Vec2::of(None),
                }
            }
            SplitPaneContent::Split {
                orientation,
                flex,
                children,
            } => {
                let built_children: Vec<SplitChild> =
                    children.into_iter().map(SplitPaneChild::build).collect();
                SplitChild {
                    node: SplitNode::Container {
                        orientation,
                        children: built_children,
                        flex,
                    },
                    size: Cell::new(Vec2::of(0)),
                    dragged: Vec2::of(None),
                }
            }
        }
    }
}

impl<T: Widget + 'static> From<Box<T>> for SplitPaneChild {
    fn from(w: Box<T>) -> Self {
        SplitPaneChild {
            bordered: true,
            border: None,
            border_style: Style::new(),
            title: None,
            draggable: true,
            content: SplitPaneContent::Widget(w),
        }
    }
}

impl From<Box<dyn Widget>> for SplitPaneChild {
    fn from(w: Box<dyn Widget>) -> Self {
        SplitPaneChild {
            bordered: true,
            border: None,
            border_style: Style::new(),
            title: None,
            draggable: true,
            content: SplitPaneContent::Widget(w),
        }
    }
}

impl From<SplitPane> for SplitPaneChild {
    fn from(p: SplitPane) -> Self {
        SplitPaneChild {
            bordered: false,
            border: None,
            border_style: Style::new(),
            title: None,
            draggable: true,
            content: SplitPaneContent::Split {
                orientation: p.orientation,
                flex: p.flex,
                children: p.children,
            },
        }
    }
}

impl SplitPaneChild {
    /// Returns the widget when this child is a leaf.
    pub fn get_widget(&self) -> Option<&dyn Widget> {
        match &self.content {
            SplitPaneContent::Widget(w) => Some(&**w),
            _ => None,
        }
    }

    /// Returns the widget mutably when this child is a leaf.
    pub fn get_widget_mut(&mut self) -> Option<&mut dyn Widget> {
        match &mut self.content {
            SplitPaneContent::Widget(w) => Some(&mut **w),
            _ => None,
        }
    }

    /// Sets the border glyph set and enables the border.
    pub fn border(mut self, border: &'static Border) -> Self {
        self.bordered = true;
        self.border = Some(border);
        self
    }

    /// Calls [`SplitPaneChild::border`] when `value` is `true`.
    pub fn border_if(self, value: bool, border: &'static Border) -> Self {
        if value {
            self.border(border)
        } else {
            self
        }
    }

    /// Sets the border style for this pane.
    pub fn border_style(mut self, style: Style) -> Self {
        self.border_style = style;
        self
    }

    /// Disables the border and dragging on this pane.
    pub fn borderless(mut self) -> Self {
        self.bordered = false;
        self.draggable = false;
        self
    }

    /// Calls [`SplitPaneChild::borderless`] when `value` is `true`.
    pub fn borderless_if(self, value: bool) -> Self {
        if value {
            self.borderless()
        } else {
            self
        }
    }

    /// Enables border dragging on this pane.
    pub fn draggable(mut self) -> Self {
        self.draggable = true;
        self
    }

    /// Calls [`SplitPaneChild::draggable`] when `value` is `true`.
    pub fn draggable_if(self, value: bool) -> Self {
        if value {
            self.draggable()
        } else {
            self
        }
    }

    /// Sets the pane title rendered in the top border.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

/// Builder for a row or column of [`SplitPaneChild`] entries.
pub struct SplitPane {
    orientation: Axis2D,
    flex: u8,
    children: Vec<SplitPaneChild>,
}

impl SplitPane {
    fn build(self) -> SplitChild {
        let built_children: Vec<SplitChild> = self
            .children
            .into_iter()
            .map(SplitPaneChild::build)
            .collect();
        SplitChild {
            node: SplitNode::Container {
                orientation: self.orientation,
                children: built_children,
                flex: self.flex,
            },
            size: Cell::new(Vec2::of(0)),
            dragged: Vec2::of(None),
        }
    }
}

impl SplitPane {
    /// Creates an empty pane stacking children along the Y axis.
    pub fn new() -> Self {
        SplitPane {
            orientation: Axis2D::Y,
            flex: 1,
            children: Vec::new(),
        }
    }

    /// Creates an empty pane laying children out along the X axis.
    pub fn horizontal() -> Self {
        SplitPane {
            orientation: Axis2D::X,
            flex: 1,
            children: Vec::new(),
        }
    }

    /// Creates an empty pane stacking children along the Y axis.
    pub fn vertical() -> Self {
        Self::horizontal().orientation(Axis2D::Y)
    }

    /// Sets the layout `axis`.
    pub fn orientation(mut self, axis: Axis2D) -> Self {
        self.orientation = axis;
        self
    }

    /// Sets the flex weight for this pane when nested in a split.
    pub fn flex(mut self, flex: u8) -> Self {
        self.flex = flex;
        self
    }

    /// Appends `children` to this pane.
    pub fn children<const N: usize>(mut self, children: [SplitPaneChild; N]) -> Self {
        self.children.extend(children);
        self
    }
}

fn halve_with_gap(total: u16, gap: u16) -> (u16, u16) {
    let existing_share = total / 2;
    let raw_new = (total - existing_share).saturating_sub(gap);
    let new_cap = total.saturating_sub(gap + Split::MINIMUM);
    let new_size = raw_new.clamp(Split::MINIMUM.min(new_cap), new_cap);
    let existing = total.saturating_sub(gap + new_size);
    (existing, new_size)
}

fn gap_between(a: &SplitChild, b: &SplitChild, default_gap: u16) -> u16 {
    if a.all_borderless() && b.all_borderless() {
        0
    } else {
        default_gap
    }
}

fn total_gap(children: &[SplitChild], default_gap: u16) -> u16 {
    children
        .windows(2)
        .map(|w| gap_between(&w[0], &w[1], default_gap))
        .fold(0u16, |a, b| a.saturating_add(b))
}

#[derive(Clone, Copy)]
struct CrossEdge {
    start: i32,
    end: i32,
    style: Style,
    border: Option<&'static Border>,
}

fn merge_cross_edges(
    a_edges: &[CrossEdge],
    b_edges: &[CrossEdge],
    out: &mut Vec<CrossEdge>,
    cuts: &mut Vec<i32>,
) {
    cuts.clear();
    for e in a_edges.iter().chain(b_edges.iter()) {
        cuts.push(e.start);
        cuts.push(e.end);
    }
    cuts.sort_unstable();
    cuts.dedup();

    for pair in cuts.windows(2) {
        let seg_start = pair[0];
        let seg_end = pair[1];
        if seg_start >= seg_end {
            continue;
        }
        let edge_a = a_edges
            .iter()
            .find(|e| e.start <= seg_start && seg_start < e.end);
        let edge_b = b_edges
            .iter()
            .find(|e| e.start <= seg_start && seg_start < e.end);
        let seg = match (edge_a, edge_b) {
            (Some(a), Some(b)) => CrossEdge {
                start: seg_start,
                end: seg_end,
                style: a.style.apply(b.style),
                border: a.border.or(b.border),
            },
            (Some(a), None) => CrossEdge {
                start: seg_start,
                end: seg_end,
                ..*a
            },
            (None, Some(b)) => CrossEdge {
                start: seg_start,
                end: seg_end,
                ..*b
            },
            (None, None) => continue,
        };
        out.push(seg);
    }
}

fn edge_style_at(edges: &[CrossEdge], pos: i32) -> Option<&CrossEdge> {
    let idx = edges.partition_point(|e| e.end <= pos);
    edges.get(idx).filter(|e| e.start <= pos)
}

fn arm_border(
    edges: &[CrossEdge],
    pos: i32,
    fallback: &'static Border,
) -> &'static Border {
    edge_style_at(edges, pos)
        .and_then(|e| e.border)
        .unwrap_or(fallback)
}

fn junction_along(
    along: Axis2D,
    along_neg: &Border,
    along_pos: &Border,
    perp_neg: &Border,
    perp_pos: &Border,
) -> Option<char> {
    let (l, r, u, d) = match along {
        Axis2D::X => (along_neg, along_pos, perp_neg, perp_pos),
        Axis2D::Y => (perp_neg, perp_pos, along_neg, along_pos),
    };
    junction(l, r, u, d)
}

struct Divider {
    axis: Axis2D,
    pos: i32,
    start: i32,
    end: i32,
    edges: Vec<CrossEdge>,
}

struct DragDivider {
    container_path: Vec<usize>,
    divider_idx: usize,
    axis: Axis2D,
    grab_offset: i32,
}

struct DragState {
    dividers: Vec<DragDivider>,
}

fn build_divider_map(
    dividers: &[Divider],
) -> std::collections::HashMap<(Axis2D, i32), Vec<usize>> {
    let mut map = std::collections::HashMap::new();
    for (i, div) in dividers.iter().enumerate() {
        map.entry((div.axis, div.pos))
            .or_insert_with(Vec::new)
            .push(i);
    }
    map
}

/// Resizable, recursively splittable widget container.
pub struct Split {
    layout: Layout,
    root: SplitChild,
    chrome: Chrome,
    drag: Option<DragState>,
    dividers: Vec<Divider>,
    divider_map: std::collections::HashMap<(Axis2D, i32), Vec<usize>>,
    top_edges: Vec<CrossEdge>,
    bottom_edges: Vec<CrossEdge>,
    left_edges: Vec<CrossEdge>,
    right_edges: Vec<CrossEdge>,
    gap: u16,
}

impl Split {
    const MINIMUM: u16 = 1;

    fn inner_offset(&self) -> Vec2<i32> {
        Vec2::of(self.chrome.get_border_size() as i32)
    }

    fn inner_content_size(&self) -> Vec2<u16> {
        let inner_offset = self.inner_offset();
        let layout_content = self.layout.rect.size;
        Axis2D::map(|a| layout_content[a].saturating_sub(inner_offset[a] as u16 * 2))
    }

    fn inner_content_pos(&self) -> Vec2<i32> {
        self.layout.rect.pos + self.inner_offset()
    }

    fn working_inner(&self, allocated: Vec2<u16>) -> Vec2<u16> {
        let inner_offset = self.inner_offset();
        Axis2D::map(|a| allocated[a].saturating_sub(inner_offset[a] as u16 * 2))
    }

    fn do_layout(&mut self) {
        let inner = self.inner_content_size();
        self.root.resize_root(inner, self.gap, self.chrome.padding);
        self.root.set_leaf_sizes_mut(self.chrome.padding);
    }

    fn preferred_outer_size(&self, base_size: BaseSize) -> Vec2<u16> {
        let borders = self.chrome.get_border_size() * 2;
        Axis2D::map(|a| {
            let raw = self.root.preferred_or_min(a, self.gap, self.chrome.padding, base_size);
            raw.saturating_add(borders)
        })
    }

    fn do_flow_layout(&mut self, allocated: Vec2<u16>) -> Vec2<u16> {
        let prev_size = self.layout.rect.size;
        self.layout.rect.size = allocated;
        self.do_layout();
        self.root.reflow_leaves_mut(self.chrome.padding);
        self.rebuild_render_cache();
        let result = self.preferred_outer_size(get_flow_output_size_layout);
        self.layout.rect.size = prev_size;
        result
    }

    fn do_flow_measure(&self, allocated: Vec2<u16>) -> Vec2<u16> {
        let inner = self.working_inner(allocated);
        self.root.resize_root_measure(inner, self.gap, self.chrome.padding);
        self.root.reflow_leaves_measure(self.chrome.padding);
        self.preferred_outer_size(get_flow_output_size_measure)
    }

    fn rebuild_render_cache(&mut self) {
        let inner_offset = self.inner_offset();
        let has_border = self.chrome.bordered;

        EDGE_POOL.with(|cell| {
            let mut pool = cell.borrow_mut();
            for div in &mut self.dividers {
                let mut edges = std::mem::take(&mut div.edges);
                edges.clear();
                pool.push(edges);
            }
        });
        self.dividers.clear();
        self.root.node.collect_dividers(inner_offset, self.gap, &mut self.dividers);

        let render_size = self.layout.rect.size;

        if has_border {
            let inner_size = self.inner_content_size();
            for div in &mut self.dividers {
                let ca = div.axis.flip();
                let content_start = inner_offset[ca];
                let content_end = inner_offset[ca] + inner_size[ca] as i32;
                if div.start == content_start && div.start > 1 {
                    if let Some(first) = div.edges.first_mut() {
                        first.start = 1;
                    }
                    div.start = 1;
                }
                if div.end == content_end && div.end < render_size[ca] as i32 - 1 {
                    if let Some(last) = div.edges.last_mut() {
                        last.end = render_size[ca] as i32 - 1;
                    }
                    div.end = render_size[ca] as i32 - 1;
                }
            }
        }

        self.divider_map = build_divider_map(&self.dividers);

        if has_border {
            self.top_edges.clear();
            self.root.node.collect_edge_styles(
                Axis2D::X, false, inner_offset.x as i32, self.gap, &mut self.top_edges,
            );
            self.bottom_edges.clear();
            self.root.node.collect_edge_styles(
                Axis2D::X, true, inner_offset.x as i32, self.gap, &mut self.bottom_edges,
            );
            self.left_edges.clear();
            self.root.node.collect_edge_styles(
                Axis2D::Y, false, inner_offset.y as i32, self.gap, &mut self.left_edges,
            );
            self.right_edges.clear();
            self.root.node.collect_edge_styles(
                Axis2D::Y, true, inner_offset.y as i32, self.gap, &mut self.right_edges,
            );

            for edges in [&mut self.top_edges, &mut self.bottom_edges] {
                if let Some(first) = edges.first_mut() {
                    first.start = 1;
                }
                if let Some(last) = edges.last_mut() {
                    last.end = render_size.x as i32 - 1;
                }
            }
            for edges in [&mut self.left_edges, &mut self.right_edges] {
                if let Some(first) = edges.first_mut() {
                    first.start = 1;
                }
                if let Some(last) = edges.last_mut() {
                    last.end = render_size.y as i32 - 1;
                }
            }
        } else {
            self.top_edges.clear();
            self.bottom_edges.clear();
            self.left_edges.clear();
            self.right_edges.clear();
        }
    }

    fn border_did_change(&mut self) {
        let want = self.chrome.border.is_some();
        if self.chrome.bordered != want {
            self.chrome.bordered = want;
            self.dirty_layout();
        } else {
            self.dirty_paint();
        }
    }
}

impl Widget for Split {
    fn get_layout(&self) -> &Layout {
        &self.layout
    }

    fn get_layout_mut(&mut self) -> &mut Layout {
        &mut self.layout
    }

    fn get_name(&self) -> &'static str {
        "Split"
    }

    fn measure_constraints(&mut self) -> Constraints {
        self.each_child_mut(&mut constrain_child, Sign::Positive);
        let borders = self.chrome.get_border_size() * 2;
        let min = self.root.compute_min_size(self.gap, self.chrome.padding);
        let min_size = Axis2D::map(|a| min[a].saturating_add(borders));
        Constraints {
            min_size,
            max_size: Vec2::of(u16::MAX),
            preferred_size: Vec2::of(u16::MAX),
        }
    }

    fn layout_flow(&mut self, allocated: Vec2<u16>) -> Vec2<u16> {
        self.do_flow_layout(allocated)
    }

    fn layout_measure(&self, allocated: Vec2<u16>) -> Vec2<u16> {
        self.do_flow_measure(allocated)
    }

    fn layout_position(&mut self) {
        let pos = self.inner_content_pos();
        self.root.position_children_mut(pos, self.gap, self.chrome.padding);
        self.root.node.reposition_leaves_mut();
    }

    fn render(&self, mut ctx: crate::render::RenderContext) {
        let inner_offset = self.inner_offset();

        self.root.node.render_leaves(inner_offset, self.gap, self.chrome.padding, &mut ctx);

        let fallback_border = self.chrome.get_resolved_border();
        let normal_border_style = self.chrome.get_resolved_border_style();

        for div in &self.dividers {
            let a = div.axis;
            let ca = a.flip();
            for cross in div.start..div.end {
                let edge = edge_style_at(&div.edges, cross);
                let mut cell_style = normal_border_style;
                if let Some(e) = edge {
                    cell_style = cell_style.apply(e.style);
                }

                let mut perp_neg: &Border = Border::HIDDEN;
                let mut perp_pos: &Border = Border::HIDDEN;
                let mut crossed = false;
                for &idx in self.divider_map.get(&(ca, cross)).into_iter().flatten() {
                    let d = &self.dividers[idx];
                    let has_neg = d.start <= div.pos && div.pos - 1 < d.end;
                    let has_pos = d.start <= div.pos + 1 && div.pos < d.end;
                    if !has_neg && !has_pos {
                        continue;
                    }
                    crossed = true;
                    if has_neg {
                        if let Some(e) = edge_style_at(&d.edges, div.pos - 1) {
                            cell_style = cell_style.apply(e.style);
                            perp_neg = e.border.unwrap_or(fallback_border);
                        }
                    }
                    if has_pos {
                        if let Some(e) = edge_style_at(&d.edges, div.pos + 1) {
                            cell_style = cell_style.apply(e.style);
                            perp_pos = e.border.unwrap_or(fallback_border);
                        }
                    }
                }

                let ch = if !crossed {
                    let main_border = edge.and_then(|e| e.border).unwrap_or(fallback_border);
                    main_border.get_edge(a)
                } else {
                    let along_neg = arm_border(&div.edges, cross - 1, fallback_border);
                    let along_pos = arm_border(&div.edges, cross + 1, fallback_border);
                    junction_along(ca, along_neg, along_pos, perp_neg, perp_pos)
                        .unwrap_or_else(|| fallback_border.get_edge(a))
                };

                ctx.set_style(cell_style);
                ctx.move_to(Axis2D::map(|ax| if ax == a { div.pos } else { cross }));
                write!(ctx, "{}", ch);
            }
        }

        if self.chrome.bordered {
            let render_size = self.layout.rect.size;
            let full: Vec2<i32> = Axis2D::map(|a| render_size[a] as i32);

            let get_edges =
                |side_axis: Axis2D, at_end: bool| match (side_axis, at_end) {
                    (Axis2D::Y, false) => &self.top_edges,
                    (Axis2D::Y, true) => &self.bottom_edges,
                    (Axis2D::X, false) => &self.left_edges,
                    (Axis2D::X, true) => &self.right_edges,
                };

            for side_axis in [Axis2D::Y, Axis2D::X] {
                let run_axis = side_axis.flip();
                let run_size = full[run_axis];
                let handles_corners = side_axis == Axis2D::Y;
                let (start, end) = if handles_corners {
                    (0, run_size)
                } else {
                    (1, run_size - 1)
                };

                for at_end in [false, true] {
                    let edge_pos = if at_end { full[side_axis] - 1 } else { 0 };
                    let main_edges = get_edges(side_axis, at_end);

                    for run_pos in start..end {
                        ctx.move_to(Axis2D::map(|ax| {
                            if ax == run_axis {
                                run_pos
                            } else {
                                edge_pos
                            }
                        }));
                        let main_edge = edge_style_at(main_edges, run_pos);
                        let mut cell_style = normal_border_style;
                        let mut cell_border: Option<&Border> = None;

                        let is_corner = handles_corners
                            && (run_pos == 0 || run_pos == run_size - 1);
                        let is_end_corner = run_pos == run_size - 1;

                        if let Some(e) = main_edge {
                            cell_style = cell_style.apply(e.style);
                            cell_border = e.border;
                        } else if is_corner {
                            if let Some(e) = if is_end_corner {
                                main_edges.last()
                            } else {
                                main_edges.first()
                            } {
                                cell_style = cell_style.apply(e.style);
                                if cell_border.is_none() {
                                    cell_border = e.border;
                                }
                            }
                            let perp_edges = get_edges(run_axis, is_end_corner);
                            if let Some(e) = if at_end {
                                perp_edges.last()
                            } else {
                                perp_edges.first()
                            } {
                                cell_style = cell_style.apply(e.style);
                                if cell_border.is_none() {
                                    cell_border = e.border;
                                }
                            }
                        }

                        let crossing = self
                            .divider_map
                            .get(&(run_axis, run_pos))
                            .and_then(|indices| {
                                indices
                                    .iter()
                                    .map(|&i| &self.dividers[i])
                                    .find(|d| {
                                        if at_end {
                                            d.end == full[side_axis] - 1
                                        } else {
                                            d.start == 1
                                        }
                                    })
                            });
                        let crossing_div_border = crossing.and_then(|d| {
                            let crossing_edge = if at_end { d.end - 1 } else { d.start };
                            if let Some(de) = edge_style_at(&d.edges, crossing_edge) {
                                cell_style = cell_style.apply(de.style);
                                if cell_border.is_none() {
                                    cell_border = de.border;
                                }
                                de.border
                            } else {
                                None
                            }
                        });

                        let border = cell_border.unwrap_or(fallback_border);
                        ctx.set_style(cell_style);
                        let ch = if is_corner {
                            let corner_end = Axis2D::map(|ax| {
                                if ax == run_axis { is_end_corner } else { at_end }
                            });
                            border.get_corner(corner_end)
                        } else if crossing.is_some() {
                            let along_neg = arm_border(main_edges, run_pos - 1, fallback_border);
                            let along_pos = arm_border(main_edges, run_pos + 1, fallback_border);
                            let inward = crossing_div_border.unwrap_or(border);
                            let (perp_neg, perp_pos) = if at_end {
                                (inward, Border::HIDDEN)
                            } else {
                                (Border::HIDDEN, inward)
                            };
                            junction_along(run_axis, along_neg, along_pos, perp_neg, perp_pos)
                                .unwrap_or_else(|| border.get_edge(side_axis))
                        } else {
                            border.get_edge(side_axis)
                        };
                        write!(ctx, "{}", ch);
                    }
                }
            }
        }

        self.root.node.render_titles(
            inner_offset,
            self.gap,
            normal_border_style,
            &mut ctx,
        );
    }

    fn each_child(
        &self,
        f: &mut dyn FnMut(&dyn Widget),
        direction: Sign,
    ) {
        self.root.node.for_each_leaf(f, direction);
    }

    fn each_child_mut(
        &mut self,
        f: &mut dyn FnMut(&mut dyn Widget),
        direction: Sign,
    ) {
        self.root.node.for_each_leaf_mut(f, direction);
    }

    fn find_descendant(
        &self,
        predicate: &dyn Fn(&dyn Widget) -> bool,
        mut path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        self.root.node.find_leaf(&mut |w| {
            let grandchild =
                w.find_descendant(predicate, path.as_mut().map(|p| &mut **p));
            if grandchild.is_some() {
                if let Some(p) = &mut path {
                    p.push(w.get_id());
                }
                return grandchild;
            }
            if predicate(w) {
                if let Some(p) = &mut path {
                    p.push(w.get_id());
                }
                return Some(w.get_id());
            }
            None
        })
    }

    fn descendant_at_pos(
        &self,
        pos: Vec2<f32>,
        mut path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        let abs_offset = self.inner_content_pos();
        self.root.node.find_descendant_at_pos(
            pos,
            abs_offset,
            self.gap,
            path.as_mut().map(|p| &mut **p),
        )
    }

    fn find_descendant_at_pos(
        &self,
        pos: Vec2<f32>,
        predicate: &dyn Fn(&dyn Widget) -> bool,
        mut path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        let abs_offset = self.inner_content_pos();
        self.root.node.find_matching_descendant_at_pos(
            pos,
            predicate,
            abs_offset,
            self.gap,
            path.as_mut().map(|p| &mut **p),
        )
    }

    fn can_scroll(&self, direction: Direction2D) -> bool {
        self.root
            .node
            .find_leaf(&mut |w| w.can_scroll(direction).then_some(true))
            .unwrap_or(false)
    }

    fn get_cursor(
        &self,
        selected: Option<WidgetId>,
    ) -> Option<(CursorShape, Vec2<i32>)> {
        let selected = selected?;
        self.root.node.find_leaf(&mut |w| {
            self.layout.get_child_cursor(w, selected)
        })
    }

    fn on_input(&mut self, queue: &mut InputQueue) -> InputResult {
        let Some(event) = queue.peek() else { return InputResult::Rejected; };
        match &event.chord {
            chord!(LeftClick) => {
                let mouse = event.cell();
                let mouse_vec = Vec2::new(mouse.x as i32, mouse.y as i32);

                let inner_offset = self.inner_offset();

                let mut drag_dividers = Vec::new();
                let mut path_buf = Vec::new();

                self.root.node.find_all_dividers_at_pos(
                    inner_offset,
                    Axis2D::X,
                    mouse_vec,
                    self.gap,
                    &mut path_buf,
                    &mut drag_dividers,
                );
                path_buf.clear();
                self.root.node.find_all_dividers_at_pos(
                    inner_offset,
                    Axis2D::Y,
                    mouse_vec,
                    self.gap,
                    &mut path_buf,
                    &mut drag_dividers,
                );

                if drag_dividers.is_empty() {
                    return InputResult::Rejected;
                }

                queue.next();

                self.drag = Some(DragState {
                    dividers: drag_dividers,
                });
                InputResult::Handled
            }
            chord!(LeftDrag) => {
                if let Some(drag) = self.drag.take() {
                    queue.next();
                    let inner_offset = self.inner_offset();

                    let snapshot = if self.layout.flex > 0 {
                        let mut s = Vec::new();
                        self.root.snapshot_sizes(&mut s);
                        let before = Axis2D::map(|a| {
                            self.root.preferred_or_min(a, self.gap, self.chrome.padding, get_flow_output_size_layout)
                        });
                        Some((s, before))
                    } else {
                        None
                    };

                    for dd in &drag.dividers {
                        let a = dd.axis;
                        let Some(div_pos) = self.root.divider_abs_pos(
                            &dd.container_path,
                            dd.divider_idx,
                            a,
                            inner_offset,
                            self.gap,
                        ) else {
                            continue;
                        };
                        let delta = event.cell()[a] as i32
                            - div_pos
                            - dd.grab_offset;
                        if delta != 0 {
                            self.root.move_divider_propagate(
                                &dd.container_path,
                                dd.divider_idx,
                                a,
                                delta,
                                self.gap,
                                self.chrome.padding,
                                true,
                            );
                        }
                    }

                    self.root.set_leaf_sizes_mut(self.chrome.padding);
                    self.root.reflow_leaves_mut(self.chrome.padding);

                    if let Some((snap, before)) = snapshot {
                        let inner = self.inner_content_size();
                        let after = Axis2D::map(|a| {
                            self.root.preferred_or_min(a, self.gap, self.chrome.padding, get_flow_output_size_layout)
                        });
                        let worsens = [Axis2D::X, Axis2D::Y].iter().any(|&a| {
                            after[a] > inner[a] && after[a] > before[a]
                        });
                        if worsens {
                            let mut idx = 0;
                            self.root.restore_sizes(&snap, &mut idx);
                            self.root.set_leaf_sizes_mut(self.chrome.padding);
                            self.root.reflow_leaves_mut(self.chrome.padding);
                        }
                    }

                    let pos = self.inner_content_pos();
                    self.root.position_children_mut(pos, self.gap, self.chrome.padding);
                    self.root.node.reposition_leaves_mut();
                    self.rebuild_render_cache();
                    self.dirty_layout();

                    self.drag = Some(drag);
                    InputResult::Handled
                } else {
                    InputResult::Rejected
                }
            }
            chord!(LeftRelease) => {
                if self.drag.is_some() {
                    self.drag = None;
                    queue.next();
                    InputResult::Handled
                } else {
                    InputResult::Rejected
                }
            }
            _ => InputResult::Rejected,
        }
    }
}

impl Split {
    /// Creates a split from a [`SplitPane`].
    pub fn new(pane: SplitPane) -> Box<Self> {
        let root = pane.build();
        Box::new(Self {
            layout: Layout::new(),
            root,
            chrome: Chrome::new(),
            drag: None,
            dividers: Vec::new(),
            divider_map: std::collections::HashMap::new(),
            top_edges: Vec::new(),
            bottom_edges: Vec::new(),
            left_edges: Vec::new(),
            right_edges: Vec::new(),
            gap: 1,
        })
    }

    /// Creates an empty split laying children out along the X axis.
    pub fn horizontal() -> Box<Self> {
        Self::new(SplitPane::horizontal())
    }

    /// Creates an empty split stacking children along the Y axis.
    pub fn vertical() -> Box<Self> {
        Self::new(SplitPane::vertical())
    }

    /// Sets the layout `axis` of the root container.
    pub fn orientation(mut self: Box<Self>, axis: Axis2D) -> Box<Self> {
        if let SplitNode::Container { orientation, .. } = &mut self.root.node {
            *orientation = axis;
        }
        self
    }

    /// Appends `children` to the root container.
    pub fn children<const N: usize>(
        mut self: Box<Self>,
        children: [SplitPaneChild; N],
    ) -> Box<Self> {
        if let SplitNode::Container { children: existing, .. } = &mut self.root.node {
            for c in children {
                existing.push(c.build());
            }
        }
        self
    }

    crate::layout_field! {
        /// Whether a border is drawn around the split.
        bordered: bool => chrome.bordered
    }

    crate::field! {
        /// The border glyph set.
        border: Option<&'static Border> => chrome.border;
        border_did_change
    }

    crate::style_field! {
        /// The border style.
        border_style: Style => chrome.border_style
    }

    crate::layout_field! {
        /// The space between the border and the content.
        padding: Spacing => chrome.padding
    }

    /// Sets top and bottom padding to `n` cells.
    pub fn vertical_padding(mut self: Box<Self>, n: u8) -> Box<Self> {
        self.set_padding(self.get_padding().vertical(n));
        self
    }

    /// Sets left and right padding to `n` cells.
    pub fn horizontal_padding(mut self: Box<Self>, n: u8) -> Box<Self> {
        self.set_padding(self.get_padding().horizontal(n));
        self
    }

    /// Sets the left padding to `n` cells.
    pub fn padding_left(mut self: Box<Self>, n: u8) -> Box<Self> {
        self.set_padding(self.get_padding().left(n));
        self
    }

    /// Sets the right padding to `n` cells.
    pub fn padding_right(mut self: Box<Self>, n: u8) -> Box<Self> {
        self.set_padding(self.get_padding().right(n));
        self
    }

    /// Sets the top padding to `n` cells.
    pub fn padding_top(mut self: Box<Self>, n: u8) -> Box<Self> {
        self.set_padding(self.get_padding().top(n));
        self
    }

    /// Sets the bottom padding to `n` cells.
    pub fn padding_bottom(mut self: Box<Self>, n: u8) -> Box<Self> {
        self.set_padding(self.get_padding().bottom(n));
        self
    }

    /// Returns mutable access to the leaf containing `target`.
    pub fn get_leaf_mut(
        &mut self,
        target: WidgetId<impl ?Sized>,
    ) -> Option<&mut SplitLeaf> {
        let leaf = self.root.node.leaf_mut(target.untyped())?;
        self.layout.or_dirty(crate::widget::DirtyImpact::Layout);
        Some(leaf)
    }

    /// Splits the leaf containing `target` along `axis`, inserting `child` on the given side.
    ///
    /// # Panics
    ///
    /// Panics if `child` is a nested [`SplitPane`] rather than a single widget leaf.
    #[track_caller]
    pub fn split(
        &mut self,
        target: WidgetId<impl ?Sized>,
        child: impl Into<SplitPaneChild>,
        axis: Axis2D,
        direction: Sign,
    ) {
        let target = target.untyped();
        let new_leaf = child.into().into_leaf_data();
        if self.root.split(target, new_leaf, axis, direction, self.gap) {
            self.root.reinforce_mins(self.gap, self.chrome.padding, get_flow_output_size_layout);
            self.dirty_layout();
        }
    }

    /// Splits the root along `axis`, inserting `child` on the given side.
    ///
    /// # Panics
    ///
    /// Panics if `child` is a nested [`SplitPane`] rather than a single widget leaf.
    pub fn split_root(
        &mut self,
        child: impl Into<SplitPaneChild>,
        axis: Axis2D,
        direction: Sign,
    ) {
        let new_node = SplitNode::Leaf(child.into().into_leaf_data());
        self.root.push_to_root(new_node, axis, direction, self.gap);
        self.root.reinforce_mins(self.gap, self.chrome.padding, get_flow_output_size_layout);
        self.dirty_layout();
    }

    /// Sets the flex weight of the container holding `target`.
    pub fn set_container_flex_of(&mut self, target: WidgetId<impl ?Sized>, flex: u8) -> bool {
        let changed = self.root.node.set_container_flex_of(target.untyped(), flex);
        if changed {
            self.dirty_layout();
        }
        changed
    }

    /// Returns whether `target` exists anywhere in the split.
    pub fn contains(&self, target: WidgetId<impl ?Sized>) -> bool {
        self.root.node.contains(target.untyped())
    }

    /// Removes and returns the leaf containing `target`, or `None` if only one leaf remains.
    pub fn remove(&mut self, target: WidgetId<impl ?Sized>) -> Option<SplitPaneChild> {
        let target = target.untyped();
        if self.root.node.is_single_leaf() {
            return None;
        }
        let result = self.root.node.close_inner(target, self.gap, self.chrome.padding);
        if result.is_some() {
            self.root.relayout_leaves(self.chrome.padding);
            self.dirty_layout();
        }
        result.map(SplitPaneChild::from_leaf_data)
    }

    /// Resets pane sizes from flex weights, discarding drag adjustments.
    pub fn redistribute(&mut self) {
        self.root.clear_dragged();
        self.root.flex_distribute(self.gap, self.chrome.padding, get_flow_output_size_layout);
        self.root.reinforce_mins(self.gap, self.chrome.padding, get_flow_output_size_layout);
        self.dirty_layout();
    }
}
