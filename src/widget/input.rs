//! Input event types and the queue widgets consume during dispatch.

use crate::prelude::*;

/// Single input event with mouse position, [`Chord`], and repeat count.
#[derive(Clone, PartialEq)]
pub struct InputEvent {
    /// Mouse position as leaf-local cell plus sub-cell fraction.
    pub pos: Vec2<f32>,
    /// Key or mouse [`Chord`] that triggered the event.
    pub chord: Chord,
    /// Repeat count. One for the first press, increasing on subsequent repeats.
    pub count: u8,
}

impl std::fmt::Display for InputEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.chord)
    }
}

impl InputEvent {
    /// Builds a synthetic event from `chord` with no mouse info.
    pub fn from_chord(chord: Chord) -> Self {
        Self {
            chord,
            pos: Vec2::of(-1.0),
            count: 1,
        }
    }

    /// Returns the cell containing [`pos`](Self::pos).
    pub fn cell(&self) -> Vec2<i32> {
        self.pos.map(|v| v.floor() as i32)
    }

    /// Returns `count` mapped onto the cycle `1..=max`, repeating after `max` clicks.
    pub fn click_cycle(&self, max: u8) -> u8 {
        1 + (self.count.saturating_sub(1)) % max
    }

    /// Returns true when [`click_cycle`](Self::click_cycle) lands on `n`.
    pub fn is_cycle(&self, n: u8, max: u8) -> bool {
        self.click_cycle(max) == n
    }

    /// Returns true when the [`Chord`] is any mouse trigger.
    pub fn is_mouse_event(&self) -> bool {
        matches!(
            self.chord.trigger,
            Trigger::MouseHover
                | Trigger::MouseDown(_)
                | Trigger::MouseDrag(_)
                | Trigger::MouseUp(_)
                | Trigger::MouseScroll(_)
                | Trigger::MouseSmoothScroll(_, _)
        )
    }
}

/// Outcome of a widget consuming an [`InputEvent`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputResult {
    /// Event was consumed and dispatch stops.
    Handled,
    /// Event was not consumed and dispatch continues to the next handler.
    Rejected,
    /// Event was claimed but more events are needed before it resolves.
    Pending,
}

/// Cursor over a slice of [`InputEvent`]s with flushing and unhandled flags.
pub struct InputQueue<'a> {
    events: &'a [InputEvent],
    pos: usize,
    flushing: bool,
    unhandled: bool,
}

impl std::fmt::Display for InputQueue<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[")?;
        for (i, event) in self.events.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            if i == self.pos {
                write!(f, ">")?;
            }
            write!(f, "{}", event)?;
        }
        write!(f, "]")
    }
}

impl<'a> InputQueue<'a> {
    /// Creates a queue starting at the first event.
    pub fn new(events: &'a [InputEvent], unhandled: bool) -> Self {
        Self {
            events,
            pos: 0,
            flushing: false,
            unhandled,
        }
    }

    /// Creates a queue in flushing mode.
    pub fn new_flushing(events: &'a [InputEvent], unhandled: bool) -> Self {
        Self {
            events,
            pos: 0,
            flushing: true,
            unhandled,
        }
    }

    /// Returns true when the queue is in flushing mode.
    pub fn is_flushing(&self) -> bool {
        self.flushing
    }

    /// Returns true when no widget consumed the events backing this queue.
    pub fn is_unhandled(&self) -> bool {
        self.unhandled
    }

    /// Advances the cursor and returns the consumed event.
    pub fn next(&mut self) -> Option<&'a InputEvent> {
        if self.pos < self.events.len() {
            let event = &self.events[self.pos];
            self.pos += 1;
            Some(event)
        } else {
            None
        }
    }

    /// Returns the next event without advancing the cursor.
    pub fn peek(&self) -> Option<&'a InputEvent> {
        self.events.get(self.pos)
    }

    /// Returns the number of events consumed so far.
    pub fn get_consumed_count(&self) -> usize {
        self.pos
    }

    /// Returns the events at or after the cursor.
    pub fn get_remaining(&self) -> &[InputEvent] {
        &self.events[self.pos..]
    }

    /// Returns the full backing slice regardless of cursor position.
    pub fn get_all(&self) -> &[InputEvent] {
        self.events
    }

    /// Rewinds the cursor to the first event.
    pub fn reset(&mut self) {
        self.pos = 0;
    }
}
