//! Test harness for driving widgets through the runtime.

use crate::prelude::*;

/// Drives a widget tree through the runtime and captures the rendered output.
pub struct Emulator(());

impl Emulator {
    /// Creates a [`Emulator`] with `root` rendered at `size`.
    pub fn new(root: &mut dyn Widget, size: Vec2<u16>) -> Self {
        crate::runtime::init_emulator(size);
        let _ = crate::runtime::update(root, &[RuntimeEvent::Resize(size)]);
        Self(())
    }

    /// Processes `events` through the runtime.
    pub fn update(&mut self, root: &mut dyn Widget, events: &[RuntimeEvent]) {
        let _ = crate::runtime::update(root, events);
    }

    /// Returns the most recently rendered frame as a [`StyledString`].
    pub fn get_snapshot(&self) -> StyledString {
        crate::runtime::get_emulator_snapshot()
    }

    /// Returns the most recently rendered frame as plain text.
    pub fn get_snapshot_text(&self) -> String {
        self.get_snapshot().text
    }

    /// Asserts the rendered frame matches `lines` row for row.
    ///
    /// # Panics
    ///
    /// Panics on mismatch.
    #[track_caller]
    pub fn assert_lines<'a, I>(&self, lines: I)
    where
        I: IntoIterator<Item = &'a str>,
    {
        let expected: Vec<&str> = lines.into_iter().collect();
        let actual = self.get_snapshot_text();
        let actual_lines: Vec<&str> = actual.split('\n').collect();
        if expected != actual_lines {
            let mut msg = String::from("rendered output did not match expected:\n");
            let n = expected.len().max(actual_lines.len());
            for i in 0..n {
                let e = expected.get(i).copied().unwrap_or("<missing>");
                let a = actual_lines.get(i).copied().unwrap_or("<missing>");
                let marker = if e == a {
                    "  "
                } else {
                    "!="
                };
                msg.push_str(&format!("  {marker} expected {e:?}\n     actual   {a:?}\n"));
            }
            panic!("{msg}");
        }
    }
}
