//! Widget trait and layout primitives.

pub mod align;
pub mod chrome;
pub mod events;
pub mod field;
pub mod flex;
pub mod input;
pub mod revelation;
pub mod scrollbar;
/// Concrete widget implementations.
pub mod widgets;
pub use revelation::{Revelation, resolve_revelation_axis};

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use crate::prelude::*;
use crate::widget::align::{AlignOverride, FlexAlign};
use nonmax::NonMaxU16;
use std::marker::PhantomData;
use std::ops::{Bound, RangeBounds};

/// Severity of a pending change.
#[derive(Debug, PartialEq, Eq, Clone, Copy, PartialOrd, Ord)]
pub enum DirtyImpact {
    /// No pending change.
    None = 0,
    /// A repaint is required.
    Paint = 1,
    /// A layout and repaint are required.
    Layout = 2,
}

impl std::fmt::Display for DirtyImpact {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::Paint => write!(f, "Paint"),
            Self::Layout => write!(f, "Layout"),
        }
    }
}

/// Render layer used to order widgets that overlap.
#[derive(Debug, PartialEq, Eq, Clone, Copy, PartialOrd, Ord, Hash)]
pub enum Layer {
    /// The bottom render layer.
    Bottom = 0,
    /// The middle render layer.
    Middle = 1,
    /// The top render layer.
    Top = 2,
}

impl std::fmt::Display for Layer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bottom => write!(f, "Bottom"),
            Self::Middle => write!(f, "Middle"),
            Self::Top => write!(f, "Top"),
        }
    }
}

impl std::ops::BitOrAssign for DirtyImpact {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = std::cmp::max(*self, rhs);
    }
}

fn next_widget_id() -> u64 {
    thread_local! {
        static COUNTER: Cell<u64> = const { Cell::new(1) };
    }
    COUNTER.with(|c| {
        let id = c.get();
        c.set(id + 1);
        id
    })
}

/// Coercion from `Box<Dyn>` to `Box<Self>` when the runtime type matches.
pub trait Downcastable<Dyn: ?Sized> {
    /// Returns the boxed value as `Box<Self>` when the underlying type matches.
    fn downcast_box(boxed: Box<Dyn>) -> Option<Box<Self>>;
}

impl<Dyn: ?Sized> Downcastable<Dyn> for Dyn {
    fn downcast_box(boxed: Box<Dyn>) -> Option<Box<Dyn>> {
        Some(boxed)
    }
}

impl<T: Widget> Downcastable<dyn Widget> for T {
    fn downcast_box(boxed: Box<dyn Widget>) -> Option<Box<T>> {
        (boxed as Box<dyn std::any::Any>).downcast::<T>().ok()
    }
}

/// Blanket bound for anything that can be downcast from `Box<dyn Widget>`.
pub trait AnyWidget: Downcastable<dyn Widget> {}
impl<T: Downcastable<dyn Widget> + ?Sized> AnyWidget for T {}

#[macro_export]
macro_rules! delegate_widget {
    ($field:ident) => {
        fn get_delegate(&self) -> &dyn $crate::widget::Widget {
            &*self.$field
        }
        fn get_delegate_mut(&mut self) -> &mut dyn $crate::widget::Widget {
            &mut *self.$field
        }
    };
}

/// Stable identifier for a [`Widget`], optionally typed for safe downcasts.
pub struct WidgetId<T: ?Sized = dyn Widget>(u64, PhantomData<*const T>);

impl<T: ?Sized> Clone for WidgetId<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: ?Sized> Copy for WidgetId<T> {}

unsafe impl<T: ?Sized> Send for WidgetId<T> {}
unsafe impl<T: ?Sized> Sync for WidgetId<T> {}

impl<T: ?Sized, U: ?Sized> PartialEq<WidgetId<U>> for WidgetId<T> {
    fn eq(&self, other: &WidgetId<U>) -> bool {
        self.0 == other.0
    }
}
impl<T: ?Sized> Eq for WidgetId<T> {}

impl<T: ?Sized> WidgetId<T> {
    /// A sentinel [`WidgetId`] representing no widget.
    pub const EMPTY: Self = Self(0, PhantomData);

    /// Returns an untyped [`WidgetId`] over `dyn Widget`.
    pub fn untyped(self) -> WidgetId {
        WidgetId(self.0, PhantomData)
    }

    /// Returns true when this id equals [`WidgetId::EMPTY`].
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl<T: ?Sized> std::fmt::Debug for WidgetId<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WidgetId({})", self.0)
    }
}

impl<T: ?Sized> std::hash::Hash for WidgetId<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

/// Ordered chain of [`WidgetId`]s from a root to a descendant.
#[derive(Clone)]
pub struct WidgetPath {
    ids: Vec<WidgetId>,
}

impl WidgetPath {
    /// Returns the leaf [`WidgetId`] this path resolves to.
    ///
    /// # Panics
    ///
    /// Panics if the path is empty.
    pub fn get_target(&self) -> WidgetId {
        *self.ids.last().expect("WidgetPath is never empty")
    }

    /// Returns the ids in root-to-leaf order.
    pub fn as_slice(&self) -> &[WidgetId] {
        &self.ids
    }

    /// Returns the number of ids in the path.
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// Returns true if the path contains no ids.
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Returns the leaf widget by resolving the path against `root`.
    pub fn get<'a>(&self, root: &'a dyn Widget) -> Option<&'a dyn Widget> {
        walk_path(root, &self.ids)
    }

    /// Creates a [`WidgetPath`] from a raw id sequence.
    pub fn from_ids(ids: Vec<WidgetId>) -> Self {
        WidgetPath { ids }
    }

    /// Returns the leaf widget mutably by resolving the path against `root`.
    pub fn get_mut<'a>(&self, root: &'a mut dyn Widget) -> Option<&'a mut dyn Widget> {
        walk_path_mut(root, &self.ids)
    }

    /// Calls `f` on every widget in the path whose depth falls within `range`.
    pub fn for_each_mut(
        &self,
        root: &mut dyn Widget,
        range: impl RangeBounds<usize>,
        f: impl FnMut(&mut dyn Widget),
    ) {
        let start = match range.start_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n + 1,
            Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            Bound::Included(&n) => n + 1,
            Bound::Excluded(&n) => n,
            Bound::Unbounded => self.ids.len(),
        };
        for_each_in_path(root, &self.ids, 0, start, end, f);
    }

    /// Calls `f` for each parent-child edge in the path.
    pub fn for_each_edge_mut(
        &self,
        root: &mut dyn Widget,
        f: impl FnMut(&mut dyn Widget, WidgetId),
    ) {
        for_each_edge_in_path(root, &self.ids, 0, f);
    }

    /// Dispatches `event` to every widget along the path.
    pub fn emit_event(&self, root: &mut dyn Widget, event: &mut WidgetEvent) {
        emit_along_path(root, &self.ids, 0, event);
    }
}

fn walk_path<'a>(root: &'a dyn Widget, path: &[WidgetId]) -> Option<&'a dyn Widget> {
    if path.is_empty() || root.get_id() != path[0] {
        return None;
    }
    let mut current = root;
    for &next_id in &path[1..] {
        current = current.get_child(next_id)?;
    }
    Some(current)
}

pub(crate) fn walk_path_mut<'a>(root: &'a mut dyn Widget, path: &[WidgetId]) -> Option<&'a mut dyn Widget> {
    if path.is_empty() || root.get_id() != path[0] {
        return None;
    }
    let mut current = root;
    for &next_id in &path[1..] {
        current = current.get_child_mut(next_id)?;
    }
    Some(current)
}

fn for_each_in_path(
    widget: &mut dyn Widget,
    path: &[WidgetId],
    idx: usize,
    start: usize,
    end: usize,
    mut f: impl FnMut(&mut dyn Widget),
) {
    fn inner(
        widget: &mut dyn Widget,
        path: &[WidgetId],
        idx: usize,
        start: usize,
        end: usize,
        f: &mut dyn FnMut(&mut dyn Widget),
    ) {
        if idx >= path.len() || widget.get_id() != path[idx] {
            return;
        }
        if idx >= start && idx < end {
            f(widget);
        }
        if idx + 1 < path.len()
            && let Some(child) = widget.get_child_mut(path[idx + 1])
        {
            inner(child, path, idx + 1, start, end, f);
        }
    }
    inner(widget, path, idx, start, end, &mut f);
}

fn for_each_edge_in_path(
    widget: &mut dyn Widget,
    path: &[WidgetId],
    idx: usize,
    mut f: impl FnMut(&mut dyn Widget, WidgetId),
) {
    fn inner(
        widget: &mut dyn Widget,
        path: &[WidgetId],
        idx: usize,
        f: &mut dyn FnMut(&mut dyn Widget, WidgetId),
    ) {
        if idx >= path.len() || widget.get_id() != path[idx] {
            return;
        }
        if idx + 1 < path.len() {
            f(widget, path[idx + 1]);
            if let Some(child) = widget.get_child_mut(path[idx + 1]) {
                inner(child, path, idx + 1, f);
            }
        }
    }
    inner(widget, path, idx, &mut f);
}

fn emit_along_path(
    widget: &mut dyn Widget,
    path: &[WidgetId],
    idx: usize,
    event: &mut WidgetEvent,
) {
    if idx >= path.len() || widget.get_id() != path[idx] {
        return;
    }
    if idx + 1 < path.len()
        && let Some(child) = widget.get_child_mut(path[idx + 1])
    {
        emit_along_path(child, path, idx + 1, event);
    }
    widget.on_event(event);
}

pub(crate) fn valid_prefix_len_recursive(parent: &dyn Widget, remaining: &[WidgetId], len: &mut usize) {
    if remaining.is_empty() {
        return;
    }
    if let Some(child) = parent.get_child(remaining[0]) {
        *len += 1;
        valid_prefix_len_recursive(child, &remaining[1..], len);
    }
}

thread_local! {
    static PATH_CACHE: RefCell<HashMap<WidgetId, WidgetPath>> = RefCell::new(HashMap::new());
}

pub(crate) fn clear_path_cache() {
    PATH_CACHE.with_borrow_mut(|cache| cache.clear());
}

/// Per-edge spacing in cells.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Spacing(Vec2<[u8; 2]>);

impl Default for Spacing {
    fn default() -> Self {
        Self::new()
    }
}

impl Spacing {
    /// Creates zero spacing on every edge.
    pub const fn new() -> Self {
        Spacing(Vec2 {
            x: [0, 0],
            y: [0, 0],
        })
    }
    /// Creates equal spacing on each axis scaled to `n` horizontal cells.
    pub const fn balanced(n: u8) -> Self {
        let y = (n + 1) / 3;
        Spacing(Vec2 {
            x: [n, n],
            y: [y, y],
        })
    }

    /// Sets the left spacing.
    pub const fn left(mut self, n: u8) -> Self {
        self.0.x[0] = n;
        self
    }
    /// Sets the right spacing.
    pub const fn right(mut self, n: u8) -> Self {
        self.0.x[1] = n;
        self
    }
    /// Sets the top spacing.
    pub const fn top(mut self, n: u8) -> Self {
        self.0.y[0] = n;
        self
    }
    /// Sets the bottom spacing.
    pub const fn bottom(mut self, n: u8) -> Self {
        self.0.y[1] = n;
        self
    }

    /// Sets both left and right spacing to `n`.
    pub const fn horizontal(mut self, n: u8) -> Self {
        self.0.x = [n, n];
        self
    }
    /// Sets both top and bottom spacing to `n`.
    pub const fn vertical(mut self, n: u8) -> Self {
        self.0.y = [n, n];
        self
    }

    /// Returns the spacing on the leading edge of `axis`.
    pub fn get_before(&self, axis: Axis2D) -> u8 {
        self.0[axis][0]
    }
    /// Returns the spacing on the trailing edge of `axis`.
    pub fn get_after(&self, axis: Axis2D) -> u8 {
        self.0[axis][1]
    }

    /// Returns the total spacing on each axis.
    pub fn get_total(&self) -> Vec2<u16> {
        self.0.map(|[a, b]| a as u16 + b as u16)
    }
}

#[derive(Clone, Copy, Default)]
struct LayoutFlags(u8);

impl LayoutFlags {
    const DIRTY_PAINT: u8 = 1 << 0;
    const DIRTY_LAYOUT: u8 = 1 << 1;
    const OVERFLOWS: u8 = 1 << 2;
    const FLOW_LAYOUT_VALID: u8 = 1 << 3;

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

/// Layout state owned by every [`Widget`].
pub struct Layout {
    id: u64,
    /// Outer rect in window coordinates.
    pub rect: Rect<i32, u16>,
    /// The spacing around each edge.
    pub margin: Spacing,
    /// The default style for rendering.
    pub style: Style,
    /// The flex weight for this widget.
    pub flex: u8,
    /// The per-axis alignment override.
    pub align: AlignOverride,
    pub(crate) explicit_min: Vec2<Option<NonMaxU16>>,
    pub(crate) explicit_max: Vec2<Option<NonMaxU16>>,
    pub(crate) explicit_pref: Vec2<Option<NonMaxU16>>,
    flags: LayoutFlags,
    /// The cached min, max, and preferred sizes.
    pub constraints: Constraints,
    pub(crate) flow_layout: FlowSlot,
    pub(crate) flow_measure: Cell<FlowMeasureCache>,
}

/// Min, max, and preferred sizes a widget reports to its parent.
#[derive(Copy, Clone)]
pub struct Constraints {
    /// The smallest acceptable size.
    pub min_size: Vec2<u16>,
    /// The largest acceptable size.
    pub max_size: Vec2<u16>,
    /// The preferred size.
    pub preferred_size: Vec2<u16>,
}

#[derive(Copy, Clone)]
pub(crate) struct FlowSlot {
    pub input_size: Vec2<u16>,
    pub output_size: Vec2<u16>,
}

pub(crate) const FLOW_MEASURE_CACHE_SIZE: usize = 4;

#[derive(Copy, Clone)]
pub(crate) struct FlowMeasureCache {
    pub entries: [FlowSlot; FLOW_MEASURE_CACHE_SIZE],
    pub len: u8,
}

impl FlowMeasureCache {
    /// Returns the cached output for `input_size`, or `None` when not cached.
    pub fn find_and_promote(&mut self, input_size: Vec2<u16>) -> Option<Vec2<u16>> {
        let len = self.len as usize;
        for i in 0..len {
            if self.entries[i].input_size == input_size {
                let entry = self.entries[i];
                if i != len - 1 {
                    for j in i..len - 1 {
                        self.entries[j] = self.entries[j + 1];
                    }
                    self.entries[len - 1] = entry;
                }
                return Some(entry.output_size);
            }
        }
        None
    }

    /// Inserts or updates the entry for `input_size`.
    pub fn insert(&mut self, input_size: Vec2<u16>, output_size: Vec2<u16>) {
        let entry = FlowSlot { input_size, output_size };
        for i in 0..self.len as usize {
            if self.entries[i].input_size == input_size {
                for j in i..self.len as usize - 1 {
                    self.entries[j] = self.entries[j + 1];
                }
                self.entries[self.len as usize - 1] = entry;
                return;
            }
        }
        let len = self.len as usize;
        if len < FLOW_MEASURE_CACHE_SIZE {
            self.entries[len] = entry;
            self.len += 1;
        } else {
            for i in 0..FLOW_MEASURE_CACHE_SIZE - 1 {
                self.entries[i] = self.entries[i + 1];
            }
            self.entries[FLOW_MEASURE_CACHE_SIZE - 1] = entry;
        }
    }

    /// Removes all entries.
    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// Returns the output size of the most recently inserted entry.
    pub fn get_last_output(&self) -> Vec2<u16> {
        if self.len == 0 {
            Vec2::of(0)
        } else {
            self.entries[self.len as usize - 1].output_size
        }
    }
}

impl Default for Layout {
    fn default() -> Self {
        Self::new()
    }
}

impl Layout {
    /// Creates a [`Layout`] with a fresh widget id.
    pub fn new() -> Self {
        Self {
            id: next_widget_id(),
            rect: Rect::new(Vec2::of(0), Vec2::of(0)),
            margin: Spacing::new(),
            style: Style::new(),
            flex: 0,
            align: AlignOverride::new(),
            explicit_min: Vec2::of(None),
            explicit_max: Vec2::of(None),
            explicit_pref: Vec2::of(None),
            flags: LayoutFlags(LayoutFlags::DIRTY_LAYOUT),
            constraints: Constraints {
                min_size: Vec2::of(0),
                max_size: Vec2::of(u16::MAX),
                preferred_size: Vec2::of(0),
            },
            flow_layout: FlowSlot {
                input_size: Vec2::of(0),
                output_size: Vec2::of(0),
            },
            flow_measure: Cell::new(FlowMeasureCache {
                entries: [FlowSlot {
                    input_size: Vec2::of(0),
                    output_size: Vec2::of(0),
                }; FLOW_MEASURE_CACHE_SIZE],
                len: 0,
            }),
        }
    }

    pub(crate) fn flow_measure_find(&self, input_size: Vec2<u16>) -> Option<Vec2<u16>> {
        let mut cache = self.flow_measure.get();
        let output = cache.find_and_promote(input_size);
        self.flow_measure.set(cache);
        output
    }

    pub(crate) fn flow_measure_insert(&self, input_size: Vec2<u16>, output_size: Vec2<u16>) {
        let mut cache = self.flow_measure.get();
        cache.insert(input_size, output_size);
        self.flow_measure.set(cache);
    }

    fn get_flow_measure_last(&self) -> Vec2<u16> {
        self.flow_measure.get().get_last_output()
    }

    pub(crate) fn flow_lookup_by_main(&self, axis: Axis2D, main: u16) -> Option<Vec2<u16>> {
        if self.flags.get(LayoutFlags::FLOW_LAYOUT_VALID)
            && self.flow_layout.input_size[axis] == main
        {
            return Some(self.flow_layout.output_size);
        }
        let cache = self.flow_measure.get();
        let len = cache.len as usize;
        for i in (0..len).rev() {
            if cache.entries[i].input_size[axis] == main {
                return Some(cache.entries[i].output_size);
            }
        }
        None
    }

    /// Returns the current [`DirtyImpact`].
    pub fn get_dirty(&self) -> DirtyImpact {
        if self.flags.get(LayoutFlags::DIRTY_LAYOUT) {
            DirtyImpact::Layout
        } else if self.flags.get(LayoutFlags::DIRTY_PAINT) {
            DirtyImpact::Paint
        } else {
            DirtyImpact::None
        }
    }

    /// Sets the dirty state to `impact`.
    pub fn set_dirty(&mut self, impact: DirtyImpact) {
        self.flags.set(LayoutFlags::DIRTY_PAINT, impact == DirtyImpact::Paint);
        self.flags.set(LayoutFlags::DIRTY_LAYOUT, impact == DirtyImpact::Layout);
        if impact == DirtyImpact::Layout {
            self.invalidate_flow_caches();
        }
    }

    /// Raises the dirty state to at least `impact`.
    pub fn or_dirty(&mut self, impact: DirtyImpact) {
        match impact {
            DirtyImpact::None => {}
            DirtyImpact::Paint => self.flags.set(LayoutFlags::DIRTY_PAINT, true),
            DirtyImpact::Layout => {
                self.flags.set(LayoutFlags::DIRTY_LAYOUT, true);
                self.invalidate_flow_caches();
            }
        }
    }

    fn invalidate_flow_caches(&mut self) {
        self.flags.set(LayoutFlags::FLOW_LAYOUT_VALID, false);
        self.flow_measure.get_mut().clear();
    }

    /// Returns true when the widget overflowed its allocated size.
    pub fn is_overflowing(&self) -> bool {
        self.flags.get(LayoutFlags::OVERFLOWS)
    }

    /// Sets whether the widget overflowed during layout.
    pub fn set_overflowing(&mut self, value: bool) {
        self.flags.set(LayoutFlags::OVERFLOWS, value);
    }

    /// Returns the explicit minimum on `axis`, or `None` when unset.
    pub fn get_explicit_min(&self, axis: Axis2D) -> Option<u16> {
        self.explicit_min[axis].map(Into::into)
    }

    /// Returns the explicit maximum on `axis`, or `None` when unset.
    pub fn get_explicit_max(&self, axis: Axis2D) -> Option<u16> {
        self.explicit_max[axis].map(Into::into)
    }

    /// Returns the explicit preferred size on `axis`, or `None` when unset.
    pub fn get_explicit_pref(&self, axis: Axis2D) -> Option<u16> {
        self.explicit_pref[axis].map(Into::into)
    }

    /// Sets the explicit minimum on `axis`.
    pub fn set_explicit_min(&mut self, axis: Axis2D, value: Option<u16>) {
        let value = value.and_then(NonMaxU16::new);
        if self.explicit_min[axis] != value {
            self.explicit_min[axis] = value;
            self.or_dirty(DirtyImpact::Layout);
        }
    }

    /// Sets the explicit maximum on `axis`.
    pub fn set_explicit_max(&mut self, axis: Axis2D, value: Option<u16>) {
        let value = value.and_then(NonMaxU16::new);
        if self.explicit_max[axis] != value {
            self.explicit_max[axis] = value;
            self.or_dirty(DirtyImpact::Layout);
        }
    }

    /// Sets the explicit preferred size on `axis`.
    pub fn set_explicit_pref(&mut self, axis: Axis2D, value: Option<u16>) {
        let value = value.and_then(NonMaxU16::new);
        if self.explicit_pref[axis] != value {
            self.explicit_pref[axis] = value;
            self.or_dirty(DirtyImpact::Layout);
        }
    }

    /// Returns the leading margin on each axis.
    pub fn get_margin_before(&self) -> Vec2<u16> {
        Axis2D::map(|a| self.margin.get_before(a) as u16)
    }

    /// Returns the trailing margin on each axis.
    pub fn get_margin_after(&self) -> Vec2<u16> {
        Axis2D::map(|a| self.margin.get_after(a) as u16)
    }

    /// Returns the total margin on each axis.
    pub fn get_margin_total(&self) -> Vec2<u16> {
        self.margin.get_total()
    }

    /// Returns the content size plus total margin on each axis.
    pub fn get_outer_size(&self) -> Vec2<u16> {
        let margin_total = self.margin.get_total();
        Axis2D::map(|a| self.rect.size[a].saturating_add(margin_total[a]))
    }

    /// Returns the cursor shape and position of `child` relative to this widget's content area.
    pub fn get_child_cursor(
        &self,
        child: &dyn Widget,
        selected_id: WidgetId,
    ) -> Option<(CursorShape, Vec2<i32>)> {
        let cursor = if child.get_id() == selected_id {
            child.get_cursor(None)
        } else {
            child.get_cursor(Some(selected_id))
        };
        let (shape, cursor_pos) = cursor?;
        let child_cp = child.get_pos();
        let self_cp = self.rect.pos;
        let pos = cursor_pos + child_cp - self_cp;
        let size = self.rect.size.map(|v| v as i32);
        let reject_x = pos.x < -1 || pos.x > size.x;
        let reject_y = pos.y < -1 || pos.y > size.y;
        if reject_x || reject_y {
            return None;
        }
        Some((shape, pos))
    }
}

/// Interaction state of a widget, used to drive styling and event routing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum WidgetState {
    /// Neither focused nor hovered.
    None,
    /// Pointer is over the widget.
    Hover,
    /// Widget is the focused leaf.
    Focused,
    /// Widget is the focused leaf and the pointer is over it.
    FocusedHover,
    /// The widget is being actively manipulated.
    Active,
}

impl std::fmt::Display for WidgetState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::Hover => write!(f, "Hover"),
            Self::Focused => write!(f, "Focused"),
            Self::FocusedHover => write!(f, "FocusedHover"),
            Self::Active => write!(f, "Active"),
        }
    }
}

/// A type-erased message published by a widget.
#[non_exhaustive]
pub struct WidgetEvent {
    /// Id of the widget that emitted the event.
    pub source: WidgetId,
    /// The type-erased event payload.
    pub payload: Box<dyn std::any::Any>,
}

impl WidgetEvent {
    /// Creates an event with the given source id and payload.
    pub fn new(
        source: WidgetId,
        payload: Box<dyn std::any::Any>,
    ) -> Self {
        Self { source, payload }
    }

    /// Returns the payload as `T`, consuming it, or `None` when the type does not match.
    pub fn take<T: 'static>(&mut self) -> Option<T> {
        if !self.payload.is::<T>() {
            return None;
        }
        let stolen = std::mem::replace(&mut self.payload, Box::new(()));
        match stolen.downcast::<T>() {
            Ok(b) => Some(*b),
            Err(b) => {
                self.payload = b;
                None
            }
        }
    }

    /// Returns true when the event was emitted by `source` widget.
    pub fn by<U: ?Sized>(&self, source: WidgetId<U>) -> bool {
        self.source == source
    }

    /// Returns a reference to the payload as `T`, or `None` when the type does not match.
    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.payload.downcast_ref::<T>()
    }

    /// Returns true when the payload is of type `T`.
    pub fn of<T: 'static>(&self) -> bool {
        self.payload.is::<T>()
    }

    /// Returns true when the payload is of type `T` and `source` matches.
    pub fn of_by<T: 'static>(&self, source: WidgetId<impl ?Sized>) -> bool {
        self.source == source && self.payload.is::<T>()
    }

    /// Returns a reference to the payload as `T` when `source` matches and the type aligns.
    pub fn get_by<T: 'static>(&self, source: WidgetId<impl ?Sized>) -> Option<&T> {
        if self.source == source {
            self.payload.downcast_ref::<T>()
        } else {
            None
        }
    }
}

/// An adapter that delegates [`Widget`] methods to an inner widget.
pub trait DelegateWidget: 'static {
    /// Returns the inner delegate widget.
    fn get_delegate(&self) -> &dyn Widget;
    /// Returns the inner delegate widget mutably.
    fn get_delegate_mut(&mut self) -> &mut dyn Widget;

    /// Override hook for [`Widget::get_identity`].
    fn override_get_identity(&self) -> WidgetId {
        self.get_delegate().get_identity()
    }

    /// Override hook for [`Widget::render`].
    fn override_render(&self, ctx: RenderContext) {
        self.get_delegate().render(ctx);
    }

    /// Override hook for [`Widget::get_layer`].
    fn override_get_layer(&self) -> Layer {
        self.get_delegate().get_layer()
    }

    /// Override hook for [`Widget::get_flow_axis`].
    fn override_get_flow_axis(&self) -> Axis2D {
        self.get_delegate().get_flow_axis()
    }

    /// Override hook for [`Widget::find_descendant`].
    fn override_find_descendant(
        &self,
        predicate: &dyn Fn(&dyn Widget) -> bool,
        path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        self.get_delegate().find_descendant(predicate, path)
    }

    /// Override hook for [`Widget::descendant_at_pos`].
    fn override_descendant_at_pos(
        &self,
        pos: Vec2<f32>,
        path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        self.get_delegate().descendant_at_pos(pos, path)
    }

    /// Override hook for [`Widget::find_descendant_at_pos`].
    fn override_find_descendant_at_pos(
        &self,
        pos: Vec2<f32>,
        predicate: &dyn Fn(&dyn Widget) -> bool,
        path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        self.get_delegate().find_descendant_at_pos(pos, predicate, path)
    }

    /// Override hook for [`Widget::get_subcell_offset`].
    fn override_get_subcell_offset(&self) -> Vec2<f32> {
        self.get_delegate().get_subcell_offset()
    }

    /// Override hook for [`Widget::each_child`].
    fn override_each_child(
        &self,
        f: &mut dyn FnMut(&dyn Widget),
        direction: Sign,
    ) {
        self.get_delegate().each_child(f, direction);
    }

    /// Override hook for [`Widget::each_child_mut`].
    fn override_each_child_mut(
        &mut self,
        f: &mut dyn FnMut(&mut dyn Widget),
        direction: Sign,
    ) {
        self.get_delegate_mut().each_child_mut(f, direction);
    }

    /// Override hook for [`Widget::get_child`].
    fn override_get_child(&self, id: WidgetId) -> Option<&dyn Widget> {
        self.get_delegate().get_child(id)
    }

    /// Override hook for [`Widget::get_child_mut`].
    fn override_get_child_mut(&mut self, id: WidgetId) -> Option<&mut dyn Widget> {
        self.get_delegate_mut().get_child_mut(id)
    }

    /// Override hook for [`Widget::before_layout`].
    fn override_before_layout(&mut self) {
        self.get_delegate_mut().before_layout();
    }

    /// Override hook for [`Widget::after_layout`].
    fn override_after_layout(&mut self) {
        self.get_delegate_mut().after_layout();
    }

    /// Override hook for [`Widget::measure_constraints`].
    fn override_measure_constraints(&mut self) -> Constraints {
        self.get_delegate_mut().measure_constraints()
    }

    /// Override hook for [`Widget::layout_flow`].
    fn override_layout_flow(&mut self, allocated: Vec2<u16>) -> Vec2<u16> {
        self.get_delegate_mut().layout_flow(allocated)
    }

    /// Override hook for [`Widget::layout_measure`].
    fn override_layout_measure(&self, allocated: Vec2<u16>) -> Vec2<u16> {
        self.get_delegate().layout_measure(allocated)
    }

    /// Override hook for [`Widget::layout_position`].
    fn override_layout_position(&mut self) {
        self.get_delegate_mut().layout_position();
    }

    /// Override hook for [`Widget::on_event`].
    fn override_on_event(&mut self, event: &mut WidgetEvent) {
        self.get_delegate_mut().on_event(event);
    }

    /// Override hook for [`Widget::can_scroll`].
    fn override_can_scroll(&self, direction: Direction2D) -> bool {
        self.get_delegate().can_scroll(direction)
    }

    /// Override hook for [`Widget::get_scroll_clip_range`].
    fn override_get_scroll_clip_range(&self) -> Vec2<Option<(i32, i32)>> {
        self.get_delegate().get_scroll_clip_range()
    }

    /// Override hook for [`Widget::is_focusable`].
    fn override_is_focusable(&self) -> bool {
        self.get_delegate().is_focusable()
    }

    /// Override hook for [`Widget::get_focus_target`].
    fn override_get_focus_target(&self) -> Option<WidgetId> {
        self.get_delegate().get_focus_target()
    }

    /// Override hook for [`Widget::on_input`].
    fn override_on_input(&mut self, queue: &mut InputQueue) -> InputResult {
        self.get_delegate_mut().on_input(queue)
    }

    /// Override hook for [`Widget::on_state_change`].
    fn override_on_state_change(&mut self, state: WidgetState) {
        self.get_delegate_mut().on_state_change(state);
    }

    /// Override hook for [`Widget::get_cursor`].
    fn override_get_cursor(
        &self,
        selected: Option<WidgetId>,
    ) -> Option<(CursorShape, Vec2<i32>)> {
        self.get_delegate().get_cursor(selected)
    }

    /// Override hook for [`Widget::reveal`].
    fn override_reveal(
        &mut self,
        child: Option<WidgetId>,
        revelation: &mut Revelation,
        align: Vec2<Option<Align>>,
    ) {
        self.get_delegate_mut().reveal(child, revelation, align);
    }

    /// Override hook for [`Widget::before_focus_move`].
    fn override_before_focus_move(
        &mut self,
        selected_child: WidgetId,
        axis: Option<Axis2D>,
        direction: Sign,
    ) {
        self.get_delegate_mut().before_focus_move(selected_child, axis, direction);
    }

    /// Called after [`Widget::before_layout`] on the delegate.
    fn after_before_layout(&mut self) {}

    /// Called after [`Widget::after_layout`] on the delegate.
    fn after_after_layout(&mut self) {}

    /// Called after [`Widget::measure_constraints`] on the delegate.
    fn after_measure_constraints(&mut self, _result: &Constraints) {}

    /// Called after [`Widget::layout_flow`] on the delegate.
    fn after_layout_flow(&mut self, _allocated: Vec2<u16>, _result: Vec2<u16>) {}

    /// Called after [`Widget::layout_position`] on the delegate.
    fn after_layout_position(&mut self) {}

    /// Called after [`Widget::on_event`] on the delegate.
    fn after_on_event(&mut self, _event: &mut WidgetEvent) {}

    /// Called after [`Widget::on_input`] on the delegate.
    fn after_on_input(&mut self, _result: InputResult) {}

    /// Called after [`Widget::on_state_change`] on the delegate.
    fn after_on_state_change(&mut self, _state: WidgetState) {}

    /// Called after [`Widget::reveal`] on the delegate.
    fn after_reveal(
        &mut self,
        _child: Option<WidgetId>,
        _revelation: &Revelation,
        _align: Vec2<Option<Align>>,
    ) {}

    /// Called after [`Widget::before_focus_move`] on the delegate.
    fn after_before_focus_move(
        &mut self,
        _selected_child: WidgetId,
        _axis: Option<Axis2D>,
        _direction: Sign,
    ) {}
}

impl<T: DelegateWidget> Widget for T {
    fn get_layout(&self) -> &Layout {
        self.get_delegate().get_layout()
    }

    fn get_layout_mut(&mut self) -> &mut Layout {
        self.get_delegate_mut().get_layout_mut()
    }

    fn get_identity(&self) -> WidgetId {
        self.override_get_identity()
    }

    fn get_name(&self) -> &'static str {
        let name = std::any::type_name::<T>();
        match name.rfind(':') {
            Some(i) => &name[i + 1..],
            None => name,
        }
    }

    fn render(&self, ctx: RenderContext) {
        self.override_render(ctx);
    }

    fn get_layer(&self) -> Layer {
        self.override_get_layer()
    }

    fn get_flow_axis(&self) -> Axis2D {
        self.override_get_flow_axis()
    }

    fn find_descendant(
        &self,
        predicate: &dyn Fn(&dyn Widget) -> bool,
        path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        self.override_find_descendant(predicate, path)
    }

    fn descendant_at_pos(
        &self,
        pos: Vec2<f32>,
        path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        self.override_descendant_at_pos(pos, path)
    }

    fn find_descendant_at_pos(
        &self,
        pos: Vec2<f32>,
        predicate: &dyn Fn(&dyn Widget) -> bool,
        path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        self.override_find_descendant_at_pos(pos, predicate, path)
    }

    fn get_subcell_offset(&self) -> Vec2<f32> {
        self.override_get_subcell_offset()
    }

    fn each_child(
        &self,
        f: &mut dyn FnMut(&dyn Widget),
        direction: Sign,
    ) {
        self.override_each_child(f, direction);
    }

    fn each_child_mut(
        &mut self,
        f: &mut dyn FnMut(&mut dyn Widget),
        direction: Sign,
    ) {
        self.override_each_child_mut(f, direction);
    }

    fn get_child(&self, id: WidgetId) -> Option<&dyn Widget> {
        self.override_get_child(id)
    }

    fn get_child_mut(&mut self, id: WidgetId) -> Option<&mut dyn Widget> {
        self.override_get_child_mut(id)
    }

    fn before_layout(&mut self) {
        self.override_before_layout();
        self.after_before_layout();
    }

    fn after_layout(&mut self) {
        self.override_after_layout();
        self.after_after_layout();
    }

    fn measure_constraints(&mut self) -> Constraints {
        let out = self.override_measure_constraints();
        self.after_measure_constraints(&out);
        out
    }

    fn layout_flow(&mut self, allocated: Vec2<u16>) -> Vec2<u16> {
        let out = self.override_layout_flow(allocated);
        self.after_layout_flow(allocated, out);
        out
    }

    fn layout_measure(&self, allocated: Vec2<u16>) -> Vec2<u16> {
        self.override_layout_measure(allocated)
    }

    fn layout_position(&mut self) {
        self.override_layout_position();
        self.after_layout_position();
    }

    fn on_event(&mut self, event: &mut WidgetEvent) {
        self.override_on_event(event);
        self.after_on_event(event);
    }

    fn can_scroll(&self, direction: Direction2D) -> bool {
        self.override_can_scroll(direction)
    }

    fn get_scroll_clip_range(&self) -> Vec2<Option<(i32, i32)>> {
        self.override_get_scroll_clip_range()
    }

    fn is_focusable(&self) -> bool {
        self.override_is_focusable()
    }

    fn get_focus_target(&self) -> Option<WidgetId> {
        if self.is_focusable() {
            Some(self.get_id().untyped())
        } else {
            self.override_get_focus_target()
        }
    }

    fn on_input(&mut self, queue: &mut InputQueue) -> InputResult {
        let out = self.override_on_input(queue);
        self.after_on_input(out);
        out
    }

    fn on_state_change(&mut self, state: WidgetState) {
        self.override_on_state_change(state);
        self.after_on_state_change(state);
    }

    fn get_cursor(
        &self,
        selected: Option<WidgetId>,
    ) -> Option<(CursorShape, Vec2<i32>)> {
        self.override_get_cursor(selected)
    }

    fn reveal(
        &mut self,
        child: Option<WidgetId>,
        revelation: &mut Revelation,
        align: Vec2<Option<Align>>,
    ) {
        self.override_reveal(child, revelation, align);
        self.after_reveal(child, revelation, align);
    }

    fn before_focus_move(
        &mut self,
        selected_child: WidgetId,
        axis: Option<Axis2D>,
        direction: Sign,
    ) {
        self.override_before_focus_move(selected_child, axis, direction);
        self.after_before_focus_move(selected_child, axis, direction);
    }
}

/// Updates `child`'s [`Constraints`].
pub fn constrain_child(child: &mut dyn Widget) {
    if child.get_layout().get_dirty() != DirtyImpact::Layout {
        return;
    }
    let c = child.measure_constraints();
    let layout = child.get_layout();
    let margin = layout.get_margin_total();
    let mut min = c.min_size;
    let mut max = c.max_size;
    let mut pref = c.preferred_size;
    for a in [Axis2D::X, Axis2D::Y] {
        if let Some(v) = layout.get_explicit_min(a) {
            min[a] = std::cmp::max(min[a], v);
        }
        if let Some(v) = layout.get_explicit_max(a) {
            max[a] = v;
        }
        if let Some(v) = layout.get_explicit_pref(a) {
            pref[a] = v;
        }
        min[a] = min[a].min(max[a]);
        pref[a] = pref[a].clamp(min[a], max[a]);
        min[a] = min[a].saturating_add(margin[a]);
        max[a] = max[a].saturating_add(margin[a]);
        pref[a] = pref[a].saturating_add(margin[a]);
    }
    let constraints = &mut child.get_layout_mut().constraints;
    constraints.min_size = min;
    constraints.max_size = max;
    constraints.preferred_size = pref;
}

/// Runs the layout-pass flow for `child` against `size` and returns the outer output size.
pub fn flow_child(child: &mut dyn Widget, size: Vec2<u16>) -> Vec2<u16> {
    let layout = child.get_layout();
    let margin = layout.get_margin_total();
    let content_alloc = Axis2D::map(|a| size[a].saturating_sub(margin[a]));
    let hit = layout.flags.get(LayoutFlags::FLOW_LAYOUT_VALID)
        && content_alloc == layout.flow_layout.input_size;

    if hit {
        let content_out = layout.flow_layout.output_size;
        child.set_rect_size(content_alloc);
        return Axis2D::map(|a| content_out[a].saturating_add(margin[a]));
    }

    child.set_rect_size(content_alloc);

    let layout = child.get_layout_mut();
    layout.flow_layout.input_size = content_alloc;
    layout.flags.set(LayoutFlags::FLOW_LAYOUT_VALID, false);

    let content_out = child.layout_flow(content_alloc);
    let layout_ro = child.get_layout();
    let content_out = Axis2D::map(|a| {
        let mut v = content_out[a];
        if let Some(min) = layout_ro.get_explicit_min(a) {
            v = v.max(min);
        }
        if let Some(max) = layout_ro.get_explicit_max(a) {
            v = v.min(max);
        }
        v
    });

    let layout = child.get_layout_mut();
    layout.flow_layout.output_size = content_out;
    layout.flags.set(LayoutFlags::FLOW_LAYOUT_VALID, true);
    layout.set_dirty(DirtyImpact::None);
    Axis2D::map(|a| content_out[a].saturating_add(margin[a]))
}

/// Runs the measure-pass flow for `child` against `size` and returns the outer output size.
pub fn measure_child(child: &dyn Widget, size: Vec2<u16>) -> Vec2<u16> {
    let layout = child.get_layout();
    let margin = layout.get_margin_total();
    let content_alloc = Axis2D::map(|a| size[a].saturating_sub(margin[a]));
    if let Some(content_out) = layout.flow_measure_find(content_alloc) {
        return Axis2D::map(|a| content_out[a].saturating_add(margin[a]));
    }

    let content_out = child.layout_measure(content_alloc);
    let content_out = Axis2D::map(|a| {
        let mut v = content_out[a];
        if let Some(min) = layout.get_explicit_min(a) {
            v = v.max(min);
        }
        if let Some(max) = layout.get_explicit_max(a) {
            v = v.min(max);
        }
        v
    });
    layout.flow_measure_insert(content_alloc, content_out);
    Axis2D::map(|a| content_out[a].saturating_add(margin[a]))
}

/// Returns the last layout output size including margin.
pub fn flow_output_size(layout: &Layout) -> Vec2<u16> {
    let margin = layout.get_margin_total();
    let content_out = layout.flow_layout.output_size;
    Axis2D::map(|a| content_out[a].saturating_add(margin[a]))
}

/// Returns the last measure output size including margin.
pub fn measure_output_size(layout: &Layout) -> Vec2<u16> {
    let margin = layout.get_margin_total();
    let content_out = layout.get_flow_measure_last();
    Axis2D::map(|a| content_out[a].saturating_add(margin[a]))
}

/// Core trait implemented by every UI element.
pub trait Widget: std::any::Any {
    /// Returns the widget's [`Layout`].
    fn get_layout(&self) -> &Layout;
    /// Returns the widget's [`Layout`] mutably.
    fn get_layout_mut(&mut self) -> &mut Layout;

    /// Returns this widget's stable identity.
    fn get_identity(&self) -> WidgetId {
        WidgetId(self.get_layout().id, PhantomData)
    }

    /// Returns the short type name.
    fn get_name(&self) -> &'static str {
        "Widget"
    }

    /// Renders this widget into `ctx`.
    fn render(&self, _ctx: RenderContext) {}

    /// Returns the render [`Layer`] for this widget.
    fn get_layer(&self) -> Layer {
        Layer::Bottom
    }

    /// Returns the axis along which content flows.
    fn get_flow_axis(&self) -> Axis2D {
        Axis2D::Y
    }

    /// Returns the first descendant for which `predicate` returns true.
    fn find_descendant(
        &self,
        _predicate: &dyn Fn(&dyn Widget) -> bool,
        _path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        None
    }

    /// Returns the descendant whose rect contains `pos`.
    fn descendant_at_pos(
        &self,
        _pos: Vec2<f32>,
        _path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        None
    }

    /// Returns the descendant whose rect contains `pos` and for which `predicate` returns true.
    fn find_descendant_at_pos(
        &self,
        _pos: Vec2<f32>,
        _predicate: &dyn Fn(&dyn Widget) -> bool,
        _path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        None
    }

    /// Returns the render-time shift of this widget's content in cell fractions.
    fn get_subcell_offset(&self) -> Vec2<f32> {
        Vec2::of(0.0)
    }

    /// Calls `f` on each direct child in the order implied by `direction`.
    fn each_child(
        &self,
        _f: &mut dyn FnMut(&dyn Widget),
        _direction: Sign,
    ) {}

    /// Calls `f` on each direct child mutably in the order implied by `direction`.
    fn each_child_mut(
        &mut self,
        _f: &mut dyn FnMut(&mut dyn Widget),
        _direction: Sign,
    ) {}

    /// Returns the direct child with `id`.
    fn get_child(&self, id: WidgetId) -> Option<&dyn Widget> {
        let mut result: Option<*const dyn Widget> = None;
        self.each_child(&mut |child| {
            if result.is_none() && child.get_id() == id {
                result = Some(child as *const dyn Widget);
            }
        }, Sign::Positive);
        result.map(|ptr| unsafe { &*ptr })
    }

    /// Returns the direct child with `id` mutably.
    fn get_child_mut(&mut self, id: WidgetId) -> Option<&mut dyn Widget> {
        let mut result: Option<*mut dyn Widget> = None;
        self.each_child_mut(&mut |child| {
            if result.is_none() && child.get_id() == id {
                result = Some(child as *mut dyn Widget);
            }
        }, Sign::Positive);
        result.map(|ptr| unsafe { &mut *ptr })
    }

    /// Called before layout.
    fn before_layout(&mut self) {}

    /// Called after layout.
    fn after_layout(&mut self) {}

    /// Returns this widget's [`Constraints`].
    fn measure_constraints(&mut self) -> Constraints {
        self.each_child_mut(&mut constrain_child, Sign::Positive);
        Constraints {
            min_size: self.get_layout().get_margin_total(),
            max_size: Vec2::of(u16::MAX),
            preferred_size: Vec2::of(u16::MAX),
        }
    }

    /// Lays the widget out within `allocated` cells and returns the content size used.
    fn layout_flow(&mut self, _allocated: Vec2<u16>) -> Vec2<u16> {
        let slot = self.get_layout().constraints.min_size;
        let margin = self.get_layout().get_margin_total();
        Axis2D::map(|a| slot[a].saturating_sub(margin[a]))
    }

    /// Returns the content size for `allocated`.
    fn layout_measure(&self, _allocated: Vec2<u16>) -> Vec2<u16> {
        let slot = self.get_layout().constraints.min_size;
        let margin = self.get_layout().get_margin_total();
        Axis2D::map(|a| slot[a].saturating_sub(margin[a]))
    }

    /// Positions children.
    fn layout_position(&mut self) {}

    /// Handles an event dispatched to this widget.
    fn on_event(&mut self, _event: &mut WidgetEvent) {}

    /// Returns true when this widget can scroll along `direction`.
    fn can_scroll(&self, _direction: Direction2D) -> bool {
        false
    }

    /// Returns the scroll clip range on each axis, or `None` for unclipped axes.
    fn get_scroll_clip_range(&self) -> Vec2<Option<(i32, i32)>> {
        Vec2::of(None)
    }

    /// Returns true when this widget can receive focus.
    fn is_focusable(&self) -> bool {
        false
    }

    /// Returns the id of the widget that receives focus.
    fn get_focus_target(&self) -> Option<WidgetId> {
        None
    }

    /// Processes input from `queue` and returns the result.
    fn on_input(&mut self, _queue: &mut InputQueue) -> InputResult {
        InputResult::Rejected
    }

    /// Called when the widget's [`WidgetState`] changes.
    fn on_state_change(&mut self, _state: WidgetState) {}

    /// Returns the cursor shape and position for this widget.
    fn get_cursor(
        &self,
        _selected: Option<WidgetId>,
    ) -> Option<(CursorShape, Vec2<i32>)> {
        None
    }

    /// Reveals the rects in `revelation`.
    fn reveal(
        &mut self,
        _child: Option<WidgetId>,
        _revelation: &mut Revelation,
        _align: Vec2<Option<Align>>,
    ) {}

    /// Called before focus moves between children.
    fn before_focus_move(
        &mut self,
        _selected_child: WidgetId,
        _axis: Option<Axis2D>,
        _direction: Sign,
    ) {}
}

/// Convenience methods on every [`Widget`].
pub trait WidgetMethods: Widget {
    /// Returns this widget's typed [`WidgetId`].
    fn get_id(&self) -> WidgetId<Self> {
        WidgetId(self.get_identity().0, PhantomData)
    }

    /// Builder form of [`WidgetMethods::get_id`] that stores the id into `slot`.
    fn id(self: Box<Self>, slot: &mut WidgetId<Self>) -> Box<Self>
    where
        Self: Sized,
    {
        *slot = self.get_id();
        self
    }

    /// Marks the widget for re-layout.
    fn dirty_layout(&mut self) {
        self.get_layout_mut().or_dirty(DirtyImpact::Layout);
    }

    /// Marks the widget for repaint.
    fn dirty_paint(&mut self) {
        self.get_layout_mut().or_dirty(DirtyImpact::Paint);
    }

    /// Returns the widget's current style.
    fn get_style(&self) -> crate::render::style::Style {
        self.get_layout().style
    }

    /// Sets the widget's style.
    fn set_style(&mut self, style: crate::render::style::Style) {
        self.get_layout_mut().style = style;
        self.dirty_paint();
    }

    /// Returns the deepest descendant at `pos` that can scroll along `direction`.
    fn find_scrollable_at_pos(
        &self,
        pos: Vec2<f32>,
        direction: Direction2D,
    ) -> Option<WidgetId> {
        self.find_descendant_at_pos(
            pos,
            &|child| child.can_scroll(direction),
            None,
        )
    }

    /// Returns the center point of the widget's outer rect.
    fn get_center_pos(&self) -> Vec2<i32> {
        let pos = self.get_pos();
        let size = self.get_rect_size();
        Axis2D::map(|a| pos[a] + size[a] as i32 / 2)
    }

    /// Calls `f` on every descendant.
    fn for_each_child_recursive(
        &self,
        f: &mut dyn FnMut(&dyn Widget),
        direction: Sign,
    ) {
        self.each_child(
            &mut |child| {
                f(child);
                child.for_each_child_recursive(f, direction);
            },
            direction,
        );
    }

    /// Writes a debug representation of this widget.
    fn debug(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.get_name())
    }

    /// Returns true when this widget is the focused leaf.
    fn is_focused(&self) -> bool {
        super::runtime::is_focused(self.get_id())
    }

    /// Returns true when this widget is anywhere on the current focus chain.
    fn in_focus_chain(&self) -> bool {
        super::runtime::in_focus_chain(self.get_id())
    }

    /// Returns the content size.
    fn get_rect_size(&self) -> Vec2<u16> {
        self.get_layout().rect.size
    }

    /// Returns the content size plus total margin on each axis.
    fn get_outer_size(&self) -> Vec2<u16> {
        self.get_layout().get_outer_size()
    }

    /// Sets the content size.
    fn set_rect_size(&mut self, size: Vec2<u16>) {
        self.get_layout_mut().rect.size = size;
    }

    /// Returns the content width.
    fn get_rect_width(&self) -> u16 {
        self.get_rect_size().x
    }

    /// Returns the content height.
    fn get_rect_height(&self) -> u16 {
        self.get_rect_size().y
    }

    /// Sets the content width.
    fn set_rect_width(&mut self, w: u16) {
        self.get_layout_mut().rect.size.x = w;
    }

    /// Sets the content height.
    fn set_rect_height(&mut self, h: u16) {
        self.get_layout_mut().rect.size.y = h;
    }

    /// Returns the top-left position of the widget.
    fn get_pos(&self) -> Vec2<i32> {
        self.get_layout().rect.pos
    }

    /// Sets the top-left position.
    fn set_pos(&mut self, pos: Vec2<i32>) {
        self.get_layout_mut().rect.pos = pos;
    }

    /// Returns the content rect of the widget.
    fn get_rect(&self) -> Rect<i32, u16> {
        self.get_layout().rect
    }

    /// Sets both position and size.
    fn set_rect(&mut self, rect: Rect<i32, u16>) {
        self.get_layout_mut().rect = rect;
    }

    /// Returns the widget's margin.
    fn get_margin(&self) -> Spacing {
        self.get_layout().margin
    }

    /// Sets the widget's margin.
    fn set_margin(&mut self, margin: Spacing) {
        self.get_layout_mut().margin = margin;
        self.dirty_layout();
    }

    /// Returns the left margin in cells.
    fn get_margin_left(&self) -> u8 {
        self.get_margin().get_before(Axis2D::X)
    }

    /// Returns the right margin in cells.
    fn get_margin_right(&self) -> u8 {
        self.get_margin().get_after(Axis2D::X)
    }

    /// Returns the top margin in cells.
    fn get_margin_top(&self) -> u8 {
        self.get_margin().get_before(Axis2D::Y)
    }

    /// Returns the bottom margin in cells.
    fn get_margin_bottom(&self) -> u8 {
        self.get_margin().get_after(Axis2D::Y)
    }

    /// Sets the left margin in cells.
    fn set_margin_left(&mut self, n: u8) {
        let m = self.get_margin().left(n);
        self.set_margin(m);
    }

    /// Sets the right margin in cells.
    fn set_margin_right(&mut self, n: u8) {
        let m = self.get_margin().right(n);
        self.set_margin(m);
    }

    /// Sets the top margin in cells.
    fn set_margin_top(&mut self, n: u8) {
        let m = self.get_margin().top(n);
        self.set_margin(m);
    }

    /// Sets the bottom margin in cells.
    fn set_margin_bottom(&mut self, n: u8) {
        let m = self.get_margin().bottom(n);
        self.set_margin(m);
    }

    /// Sets both left and right margins to `n`.
    fn set_horizontal_margin(&mut self, n: u8) {
        let m = self.get_margin().horizontal(n);
        self.set_margin(m);
    }

    /// Sets both top and bottom margins to `n`.
    fn set_vertical_margin(&mut self, n: u8) {
        let m = self.get_margin().vertical(n);
        self.set_margin(m);
    }

    /// Builder form of [`WidgetMethods::set_margin`].
    fn margin(mut self: Box<Self>, margin: Spacing) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_margin(margin);
        self
    }

    /// Builder form of [`WidgetMethods::set_margin_left`].
    fn margin_left(mut self: Box<Self>, n: u8) -> Box<Self>
    where
        Self: Sized,
    {
        let m = self.get_margin().left(n);
        self.set_margin(m);
        self
    }

    /// Builder form of [`WidgetMethods::set_margin_right`].
    fn margin_right(mut self: Box<Self>, n: u8) -> Box<Self>
    where
        Self: Sized,
    {
        let m = self.get_margin().right(n);
        self.set_margin(m);
        self
    }

    /// Builder form of [`WidgetMethods::set_margin_top`].
    fn margin_top(mut self: Box<Self>, n: u8) -> Box<Self>
    where
        Self: Sized,
    {
        let m = self.get_margin().top(n);
        self.set_margin(m);
        self
    }

    /// Builder form of [`WidgetMethods::set_margin_bottom`].
    fn margin_bottom(mut self: Box<Self>, n: u8) -> Box<Self>
    where
        Self: Sized,
    {
        let m = self.get_margin().bottom(n);
        self.set_margin(m);
        self
    }

    /// Builder form of [`WidgetMethods::set_horizontal_margin`].
    fn horizontal_margin(mut self: Box<Self>, n: u8) -> Box<Self>
    where
        Self: Sized,
    {
        let m = self.get_margin().horizontal(n);
        self.set_margin(m);
        self
    }

    /// Builder form of [`WidgetMethods::set_vertical_margin`].
    fn vertical_margin(mut self: Box<Self>, n: u8) -> Box<Self>
    where
        Self: Sized,
    {
        let m = self.get_margin().vertical(n);
        self.set_margin(m);
        self
    }

    /// Builder form of [`WidgetMethods::set_style`].
    fn style(mut self: Box<Self>, style: crate::render::style::Style) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_style(style);
        self
    }

    /// Builder form of [`WidgetMethods::set_flex`].
    fn flex(mut self: Box<Self>, flex: u8) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_flex(flex);
        self
    }

    /// Returns the widget's flex weight.
    fn get_flex(&self) -> u8 {
        self.get_layout().flex
    }

    /// Sets the widget's flex weight.
    fn set_flex(&mut self, flex: u8) {
        let layout = self.get_layout_mut();
        if layout.flex == flex {
            return;
        }
        layout.flex = flex;
        self.dirty_layout();
    }

    /// Builder form of [`WidgetMethods::set_x_align`].
    fn x_align(mut self: Box<Self>, a: FlexAlign) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_x_align(a);
        self
    }

    /// Builder form of [`WidgetMethods::set_y_align`].
    fn y_align(mut self: Box<Self>, a: FlexAlign) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_y_align(a);
        self
    }

    /// Sets the x-axis alignment override.
    fn set_x_align(&mut self, a: FlexAlign) {
        let layout = self.get_layout_mut();
        let new_align = layout.align.mode_x(Some(a));
        if layout.align == new_align {
            return;
        }
        layout.align = new_align;
        self.dirty_layout();
    }

    /// Sets the y-axis alignment override.
    fn set_y_align(&mut self, a: FlexAlign) {
        let layout = self.get_layout_mut();
        let new_align = layout.align.mode_y(Some(a));
        if layout.align == new_align {
            return;
        }
        layout.align = new_align;
        self.dirty_layout();
    }

    /// Returns the x-axis alignment override, or `None` when inheriting from the parent.
    fn get_x_align(&self) -> Option<FlexAlign> {
        self.get_layout().align.get_mode_x()
    }

    /// Returns the y-axis alignment override, or `None` when inheriting from the parent.
    fn get_y_align(&self) -> Option<FlexAlign> {
        self.get_layout().align.get_mode_y()
    }

    /// Sets the explicit minimum width override.
    fn set_min_width(&mut self, value: Option<u16>) {
        self.get_layout_mut().set_explicit_min(Axis2D::X, value);
    }
    /// Returns the explicit minimum width override, if any.
    fn get_min_width(&self) -> Option<u16> {
        self.get_layout().get_explicit_min(Axis2D::X)
    }
    /// Builder form of [`WidgetMethods::set_min_width`].
    fn min_width(mut self: Box<Self>, value: u16) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_min_width(Some(value));
        self
    }
    /// Builder form of [`WidgetMethods::set_min_width`] accepting an `Option`.
    fn min_width_opt(mut self: Box<Self>, value: Option<u16>) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_min_width(value);
        self
    }

    /// Sets the explicit maximum width override.
    fn set_max_width(&mut self, value: Option<u16>) {
        self.get_layout_mut().set_explicit_max(Axis2D::X, value);
    }
    /// Returns the explicit maximum width override, if any.
    fn get_max_width(&self) -> Option<u16> {
        self.get_layout().get_explicit_max(Axis2D::X)
    }
    /// Builder form of [`WidgetMethods::set_max_width`].
    fn max_width(mut self: Box<Self>, value: u16) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_max_width(Some(value));
        self
    }
    /// Builder form of [`WidgetMethods::set_max_width`] accepting an `Option`.
    fn max_width_opt(mut self: Box<Self>, value: Option<u16>) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_max_width(value);
        self
    }

    /// Sets the explicit minimum height override.
    fn set_min_height(&mut self, value: Option<u16>) {
        self.get_layout_mut().set_explicit_min(Axis2D::Y, value);
    }
    /// Returns the explicit minimum height override, if any.
    fn get_min_height(&self) -> Option<u16> {
        self.get_layout().get_explicit_min(Axis2D::Y)
    }
    /// Builder form of [`WidgetMethods::set_min_height`].
    fn min_height(mut self: Box<Self>, value: u16) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_min_height(Some(value));
        self
    }
    /// Builder form of [`WidgetMethods::set_min_height`] accepting an `Option`.
    fn min_height_opt(mut self: Box<Self>, value: Option<u16>) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_min_height(value);
        self
    }

    /// Sets the explicit maximum height override.
    fn set_max_height(&mut self, value: Option<u16>) {
        self.get_layout_mut().set_explicit_max(Axis2D::Y, value);
    }
    /// Returns the explicit maximum height override, if any.
    fn get_max_height(&self) -> Option<u16> {
        self.get_layout().get_explicit_max(Axis2D::Y)
    }
    /// Builder form of [`WidgetMethods::set_max_height`].
    fn max_height(mut self: Box<Self>, value: u16) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_max_height(Some(value));
        self
    }
    /// Builder form of [`WidgetMethods::set_max_height`] accepting an `Option`.
    fn max_height_opt(mut self: Box<Self>, value: Option<u16>) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_max_height(value);
        self
    }

    /// Sets the explicit preferred width override.
    fn set_preferred_width(&mut self, value: Option<u16>) {
        self.get_layout_mut().set_explicit_pref(Axis2D::X, value);
    }
    /// Returns the explicit preferred width override, if any.
    fn get_preferred_width(&self) -> Option<u16> {
        self.get_layout().get_explicit_pref(Axis2D::X)
    }
    /// Builder form of [`WidgetMethods::set_preferred_width`].
    fn preferred_width(mut self: Box<Self>, value: u16) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_preferred_width(Some(value));
        self
    }
    /// Builder form of [`WidgetMethods::set_preferred_width`] accepting an `Option`.
    fn preferred_width_opt(mut self: Box<Self>, value: Option<u16>) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_preferred_width(value);
        self
    }

    /// Sets the explicit preferred height override.
    fn set_preferred_height(&mut self, value: Option<u16>) {
        self.get_layout_mut().set_explicit_pref(Axis2D::Y, value);
    }
    /// Returns the explicit preferred height override, if any.
    fn get_preferred_height(&self) -> Option<u16> {
        self.get_layout().get_explicit_pref(Axis2D::Y)
    }
    /// Builder form of [`WidgetMethods::set_preferred_height`].
    fn preferred_height(mut self: Box<Self>, value: u16) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_preferred_height(Some(value));
        self
    }
    /// Builder form of [`WidgetMethods::set_preferred_height`] accepting an `Option`.
    fn preferred_height_opt(mut self: Box<Self>, value: Option<u16>) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_preferred_height(value);
        self
    }

    /// Sets the explicit minimum size on each axis.
    fn set_min_size(&mut self, value: Vec2<Option<u16>>) {
        self.set_min_width(value.x);
        self.set_min_height(value.y);
    }
    /// Returns the explicit minimum size on each axis.
    fn get_min_size(&self) -> Vec2<Option<u16>> {
        Vec2::new(self.get_min_width(), self.get_min_height())
    }
    /// Builder form of [`WidgetMethods::set_min_size`].
    fn min_size(mut self: Box<Self>, value: Vec2<u16>) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_min_size(Vec2::new(Some(value.x), Some(value.y)));
        self
    }
    /// Builder form of [`WidgetMethods::set_min_size`] accepting per-axis `Option`s.
    fn min_size_opt(mut self: Box<Self>, value: Vec2<Option<u16>>) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_min_size(value);
        self
    }

    /// Sets the explicit maximum size on each axis.
    fn set_max_size(&mut self, value: Vec2<Option<u16>>) {
        self.set_max_width(value.x);
        self.set_max_height(value.y);
    }
    /// Returns the explicit maximum size on each axis.
    fn get_max_size(&self) -> Vec2<Option<u16>> {
        Vec2::new(self.get_max_width(), self.get_max_height())
    }
    /// Builder form of [`WidgetMethods::set_max_size`].
    fn max_size(mut self: Box<Self>, value: Vec2<u16>) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_max_size(Vec2::new(Some(value.x), Some(value.y)));
        self
    }
    /// Builder form of [`WidgetMethods::set_max_size`] accepting per-axis `Option`s.
    fn max_size_opt(mut self: Box<Self>, value: Vec2<Option<u16>>) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_max_size(value);
        self
    }

    /// Sets the explicit preferred size on each axis.
    fn set_preferred_size(&mut self, value: Vec2<Option<u16>>) {
        self.set_preferred_width(value.x);
        self.set_preferred_height(value.y);
    }
    /// Returns the explicit preferred size on each axis.
    fn get_preferred_size(&self) -> Vec2<Option<u16>> {
        Vec2::new(self.get_preferred_width(), self.get_preferred_height())
    }
    /// Builder form of [`WidgetMethods::set_preferred_size`].
    fn preferred_size(mut self: Box<Self>, value: Vec2<u16>) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_preferred_size(Vec2::new(Some(value.x), Some(value.y)));
        self
    }
    /// Builder form of [`WidgetMethods::set_preferred_size`] accepting per-axis `Option`s.
    fn preferred_size_opt(mut self: Box<Self>, value: Vec2<Option<u16>>) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_preferred_size(value);
        self
    }

    /// Sets the width to `value`.
    fn set_width(&mut self, value: Option<u16>) {
        self.set_min_width(value);
        self.set_max_width(value);
        self.set_preferred_width(value);
    }
    /// Returns the width if pinned.
    fn get_width(&self) -> Option<u16> {
        let min = self.get_min_width();
        let max = self.get_max_width();
        if min == max {
            min
        } else {
            None
        }
    }
    /// Builder form of [`WidgetMethods::set_width`].
    fn width(mut self: Box<Self>, value: u16) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_width(Some(value));
        self
    }
    /// Builder form of [`WidgetMethods::set_width`] accepting an `Option`.
    fn width_opt(mut self: Box<Self>, value: Option<u16>) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_width(value);
        self
    }

    /// Sets the height to `value`.
    fn set_height(&mut self, value: Option<u16>) {
        self.set_min_height(value);
        self.set_max_height(value);
        self.set_preferred_height(value);
    }
    /// Returns the height if pinned.
    fn get_height(&self) -> Option<u16> {
        let min = self.get_min_height();
        let max = self.get_max_height();
        if min == max {
            min
        } else {
            None
        }
    }
    /// Builder form of [`WidgetMethods::set_height`].
    fn height(mut self: Box<Self>, value: u16) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_height(Some(value));
        self
    }
    /// Builder form of [`WidgetMethods::set_height`] accepting an `Option`.
    fn height_opt(mut self: Box<Self>, value: Option<u16>) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_height(value);
        self
    }

    /// Sets the size to `value`.
    fn set_size(&mut self, value: Vec2<Option<u16>>) {
        self.set_width(value.x);
        self.set_height(value.y);
    }
    /// Returns the size if pinned on each axis.
    fn get_size(&self) -> Vec2<Option<u16>> {
        Vec2::new(self.get_width(), self.get_height())
    }
    /// Builder form of [`WidgetMethods::set_size`].
    fn size(mut self: Box<Self>, value: Vec2<u16>) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_size(Vec2::new(Some(value.x), Some(value.y)));
        self
    }
    /// Builder form of [`WidgetMethods::set_size`] accepting per-axis `Option`s.
    fn size_opt(mut self: Box<Self>, value: Vec2<Option<u16>>) -> Box<Self>
    where
        Self: Sized,
    {
        self.set_size(value);
        self
    }

    /// Returns the widget identified by `id` anywhere in the subtree.
    fn get_widget<T: Widget>(&self, id: WidgetId<T>) -> Option<&T> where Self: Sized {
        find_cached(self, id.untyped())?.downcast_ref::<T>()
    }

    /// Mutable variant of [`WidgetMethods::get_widget`].
    fn get_widget_mut<T: Widget>(&mut self, id: WidgetId<T>) -> Option<&mut T> where Self: Sized {
        find_cached_mut(self, id.untyped())?.downcast_mut::<T>()
    }
}

impl<T: Widget + ?Sized> WidgetMethods for T {}

impl dyn Widget {
    /// Returns the widget identified by `id` anywhere in the subtree.
    pub fn get_widget<T: Widget>(&self, id: WidgetId<T>) -> Option<&T> {
        find_cached(self, id.untyped())?.downcast_ref::<T>()
    }

    /// Mutable variant of [`dyn Widget::get_widget`].
    pub fn get_widget_mut<T: Widget>(&mut self, id: WidgetId<T>) -> Option<&mut T> {
        find_cached_mut(self, id.untyped())?.downcast_mut::<T>()
    }

    pub(crate) fn find(&self, id: WidgetId) -> Option<&dyn Widget> {
        find_cached(self, id)
    }

    pub(crate) fn find_mut(&mut self, id: WidgetId) -> Option<&mut dyn Widget> {
        find_cached_mut(self, id)
    }

    pub(crate) fn find_path(&self, id: WidgetId) -> Option<Vec<WidgetId>> {
        resolve_cached_path(self, id).map(|p| p.ids)
    }
}

fn resolve_cached_path(widget: &dyn Widget, id: WidgetId) -> Option<WidgetPath> {
    let cached = PATH_CACHE.with_borrow(|cache| cache.get(&id).cloned());
    if let Some(path) = cached {
        if path.get(widget).is_some_and(|w| w.get_id() == id) {
            return Some(path);
        }
        PATH_CACHE.with_borrow_mut(|cache| { cache.remove(&id); });
    }
    let mut ids = Vec::new();
    if find_dfs_path(widget, id, &mut ids) {
        let path = WidgetPath { ids };
        PATH_CACHE.with_borrow_mut(|cache| {
            cache.insert(id, path.clone());
        });
        Some(path)
    } else {
        None
    }
}

fn find_cached(widget: &dyn Widget, id: WidgetId) -> Option<&dyn Widget> {
    resolve_cached_path(widget, id)?.get(widget)
}

fn find_cached_mut(widget: &mut dyn Widget, id: WidgetId) -> Option<&mut dyn Widget> {
    let path = resolve_cached_path(widget, id)?;
    walk_path_mut(widget, path.as_slice())
}

fn find_dfs_path(widget: &dyn Widget, id: WidgetId, path: &mut Vec<WidgetId>) -> bool {
    path.push(widget.get_id());
    if widget.get_id() == id {
        return true;
    }
    let mut found = false;
    widget.each_child(&mut |child| {
        if !found {
            found = find_dfs_path(child, id, path);
        }
    }, Sign::Positive);
    if !found {
        path.pop();
    }
    found
}

impl dyn Widget {
    /// Attempts to downcast a `Box<dyn Widget>` into `Box<T>`.
    pub fn downcast<T: Widget>(self: Box<dyn Widget>) -> Option<Box<T>> {
        (self as Box<dyn std::any::Any>).downcast::<T>().ok()
    }

    /// Attempts to borrow this widget as `&T`.
    pub fn downcast_ref<T: Widget>(&self) -> Option<&T> {
        (self as &dyn std::any::Any).downcast_ref::<T>()
    }

    /// Attempts to borrow this widget as `&mut T`.
    pub fn downcast_mut<T: Widget>(&mut self) -> Option<&mut T> {
        (self as &mut dyn std::any::Any).downcast_mut::<T>()
    }
}

impl std::fmt::Display for dyn Widget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.debug(f)
    }
}
