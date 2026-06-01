//! Runtime event decoding and delivery.

use crate::prelude::*;
use crate::ansi;
use std::os::unix::io::RawFd;
use std::time::{Duration, Instant};

/// Terminal background brightness reported by the host.
pub use crate::ansi::ColorScheme;

/// Event delivered to the runtime.
#[derive(Clone, PartialEq)]
pub enum RuntimeEvent {
    /// Keyboard or mouse input.
    Input(InputEvent),
    /// Terminal gained or lost focus.
    Focus(bool),
    /// Terminal was resized to the given cell dimensions.
    Resize(Vec2<u16>),
    /// Bracketed paste payload.
    Paste(String),
    /// Terminal switched between light and dark mode.
    ColorSchemeChange(ColorScheme),
    /// Quit with the given exit code.
    Quit(u8),
    /// `SIGTSTP` received.
    Suspend,
    /// Async task produced a value or completed.
    Wake,
    /// GUI-only hint that the user is actively touching the input device.
    DragHold(bool),
}

impl RuntimeEvent {
    /// Returns the [`Chord`] for input events, or `None` for non-input variants.
    pub fn get_chord(&self) -> Option<&Chord> {
        match self {
            RuntimeEvent::Input(e) => Some(&e.chord),
            _ => None,
        }
    }

    /// Builds an input event positioned at the center of cell `pos`.
    pub fn input_at(chord: Chord, pos: Vec2<i32>) -> Self {
        RuntimeEvent::Input(InputEvent {
            chord,
            pos: pos.map(|v| v as f32 + 0.5),
            count: 1,
        })
    }
}

impl From<Chord> for RuntimeEvent {
    fn from(chord: Chord) -> Self {
        RuntimeEvent::Input(InputEvent::from_chord(chord))
    }
}

impl std::fmt::Display for RuntimeEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Input(event) => {
                write!(f, "Input({}, {})", event.chord, event.pos)
            }
            Self::Focus(focused) => write!(f, "Focus({focused})"),
            Self::Resize(size) => write!(f, "Resize({size})"),
            Self::Paste(text) => write!(f, "Paste({text})"),
            Self::ColorSchemeChange(scheme) => write!(f, "ColorSchemeChange({scheme})"),
            Self::Quit(signo) => write!(f, "Quit({signo})"),
            Self::Suspend => write!(f, "Suspend"),
            Self::Wake => write!(f, "Wake"),
            Self::DragHold(held) => write!(f, "DragHold({held})"),
        }
    }
}

pub(crate) struct RuntimeEventReader {
    last_click: Instant,
    click_pos: Vec2<i32>,
    mouse_pos: Vec2<i32>,
    mouse_subpx: Vec2<i32>,
    click_count: u8,
    last_key: Option<Trigger>,
    last_key_time: Instant,
    key_repeat_count: u8,
    reader: ansi::Reader,
}

impl RuntimeEventReader {
    pub(crate) const REPEAT_WINDOW: Duration = Duration::from_millis(500);
    pub(crate) const MAX_REPEAT_COUNT: u8 = 240;

    pub(crate) fn new() -> std::io::Result<Self> {
        Ok(Self {
            last_click: Instant::now(),
            click_pos: Vec2::of(-1),
            mouse_pos: Vec2::of(-1),
            mouse_subpx: Vec2::of(-1),
            click_count: 0,
            last_key: None,
            last_key_time: Instant::now(),
            key_repeat_count: 0,
            reader: ansi::Reader::new()?,
        })
    }

    /// Adds the runtime wake pipe to the reader's poll set.
    pub(crate) fn set_wake_fd(&mut self, fd: RawFd) {
        self.reader.set_wake_fd(fd);
    }

    /// Waits up to `timeout` for terminal activity and returns all decoded events.
    pub(crate) fn read_batch(
        &mut self,
        timeout: Option<Duration>,
    ) -> std::io::Result<(Vec<RuntimeEvent>, bool)> {
        loop {
            let woken = self.reader.wait(timeout)?;
            let mut out = Vec::new();
            while let Some(event) = self.reader.try_read() {
                if let Some(tui_event) = self.translate_native(event) {
                    out.push(tui_event);
                }
            }
            if !out.is_empty() || woken || timeout.is_some() {
                return Ok((out, woken));
            }
        }
    }

    fn register_key(&mut self, chord: Chord) -> RuntimeEvent {
        let now = Instant::now();
        let is_repeat = self.last_key.as_ref() == Some(&chord.trigger)
            && now.duration_since(self.last_key_time) < Self::REPEAT_WINDOW;
        if is_repeat {
            self.key_repeat_count = self.key_repeat_count % Self::MAX_REPEAT_COUNT + 1;
        } else {
            self.key_repeat_count = 1;
        }
        self.last_key = Some(chord.trigger.clone());
        self.last_key_time = now;
        RuntimeEvent::Input(InputEvent {
            chord,
            pos: self.pack_pos(),
            count: self.key_repeat_count,
        })
    }

    fn pack_pos(&self) -> Vec2<f32> {
        if self.mouse_pos.x < 0 || self.mouse_pos.y < 0 {
            return Vec2::of(-1.0);
        }
        if self.mouse_subpx.x < 0 {
            return self.mouse_pos.map(|v| v as f32 + 0.5);
        }
        let cell_px = crate::runtime::tree::cell_px();
        Axis2D::map(|a| self.mouse_pos[a] as f32 + self.mouse_subpx[a] as f32 / cell_px[a] as f32)
    }

    fn mouse_event(
        &mut self,
        trigger: Trigger,
        column: u16,
        row: u16,
        modifiers: Modifiers,
    ) -> RuntimeEvent {
        if let Some(dpr) = crate::runtime::mouse_pixel_dpr() {
            let canon = crate::runtime::tree::cell_px();
            let phys = Vec2::new(
                column as i32 * dpr.x as i32,
                row as i32 * dpr.y as i32,
            );
            self.mouse_pos = Vec2::new(phys.x / canon.x.max(1), phys.y / canon.y.max(1));
            self.mouse_subpx = Vec2::new(phys.x % canon.x.max(1), phys.y % canon.y.max(1));
        } else {
            self.mouse_pos = Vec2::new(column.into(), row.into());
            self.mouse_subpx = Vec2::of(-1);
        }
        match trigger {
            Trigger::MouseDown(_) => {
                let now = Instant::now();
                let is_multi_click = self.click_pos == self.mouse_pos
                    && now.duration_since(self.last_click) < Self::REPEAT_WINDOW;
                if is_multi_click {
                    self.click_count = self.click_count % Self::MAX_REPEAT_COUNT + 1;
                } else {
                    self.click_count = 1;
                }
                self.click_pos = self.mouse_pos;
                self.last_click = now;
                self.input_event(trigger, modifiers, self.click_count)
            }
            Trigger::MouseUp(_) | Trigger::MouseDrag(_) => {
                self.input_event(trigger, modifiers, self.click_count)
            }
            _ => self.input_event(trigger, modifiers, 1),
        }
    }

    fn input_event(
        &self,
        trigger: Trigger,
        modifiers: Modifiers,
        count: u8,
    ) -> RuntimeEvent {
        RuntimeEvent::Input(InputEvent {
            chord: Chord::new(trigger, modifiers),
            pos: self.pack_pos(),
            count,
        })
    }

    fn translate_native(&mut self, event: ansi::ParsedEvent) -> Option<RuntimeEvent> {
        use ansi::ParsedEvent as E;
        match event {
            E::Key(chord) => Some(self.register_key(chord)),
            E::Mouse(m) => Some(self.mouse_event(m.trigger, m.column, m.row, m.modifiers)),
            E::Resize(size) => Some(RuntimeEvent::Resize(size)),
            E::Focus(focused) => Some(RuntimeEvent::Focus(focused)),
            E::Paste(s) => Some(RuntimeEvent::Paste(s)),
            E::ColorScheme(scheme) => Some(RuntimeEvent::ColorSchemeChange(scheme)),
            _ => None,
        }
    }
}
