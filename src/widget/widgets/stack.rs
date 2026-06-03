//! Container that overlays layer widgets on top of a base widget.

use crate::prelude::*;
use crate::widget::flow_output_size;

/// Container that overlays one or more layer widgets on top of a base widget.
pub struct Stack {
    layout: Layout,
    base: Box<dyn Widget>,
    layers: Vec<Box<dyn Widget>>,
}

impl Stack {
    fn contains_pos(child: &dyn Widget, pos: Vec2<f32>) -> bool {
        let child_pos = child.get_pos();
        let child_size = child.get_rect_size().map(|v| v as i32);
        Axis2D::all(|a| {
            pos[a] >= child_pos[a] as f32 && pos[a] < (child_pos[a] + child_size[a]) as f32
        })
    }

    fn find_in_child(
        child: &dyn Widget,
        predicate: &dyn Fn(&dyn Widget) -> bool,
        mut path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        if let Some(found) = child.find_descendant(predicate, path.as_mut().map(|p| &mut **p)) {
            if let Some(p) = &mut path {
                p.push(child.get_id());
            }
            return Some(found);
        }
        if predicate(child) {
            if let Some(p) = &mut path {
                p.push(child.get_id());
            }
            return Some(child.get_id());
        }
        None
    }

    fn hit_child(
        child: &dyn Widget,
        pos: Vec2<f32>,
        mut path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        if !Self::contains_pos(child, pos) {
            return None;
        }
        let hit = child
            .descendant_at_pos(pos, path.as_mut().map(|p| &mut **p))
            .unwrap_or_else(|| child.get_id());
        if let Some(p) = &mut path {
            p.push(child.get_id());
        }
        Some(hit)
    }

    fn hit_layer(
        layer: &dyn Widget,
        pos: Vec2<f32>,
        mut path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        if !Self::contains_pos(layer, pos) {
            return None;
        }
        let hit = layer.descendant_at_pos(pos, path.as_mut().map(|p| &mut **p))?;
        if let Some(p) = &mut path {
            p.push(layer.get_id());
        }
        Some(hit)
    }

    fn find_hit_child(
        child: &dyn Widget,
        pos: Vec2<f32>,
        predicate: &dyn Fn(&dyn Widget) -> bool,
        mut path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        if !Self::contains_pos(child, pos) {
            return None;
        }
        if let Some(found) = child.find_descendant_at_pos(pos, predicate, path.as_mut().map(|p| &mut **p)) {
            if let Some(p) = &mut path {
                p.push(child.get_id());
            }
            return Some(found);
        }
        if predicate(child) {
            if let Some(p) = &mut path {
                p.push(child.get_id());
            }
            return Some(child.get_id());
        }
        None
    }

    fn find_hit_layer(
        layer: &dyn Widget,
        pos: Vec2<f32>,
        predicate: &dyn Fn(&dyn Widget) -> bool,
        mut path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        if !Self::contains_pos(layer, pos) {
            return None;
        }
        let found = layer.find_descendant_at_pos(pos, predicate, path.as_mut().map(|p| &mut **p))?;
        if let Some(p) = &mut path {
            p.push(layer.get_id());
        }
        Some(found)
    }

    fn clamp_base_size(
        base: &dyn Widget,
        content_size: Vec2<u16>,
        desired_size: impl Fn(&Layout) -> Vec2<u16>,
    ) -> Vec2<u16> {
        let layout = base.get_layout();
        let desired = desired_size(layout);
        Axis2D::map(|a| {
            let mut size = content_size[a];
            size = std::cmp::min(size, layout.constraints.max_size[a]);
            size = std::cmp::max(size, desired[a]);
            size = std::cmp::max(size, layout.constraints.min_size[a]);
            size
        })
    }

    fn clamp_layer_size(
        layer: &dyn Widget,
        content_size: Vec2<u16>,
        desired_size: impl Fn(&Layout) -> Vec2<u16>,
    ) -> Vec2<u16> {
        let layout = layer.get_layout();
        let has_flex = layer.get_flex() > 0;
        let desired = desired_size(layout);
        Axis2D::map(|a| {
            let mut size = if has_flex {
                content_size[a]
            } else {
                desired[a]
            };
            size = std::cmp::min(size, layout.constraints.max_size[a]);
            size = std::cmp::max(size, layout.constraints.min_size[a]);
            size
        })
    }
}

impl Widget for Stack {
    fn get_layout(&self) -> &Layout {
        &self.layout
    }

    fn get_layout_mut(&mut self) -> &mut Layout {
        &mut self.layout
    }

    fn get_name(&self) -> &'static str {
        "Stack"
    }

    fn get_flow_axis(&self) -> Axis2D {
        self.base.get_flow_axis()
    }

    fn measure_constraints(&mut self) -> Constraints {
        self.each_child_mut(&mut constrain_child, Sign::Positive);
        self.base.get_layout().constraints
    }

    fn layout_flow(&mut self, allocated: Vec2<u16>) -> Vec2<u16> {
        let base_size = Self::clamp_base_size(&*self.base, allocated, |l| l.constraints.min_size);
        flow_child(&mut *self.base, base_size);
        for layer in self.layers.iter_mut() {
            let size = Self::clamp_layer_size(&**layer, allocated, |l| l.constraints.min_size);
            flow_child(&mut **layer, size);
        }
        let base_clamped = Self::clamp_base_size(&*self.base, allocated, |l| flow_output_size(l));
        let base_margin = self.base.get_layout().get_margin_total();
        self.base.set_rect_size(Axis2D::map(|a| base_clamped[a].saturating_sub(base_margin[a])));
        for layer in self.layers.iter_mut() {
            let size = Self::clamp_layer_size(&**layer, allocated, |l| flow_output_size(l));
            let margin = layer.get_layout().get_margin_total();
            layer.set_rect_size(Axis2D::map(|a| size[a].saturating_sub(margin[a])));
        }
        flow_output_size(self.base.get_layout())
    }

    fn layout_measure(&self, allocated: Vec2<u16>) -> Vec2<u16> {
        let base_size = Self::clamp_base_size(&*self.base, allocated, |l| l.constraints.min_size);
        measure_child(&*self.base, base_size)
    }

    fn layout_position(&mut self) {
        let content_pos = self.layout.rect.pos;
        let base_margin = self.base.get_layout().get_margin_before().map(|v| v as i32);
        self.base.set_pos(content_pos + base_margin);
        self.base.layout_position();
        for layer in self.layers.iter_mut() {
            let margin = layer.get_layout().get_margin_before().map(|v| v as i32);
            layer.set_pos(content_pos + margin);
            layer.layout_position();
        }
    }

    fn render(&self, mut ctx: RenderContext) {
        let mut clear_style = self.layout.style;
        if clear_style.get_bg().is_none() {
            clear_style.set_bg(Some(Color::Background));
        }
        ctx.set_style(clear_style);
        ctx.clear();
        ctx.set_style(self.layout.style);
        let content_pos = self.layout.rect.pos;
        let base_offset = self.base.get_pos() - content_pos;
        ctx.render_child(&*self.base, base_offset);
        for layer in self.layers.iter() {
            let layer_offset = layer.get_pos() - content_pos;
            ctx.queue_layer(&**layer, layer_offset);
        }
    }

    fn find_descendant(
        &self,
        predicate: &dyn Fn(&dyn Widget) -> bool,
        mut path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        for layer in self.layers.iter().rev() {
            if let Some(r) = Self::find_in_child(&**layer, predicate, path.as_mut().map(|p| &mut **p)) {
                return Some(r);
            }
        }
        Self::find_in_child(&*self.base, predicate, path)
    }

    fn each_child(
        &self,
        f: &mut dyn FnMut(&dyn Widget),
        direction: Sign,
    ) {
        match direction {
            Sign::Positive => {
                f(&*self.base);
                for layer in self.layers.iter() {
                    f(&**layer);
                }
            }
            Sign::Negative => {
                for layer in self.layers.iter().rev() {
                    f(&**layer);
                }
                f(&*self.base);
            }
        }
    }

    fn each_child_mut(
        &mut self,
        f: &mut dyn FnMut(&mut dyn Widget),
        direction: Sign,
    ) {
        match direction {
            Sign::Positive => {
                f(&mut *self.base);
                for layer in self.layers.iter_mut() {
                    f(&mut **layer);
                }
            }
            Sign::Negative => {
                for layer in self.layers.iter_mut().rev() {
                    f(&mut **layer);
                }
                f(&mut *self.base);
            }
        }
    }

    fn descendant_at_pos(
        &self,
        pos: Vec2<f32>,
        mut path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        for layer in self.layers.iter().rev() {
            if let Some(r) = Self::hit_layer(&**layer, pos, path.as_mut().map(|p| &mut **p)) {
                return Some(r);
            }
        }
        Self::hit_child(&*self.base, pos, path)
    }

    fn find_descendant_at_pos(
        &self,
        pos: Vec2<f32>,
        predicate: &dyn Fn(&dyn Widget) -> bool,
        mut path: Option<&mut Vec<WidgetId>>,
    ) -> Option<WidgetId> {
        for layer in self.layers.iter().rev() {
            if let Some(r) = Self::find_hit_layer(&**layer, pos, predicate, path.as_mut().map(|p| &mut **p)) {
                return Some(r);
            }
        }
        Self::find_hit_child(&*self.base, pos, predicate, path)
    }

    fn get_cursor(
        &self,
        selected: Option<WidgetId>,
    ) -> Option<(CursorShape, Vec2<i32>)> {
        let selected = selected?;
        if let Some(r) = self.layout.get_child_cursor(&*self.base, selected) {
            return Some(r);
        }
        for layer in self.layers.iter() {
            if let Some(r) = self.layout.get_child_cursor(&**layer, selected) {
                return Some(r);
            }
        }
        None
    }
}

impl Stack {
    /// Creates a stack wrapping `base` with no layers.
    pub fn new(base: Box<dyn Widget>) -> Box<Self> {
        Box::new(Self {
            layout: Layout::new(),
            base,
            layers: Vec::new(),
        })
    }

    /// Appends `N` layers on top of the base.
    pub fn children<const N: usize>(
        mut self: Box<Self>,
        layers: [Box<dyn Widget>; N],
    ) -> Box<Self> {
        self.layers.extend(layers);
        self.dirty_layout();
        self
    }

    /// Builder form of [`Stack::add_child`].
    pub fn child(mut self: Box<Self>, widget: Box<dyn Widget>) -> Box<Self> {
        self.add_child(widget);
        self
    }

    /// Pushes `layer` onto the top of the stack.
    pub fn add_child(&mut self, layer: Box<dyn Widget>) {
        self.layers.push(layer);
        self.dirty_layout();
    }

    /// Inserts `layer` at position `idx`, clamped to `0..=layers.len()`.
    pub fn insert_child(&mut self, idx: usize, layer: Box<dyn Widget>) {
        let idx = idx.min(self.layers.len());
        self.layers.insert(idx, layer);
        self.dirty_layout();
    }

    /// Removes the layer with the given [`WidgetId`] and downcasts it to `T`.
    pub fn remove<T: AnyWidget + ?Sized>(&mut self, id: WidgetId<T>) -> Option<Box<T>> {
        let idx = self.layers.iter().position(|l| l.get_id() == id)?;
        let removed = self.layers.remove(idx);
        self.dirty_layout();
        T::downcast_box(removed)
    }

    /// Removes all layers, leaving the base widget alone.
    pub fn clear(&mut self) {
        if !self.layers.is_empty() {
            self.layers.clear();
            self.dirty_layout();
        }
    }

    /// Iterates over layers from bottom (first added) to top.
    pub fn iter_children(&self) -> impl ExactSizeIterator<Item = &dyn Widget> {
        self.layers.iter().map(|l| &**l)
    }
}
