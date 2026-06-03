//! Floating overlay anchored to a child widget.

use crate::prelude::*;
use crate::runtime::popup::resolve_placement;
use crate::widget::flow_output_size;

/// Floating overlay anchored to a child widget, positioned by a [`Placement`].
pub struct Tooltip {
    layout: Layout,
    anchor: Box<dyn Widget>,
    body: Option<Box<ZLayer>>,
    placement: Placement,
    visible: bool,
    on_top: bool,
    autohide: bool,
}

impl Tooltip {
    fn layer(&self) -> Layer {
        if self.on_top {
            Layer::Top
        } else {
            Layer::Middle
        }
    }

    fn sync_z(&mut self) {
        let z = self.layer();
        if let Some(body) = &mut self.body {
            body.z = z;
        }
        self.dirty_paint();
    }
}

impl Widget for Tooltip {
    fn get_layout(&self) -> &Layout {
        &self.layout
    }

    fn get_layout_mut(&mut self) -> &mut Layout {
        &mut self.layout
    }

    fn get_name(&self) -> &'static str {
        "Tooltip"
    }

    fn measure_constraints(&mut self) -> Constraints {
        constrain_child(&mut *self.anchor);
        if let Some(body) = &mut self.body {
            constrain_child(&mut **body);
        }
        self.anchor.get_layout().constraints
    }

    fn layout_flow(&mut self, allocated: Vec2<u16>) -> Vec2<u16> {
        let anchor_out = flow_child(&mut *self.anchor, allocated);
        if let Some(body) = &mut self.body {
            let canvas = crate::runtime::get_terminal_info().size;
            let body_max = body.get_layout().constraints.max_size;
            let alloc_x = body_max.x.min(canvas.x);
            let body_input = Vec2::new(alloc_x, canvas.y);
            flow_child(&mut **body, body_input);
            let flow_out = flow_output_size(body.get_layout());
            let body_size = Vec2::new(flow_out.x.min(canvas.x), flow_out.y.min(canvas.y));
            body.set_rect_size(body_size);
        }
        anchor_out
    }

    fn layout_measure(&self, allocated: Vec2<u16>) -> Vec2<u16> {
        measure_child(&*self.anchor, allocated)
    }

    fn layout_position(&mut self) {
        let anchor_margin = self.anchor.get_layout().get_margin_before().map(|v| v as i32);
        self.anchor.set_pos(self.layout.rect.pos + anchor_margin);
        self.anchor.layout_position();
        if let Some(body) = &mut self.body {
            let body_size = body.get_outer_size();
            let pos = resolve_placement(
                &self.placement,
                self.anchor.get_rect(),
                body_size,
            );
            let body_margin = body.get_layout().get_margin_before().map(|v| v as i32);
            body.set_pos(pos + body_margin);
            body.layout_position();
        }
    }

    fn render(&self, mut ctx: crate::render::RenderContext) {
        let anchor_pos = self.anchor.get_pos() - self.layout.rect.pos;
        ctx.render_child(&*self.anchor, anchor_pos);
        if self.visible {
            if let Some(body) = &self.body {
                let body_pos = body.get_pos() - self.layout.rect.pos;
                ctx.render_child(&**body, body_pos);
            }
        }
    }

    fn each_child(
        &self,
        f: &mut dyn FnMut(&dyn Widget),
        direction: Sign,
    ) {
        match direction {
            Sign::Positive => {
                f(&*self.anchor);
                if self.visible {
                    if let Some(body) = &self.body {
                        f(&**body);
                    }
                }
            }
            Sign::Negative => {
                if self.visible {
                    if let Some(body) = &self.body {
                        f(&**body);
                    }
                }
                f(&*self.anchor);
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
                f(&mut *self.anchor);
                if self.visible {
                    if let Some(body) = &mut self.body {
                        f(&mut **body);
                    }
                }
            }
            Sign::Negative => {
                if self.visible {
                    if let Some(body) = &mut self.body {
                        f(&mut **body);
                    }
                }
                f(&mut *self.anchor);
            }
        }
    }

    fn on_state_change(&mut self, state: WidgetState) {
        if self.autohide && state == WidgetState::None && self.visible {
            self.set_visible(false);
        }
    }
}

impl Tooltip {
    /// Creates a tooltip with `anchor` as the always-visible child and no body.
    pub fn new(anchor: Box<dyn Widget>) -> Box<Self> {
        Box::new(Self {
            layout: Layout::new(),
            anchor,
            body: None,
            placement: Placement::center(),
            visible: false,
            on_top: false,
            autohide: false,
        })
    }

    /// Builder form of [`Tooltip::set_content`].
    pub fn content(mut self: Box<Self>, body: Box<dyn Widget>) -> Box<Self> {
        self.set_content(body);
        self
    }

    /// Sets the floating body shown when the tooltip is visible.
    pub fn set_content(&mut self, body: Box<dyn Widget>) {
        let z = self.layer();
        self.body = Some(Box::new(ZLayer { inner: body, z }));
        self.dirty_layout();
    }

    crate::field! {
        /// Whether the body renders above all other widgets.
        on_top: bool; sync_z
    }

    crate::layout_field! {
        /// Where the body sits relative to the anchor.
        placement: Placement
    }

    crate::layout_field! {
        /// Whether the body is shown.
        visible: bool
    }

    crate::style_field! {
        /// Whether the body hides when the anchor loses hover or focus.
        autohide: bool
    }
}

struct ZLayer {
    inner: Box<dyn Widget>,
    z: Layer,
}

impl DelegateWidget for ZLayer {
    crate::delegate_widget!(inner);

    fn override_get_layer(&self) -> Layer {
        self.z
    }
}
