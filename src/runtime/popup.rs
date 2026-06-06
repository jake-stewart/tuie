//! Popup management and placement.

use crate::prelude::*;
use std::cell::RefCell;

struct PopupQueues {
    open: Vec<Popup>,
    close: Vec<WidgetId>,
    dismiss: Vec<WidgetId>,
}

thread_local! {
    static POPUP_QUEUES: RefCell<PopupQueues> = const {
        RefCell::new(PopupQueues {
            open: Vec::new(),
            close: Vec::new(),
            dismiss: Vec::new(),
        })
    };
}

/// Anchor and popup alignment points plus a cell offset that resolve a popup's screen position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub(crate) anchor_point: Vec2<Align>,
    pub(crate) popup_point: Vec2<Align>,
    pub(crate) offset: Vec2<i16>,
}

impl Placement {
    /// Centers the popup on the anchor.
    pub fn center() -> Self {
        Self {
            anchor_point: Vec2::of(Align::Center),
            popup_point: Vec2::of(Align::Center),
            offset: Vec2::of(0),
        }
    }

    /// Places the popup adjacent to the anchor along `dir`.
    pub fn side(dir: Direction2D, sign: Sign, align: Align) -> Self {
        let axis = dir.axis();
        let cross = axis.flip();

        let anchor_edge = match dir.screen_sign() {
            Sign::Positive => Align::End,
            Sign::Negative => Align::Start,
        };

        let popup_edge = match sign {
            Sign::Positive => anchor_edge.flip(),
            Sign::Negative => anchor_edge,
        };

        let mut anchor_point = Vec2::of(Align::Start);
        let mut popup_point = Vec2::of(Align::Start);

        anchor_point[axis] = anchor_edge;
        popup_point[axis] = popup_edge;
        anchor_point[cross] = align;
        popup_point[cross] = align;

        Self {
            anchor_point,
            popup_point,
            offset: Vec2::of(0),
        }
    }

    /// Sets the alignment point on the anchor rect that the popup point lines up with.
    pub fn anchor_point(mut self, point: Vec2<Align>) -> Self {
        self.anchor_point = point;
        self
    }

    /// Sets the alignment point on the popup rect that is positioned at the anchor point.
    pub fn popup_point(mut self, point: Vec2<Align>) -> Self {
        self.popup_point = point;
        self
    }

    /// Sets the cell offset applied after alignment resolution.
    pub fn offset(mut self, offset: Vec2<i16>) -> Self {
        self.offset = offset;
        self
    }
}

/// Floating widget overlay with a [`Placement`] and dismissal policy.
pub struct Popup {
    pub(crate) content: Box<dyn Widget>,
    placement: Placement,
    dismissible: bool,
}

impl Popup {
    /// Creates a popup wrapping `content`.
    pub fn new(content: Box<dyn Widget>) -> Self {
        Self {
            content,
            placement: Placement::center(),
            dismissible: false,
        }
    }

    /// Overrides the [`Placement`].
    pub fn placement(mut self, placement: Placement) -> Self {
        self.placement = placement;
        self
    }

    /// Makes outside interaction close the popup automatically.
    pub fn dismissible(mut self) -> Self {
        self.dismissible = true;
        self
    }

    /// Calls [`Popup::dismissible`] when `value` is `true`.
    pub fn dismissible_if(self, value: bool) -> Self {
        if value {
            self.dismissible()
        } else {
            self
        }
    }
}

pub(crate) struct ActivePopup {
    pub content: Box<dyn Widget>,
    pub placement: Placement,
    pub dismissible: bool,
    pub focus_chain: Vec<WidgetId>,
}

impl ActivePopup {
    pub(crate) fn from_popup(popup: Popup, focus_chain: Vec<WidgetId>) -> Self {
        Self {
            content: popup.content,
            placement: popup.placement,
            dismissible: popup.dismissible,
            focus_chain,
        }
    }
}

/// Event signalling the user attempted to dismiss a non-dismissible [`Popup`].
pub struct PopupDismissRequested;

/// Event signalling a [`Popup`] was closed.
pub struct PopupClosed;

/// Opens `popup`.
pub fn open_popup(popup: Popup) {
    POPUP_QUEUES.with_borrow_mut(|q| q.open.push(popup));
}

/// Closes the popup containing `id`.
pub fn close_popup(id: WidgetId<impl ?Sized>) {
    POPUP_QUEUES.with_borrow_mut(|q| q.close.push(id.untyped()));
}

/// Queues a [`PopupDismissRequested`] event for the popup containing `id`.
pub fn dismiss_popup(id: WidgetId<impl ?Sized>) {
    POPUP_QUEUES.with_borrow_mut(|q| q.dismiss.push(id.untyped()));
}

pub(crate) fn drain_open_requests() -> Vec<Popup> {
    POPUP_QUEUES.with_borrow_mut(|q| std::mem::take(&mut q.open))
}

pub(crate) fn drain_close_requests() -> Vec<WidgetId> {
    POPUP_QUEUES.with_borrow_mut(|q| std::mem::take(&mut q.close))
}

pub(crate) fn drain_dismiss_requests() -> Vec<WidgetId> {
    POPUP_QUEUES.with_borrow_mut(|q| std::mem::take(&mut q.dismiss))
}

pub(crate) fn resolve_placement(
    placement: &Placement,
    anchor_rect: Rect<i32, u16>,
    popup_size: Vec2<u16>,
) -> Vec2<i32> {
    Axis2D::map(|a| {
        let anchor_pos = anchor_rect.pos[a];
        let anchor_size = anchor_rect.size[a] as i32;
        let popup_size = popup_size[a] as i32;

        let anchor_coord = match placement.anchor_point[a] {
            Align::Start => anchor_pos,
            Align::Center => anchor_pos + anchor_size / 2,
            Align::End => anchor_pos + anchor_size,
        };

        let popup_coord = match placement.popup_point[a] {
            Align::Start => 0,
            Align::Center => popup_size / 2,
            Align::End => popup_size,
        };

        anchor_coord - popup_coord + placement.offset[a] as i32
    })
}

pub(crate) fn position_popup(popup: &mut ActivePopup, window_size: Vec2<u16>) {
    let window_rect = Rect::new(Vec2::of(0i32), window_size);
    let pos = resolve_placement(&popup.placement, window_rect, popup.content.get_outer_size());
    let margin_before = popup.content.get_layout().get_margin_before().map(|v| v as i32);
    popup.content.set_pos(pos + margin_before);
    popup.content.layout_position();
}
