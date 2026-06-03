//! Widget tree traversal and layout utilities.

use crate::prelude::*;

pub(crate) fn rect_contains_f32(rect: Rect<i32, u16>, pos: Vec2<f32>) -> bool {
    Axis2D::all(|a| {
        pos[a] >= rect.pos[a] as f32
            && pos[a] < (rect.pos[a] + rect.size[a] as i32) as f32
    })
}

pub(crate) fn widget_rect_or_zero(root: &dyn Widget, id: WidgetId) -> Rect<i32, u16> {
    root.find(id).map(|w| w.get_rect())
        .unwrap_or_else(|| Rect::new(Vec2::of(0i32), Vec2::of(0u16)))
}

pub(crate) fn rect_center(r: Rect<i32, u16>) -> Vec2<i32> {
    Axis2D::map(|a| r.pos[a] + r.size[a] as i32 / 2)
}

pub(crate) fn cell_px() -> Vec2<i32> {
    match crate::runtime::get_terminal_info().cell_size {
        Some(px) => Vec2::new(px.x as i32, px.y as i32),
        None => Vec2::of(1i32),
    }
}

pub(crate) fn hit_test_z(
    widget: &dyn Widget,
    pos: Vec2<f32>,
    path: &mut Vec<WidgetId>,
    excluded: &[WidgetId],
) -> Option<(WidgetId, Layer)> {
    let pos = pos - widget.subcell_offset();
    let mut best: Option<(WidgetId, Layer, Vec<WidgetId>)> = None;
    widget.each_child(
        &mut |child| {
            let z = child.get_layer();
            let rect = child.get_rect();
            let in_rect = rect_contains_f32(rect, pos);
            if z == Layer::Bottom && !in_rect && !child.get_layout().is_overflowing() {
                return;
            }
            let mut sub_path: Vec<WidgetId> = Vec::new();
            let child_hit = hit_test_z(child, pos, &mut sub_path, excluded);
            let (leaf_id, leaf_z) = match child_hit {
                Some((id, hit_z)) => {
                    sub_path.push(child.get_id());
                    (id, hit_z.max(z))
                }
                None => {
                    if !in_rect || excluded.contains(&child.get_id()) {
                        return;
                    }
                    sub_path.push(child.get_id());
                    (child.get_id(), z)
                }
            };
            if best.as_ref().map_or(true, |(_, b_z, _)| leaf_z > *b_z) {
                best = Some((leaf_id, leaf_z, sub_path));
            }
        },
        Sign::Negative,
    );
    best.map(|(id, z, sub_path)| {
        path.extend(sub_path);
        (id, z)
    })
}

#[cfg(feature = "gui")]
pub(crate) fn path_subcell_offset(
    root: &dyn Widget,
    path: &[WidgetId],
) -> Vec2<i32> {
    if path.is_empty() || root.get_id() != path[0] {
        return Vec2::of(0i32);
    }
    let mut total = root.subcell_offset();
    let mut current: &dyn Widget = root;
    for &next_id in &path[1..] {
        let Some(c) = current.get_child(next_id) else { break };
        current = c;
        total = total + current.subcell_offset();
    }
    let cp = cell_px();
    Axis2D::map(|a| (total[a] * cp[a] as f32).round() as i32)
}

/// Walks `path` from `root` applying each ancestor's render-time shift to `window_pos`,
/// yielding the post-shift window-frame position. Callers subtract `target.get_pos()` to
/// get the leaf-local coordinate.
pub(crate) fn window_to_leaf(
    root: &dyn Widget,
    path: &[WidgetId],
    window_pos: Vec2<f32>,
) -> Vec2<f32> {
    let mut pos = window_pos;
    let mut current: &dyn Widget = root;
    for &next_id in &path[1..] {
        pos = pos - current.subcell_offset();
        let Some(c) = current.get_child(next_id) else { break };
        current = c;
    }
    pos
}

pub(crate) fn build_scroll_path(
    root: &dyn Widget,
    mouse_pos: Vec2<f32>,
    direction: Direction2D,
) -> Vec<WidgetId> {
    let mut path: Vec<WidgetId> = vec![];
    let found = root.find_descendant_at_pos(
        mouse_pos,
        &|child| child.can_scroll(direction),
        Some(&mut path),
    );
    if found.is_some() || root.can_scroll(direction) {
        path.push(root.get_id());
        path.reverse();
    }
    path
}

fn recursive_before_layout(widget: &mut dyn Widget) {
    widget.before_layout();
    widget.each_child_mut(&mut recursive_before_layout, Sign::Positive);
}

pub(crate) fn check_dirty(widget: &mut dyn Widget) -> DirtyImpact {
    let mut dirty = widget.get_layout().get_dirty();
    widget.each_child_mut(&mut |child: &mut dyn Widget| {
        dirty |= check_dirty(child);
    }, Sign::Positive);
    widget.get_layout_mut().set_dirty(dirty);
    dirty
}

pub(crate) fn clear_dirty(widget: &mut dyn Widget) {
    widget.each_child_mut(&mut clear_dirty, Sign::Positive);
    widget.get_layout_mut().set_dirty(DirtyImpact::None);
}

pub(crate) fn compute_overflows(widget: &mut dyn Widget) -> bool {
    let mut overflows = widget.get_layer() > Layer::Bottom;
    widget.each_child_mut(&mut |child: &mut dyn Widget| {
        if compute_overflows(child) {
            overflows = true;
        }
    }, Sign::Positive);
    widget.get_layout_mut().set_overflowing(overflows);
    overflows
}

fn recursive_after_layout(widget: &mut dyn Widget) {
    widget.each_child_mut(&mut recursive_after_layout, Sign::Positive);
    widget.get_layout_mut().set_dirty(DirtyImpact::None);
    widget.after_layout();
}

pub(crate) fn perform_layout(root: &mut dyn Widget, size: Vec2<u16>, shrink_wrap: bool) {
    recursive_before_layout(root);
    constrain_child(root);
    let size = if shrink_wrap {
        let measured = measure_child(root, size);
        let shrunk_x = if root.get_flex() > 0 {
            size[Axis2D::X]
        } else {
            measured[Axis2D::X].min(size[Axis2D::X])
        };
        let measured = measure_child(root, Vec2::new(shrunk_x, size[Axis2D::Y]));
        Axis2D::map(|a| {
            if root.get_flex() > 0 {
                size[a]
            } else {
                measured[a].min(size[a])
            }
        })
    } else {
        size
    };
    flow_child(root, size);
    let margin = root.get_layout().get_margin_total();
    root.set_rect_size(Axis2D::map(|a| size[a].saturating_sub(margin[a])));
    recursive_after_layout(root);
}

pub(crate) fn focus_along_path(root: &mut dyn Widget, path: &[WidgetId], scroll: Vec2<Option<Align>>) {
    if path.is_empty() {
        return;
    }
    fn collect_and_focus(
        widget: &mut dyn Widget,
        path: &[WidgetId],
        idx: usize,
        scroll: Vec2<Option<Align>>,
        revelation: &mut crate::widget::Revelation,
    ) -> bool {
        if idx >= path.len() {
            return false;
        }
        if widget.get_id() != path[idx] {
            return false;
        }
        if idx == path.len() - 1 {
            revelation.push(Rect::new(Vec2::of(0i32), widget.get_rect_size()));
            widget.reveal(None, revelation, scroll);
            return true;
        }
        let next_id = path[idx + 1];
        let widget_content_pos = widget.get_pos();
        let mut found = false;
        widget.each_child_mut(
            &mut |child| {
                if !found && child.get_id() == next_id {
                    let child_content_pos = child.get_pos();
                    if collect_and_focus(child, path, idx + 1, scroll, revelation) {
                        revelation.translate(child_content_pos - widget_content_pos);
                        found = true;
                    }
                }
            },
            Sign::Positive,
        );
        if found {
            widget.reveal(Some(next_id), revelation, scroll);
            return true;
        }
        false
    }
    let mut revelation = crate::widget::Revelation::new();
    collect_and_focus(root, path, 0, scroll, &mut revelation);
}

pub(crate) fn compute_focused_measure(root: &dyn Widget, path: &[WidgetId]) -> Option<FocusedMeasure> {
    if path.is_empty() {
        return None;
    }
    fn collect_measures(
        widget: &dyn Widget,
        path: &[WidgetId],
        idx: usize,
        vis_min: &mut Vec2<i32>,
        vis_max: &mut Vec2<i32>,
    ) -> Option<(Vec2<i32>, Vec2<u16>)> {
        if idx >= path.len() || widget.get_id() != path[idx] {
            return None;
        }
        let widget_pos = widget.get_pos();
        let widget_size = widget.get_rect_size();

        if idx == path.len() - 1 {
            *vis_min = widget_pos;
            *vis_max = Axis2D::map(|a| widget_pos[a] + widget_size[a] as i32);
            return Some((widget_pos, widget_size));
        }
        let next_id = path[idx + 1];
        let mut result = None;
        widget.each_child(
            &mut |child| {
                if result.is_none() && child.get_id() == next_id {
                    result = collect_measures(child, path, idx + 1, vis_min, vis_max);
                    if result.is_some() {
                        *vis_min = Axis2D::map(|a| vis_min[a].max(widget_pos[a]));
                        *vis_max = Axis2D::map(|a| vis_max[a].min(widget_pos[a] + widget_size[a] as i32));
                    }
                }
            },
            Sign::Positive,
        );
        result
    }
    let mut vis_min = Vec2::of(0i32);
    let mut vis_max = Vec2::of(0i32);
    collect_measures(root, path, 0, &mut vis_min, &mut vis_max).map(|(pos, size)| FocusedMeasure {
        pos,
        size,
        visible_pos: Axis2D::map(|a| vis_min[a].max(0) as u16),
        visible_size: Axis2D::map(|a| (vis_max[a] - vis_min[a]).max(0) as u16),
    })
}

pub(crate) fn find_focusable_1d(
    widget: &dyn Widget,
    focus_chain: &[WidgetId],
    direction: Sign,
    out_path: &mut Vec<WidgetId>,
) -> bool {
    let mut found = false;
    let mut resume_after = focus_chain.first().copied();

    widget.each_child(
        &mut |child| {
            if found {
                return;
            }

            if let Some(selected_first) = resume_after {
                if child.get_id() == selected_first {
                    out_path.push(child.get_id());
                    if find_focusable_1d(
                        child,
                        &focus_chain[1..],
                        direction,
                        out_path,
                    ) {
                        found = true;
                        return;
                    }
                    out_path.pop();
                    resume_after = None;
                }
                return;
            }

            if child.is_focusable() || child.get_focus_target().is_some() {
                out_path.push(child.get_id());
                found = true;
                return;
            }
            out_path.push(child.get_id());
            if find_focusable_1d(child, &[], direction, out_path) {
                found = true;
                return;
            }
            out_path.pop();
        },
        direction,
    );

    found
}

struct FindFocusable2DState {
    found: bool,
    path: Vec<WidgetId>,
    shared_depth: usize,
    perp_overlap: bool,
    dist: i32,
}

pub(crate) fn find_focusable_2d(
    root: &dyn Widget,
    focus_chain: &[WidgetId],
    desired: Vec2<i32>,
    direction: Direction2D,
) -> Option<Vec<WidgetId>> {
    let root_rect = root.get_rect();
    let selected_rect = if !focus_chain.is_empty() {
        let mut clip = root_rect;
        fn get_selected_rect(
            widget: &dyn Widget,
            path: &[WidgetId],
            clip: &mut Rect<i32, u16>,
        ) {
            if path.is_empty() {
                return;
            }
            widget.each_child(
                &mut |child| {
                    if child.get_id() == path[0] {
                        *clip = rect_intersect(&child.get_rect(), clip);
                        get_selected_rect(child, &path[1..], clip);
                    }
                },
                Sign::Positive,
            );
        }
        get_selected_rect(root, focus_chain, &mut clip);
        clip
    } else {
        Rect::new(desired, Vec2::of(1))
    };

    let mut state = FindFocusable2DState {
        found: false,
        path: Vec::new(),
        shared_depth: 0,
        perp_overlap: false,
        dist: 0,
    };
    let mut out_path = vec![root.get_id()];

    find_focusable_2d_recurse(
        root,
        focus_chain,
        0,
        direction,
        desired,
        &selected_rect,
        &mut state,
        &mut out_path,
        &root_rect,
    );

    state.found.then(|| state.path)
}

fn ranges_overlap(
    a_start: i32,
    a_size: u16,
    b_start: i32,
    b_size: u16,
) -> bool {
    let a_end = a_start + a_size as i32;
    let b_end = b_start + b_size as i32;
    a_start < b_end && b_start < a_end
}

fn rect_intersect(a: &Rect<i32, u16>, b: &Rect<i32, u16>) -> Rect<i32, u16> {
    let pos = Axis2D::map(|ax| a.pos[ax].max(b.pos[ax]));
    let end = Axis2D::map(|ax| {
        (a.pos[ax] + a.size[ax] as i32).min(b.pos[ax] + b.size[ax] as i32)
    });
    let size = Axis2D::map(|ax| (end[ax] - pos[ax]).max(0) as u16);
    Rect::new(pos, size)
}

fn find_focusable_2d_recurse(
    widget: &dyn Widget,
    focus_chain: &[WidgetId],
    shared_depth: usize,
    direction: Direction2D,
    desired: Vec2<i32>,
    selected_rect: &Rect<i32, u16>,
    state: &mut FindFocusable2DState,
    out_path: &mut Vec<WidgetId>,
    clip: &Rect<i32, u16>,
) {
    let axis = direction.axis();
    let perp = axis.flip();
    let direction_sign = direction.screen_sign();

    widget.each_child(
        &mut |child| {
            out_path.push(child.get_id());

            let (child_shared, child_focus_chain) =
                if focus_chain.first().map_or(false, |&head| child.get_id() == head) {
                    (shared_depth + 1, &focus_chain[1..])
                } else {
                    (shared_depth, &[][..])
                };

            if child.is_focusable() || child.get_focus_target().is_some() {
                let outer = child.get_rect();
                let child_pos = outer.pos;
                let child_size = outer.size;

                let is_selected =
                    focus_chain.last().map_or(false, |&sel| child.get_id() == sel);

                let sel_end = selected_rect.pos[axis] + selected_rect.size[axis] as i32;
                let child_end = child_pos[axis] + child_size[axis] as i32;
                let has_axis_overlap = child_pos[axis] < sel_end
                    && selected_rect.pos[axis] < child_end;
                let child_mid = child_pos[axis] + child_size[axis] as i32 / 2;
                let sel_mid = selected_rect.pos[axis] + selected_rect.size[axis] as i32 / 2;
                let in_direction = Sign::from(&sel_mid, &child_mid) == Some(direction_sign);

                if !is_selected && !has_axis_overlap && in_direction {
                    let axis_dist = if direction_sign == Sign::Positive {
                        child_pos[axis] - sel_end
                    } else {
                        selected_rect.pos[axis] - child_end
                    };
                    let perp_near = desired[perp].clamp(
                        child_pos[perp],
                        child_pos[perp] + child_size[perp] as i32 - 1,
                    );
                    let perp_dist = (desired[perp] - perp_near).abs();

                    let y_gap = (child_pos.y - (selected_rect.pos.y + selected_rect.size.y as i32))
                        .max(selected_rect.pos.y - (child_pos.y + child_size.y as i32))
                        .max(0);

                    let (valid, perp_overlap) = if axis.is_x() {
                        let has_y_overlap = ranges_overlap(
                            selected_rect.pos.y,
                            selected_rect.size.y,
                            child_pos.y,
                            child_size.y,
                        );
                        (y_gap <= 10, has_y_overlap)
                    } else {
                        (true, false)
                    };

                    if valid {
                        let axis_mul = if axis.is_x() { 3 } else { 20 };
                        let dist = axis_dist * axis_mul + perp_dist;

                        let dominated = !state.found
                            || child_shared > state.shared_depth
                            || (child_shared == state.shared_depth
                                && if axis.is_x() {
                                    (perp_overlap && !state.perp_overlap)
                                        || (perp_overlap
                                            == state.perp_overlap
                                            && dist < state.dist)
                                } else {
                                    dist < state.dist
                                });

                        if dominated {
                            state.found = true;
                            state.shared_depth = child_shared;
                            state.perp_overlap = perp_overlap;
                            state.dist = dist;
                            state.path.clone_from(out_path);
                        }
                    }
                }
            }

            if child.is_focusable() || child.get_focus_target().is_none() {
                let child_clip = rect_intersect(&child.get_rect(), clip);
                find_focusable_2d_recurse(
                    child,
                    child_focus_chain,
                    child_shared,
                    direction,
                    desired,
                    selected_rect,
                    state,
                    out_path,
                    &child_clip,
                );
            }

            out_path.pop();
        },
        Sign::Positive,
    );
}

pub(crate) fn find_nearest_focusable(
    widget: &dyn Widget,
    target: Vec2<i32>,
    best: &mut (u8, f64, Option<WidgetId>),
) {
    widget.each_child(
        &mut |child| {
            if child.is_focusable() {
                let outer = child.get_rect();
                let pos = outer.pos;
                let size = outer.size;
                let y_dist = (target.y
                    - target.y.clamp(pos.y, pos.y + (size.y as i32 - 1).max(0)))
                .abs() as f64;
                let x_dist = (target.x
                    - target.x.clamp(pos.x, pos.x + (size.x as i32 - 1).max(0)))
                .abs() as f64;
                let near_x = target.x >= pos.x;

                let (tier, dist) = if near_x && y_dist == 0.0 {
                    (0, x_dist)
                } else if near_x && y_dist <= 1.0 {
                    (1, x_dist + y_dist)
                } else {
                    let y_normal = y_dist * 2.0;
                    (2, (x_dist * x_dist + y_normal * y_normal).sqrt())
                };

                if tier < best.0
                    || (tier == best.0 && dist < best.1)
                    || (tier <= best.0 && dist * 2.0 < best.1)
                {
                    best.0 = tier;
                    best.1 = dist;
                    best.2 = Some(child.get_id());
                }
            }

            find_nearest_focusable(child, target, best);
        },
        Sign::Positive,
    );
}
