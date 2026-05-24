# tuie

A rich, performant TUI library for Rust.

https://github.com/user-attachments/assets/ec1d8d8d-e476-4b78-8404-dd234d5a2ec7

### Features

- **Layout:** Flexbox, grids, splits, and virtualized lists, all with min, max, and preferred-size constraints.
- **Images:** Kitty with sixel and half-block fallbacks, all working over SSH and tmux passthrough.
- **Input:** Iterator-based extensible text input with vi, emacs, modern, and custom bindings.
- **Harmonious:** Generated 256-color palette, consistent with user's base16 theme, zero config.
- **Chords:** Construct and match on human readable inputs: `chord!(Ctrl + Arrow(Up | Down))`
- **Async:** Timers and async callbacks with support for any async runtime.
- **Performance:** Per subtree/widget/cell dirty tracking, batched queries, shared memory graphics, packed structs.
- **GUI:** Optionally run as a GUI with box-drawing, smooth scrolling.

## Getting started

Install:

```sh
cargo add tuie --features=harmonious
```

Write your main:

```rust
use tuie::prelude::*;
use std::process::ExitCode;

fn main() -> std::io::Result<ExitCode> {
    let root = Pane::new()
        .border(Border::SINGLE)
        .child(Text::new().content("hello world"));
    tuie::start_tui(root)
}
```

Run:

```sh
cargo run
```

### Constructing widgets

Chain builders to create a simple widget tree:

```rust
fn greeting(name: String) -> Box<Pane> {
    Pane::new()
        .border(Border::SINGLE)
        .child(Text::new().content(format!("hello {name}")))
}
```

Or compose widgets together for something more complicated:

```rust
struct Greeting {
    root: Box<Pane>,
    text_id: WidgetId<Text>,
}

impl Greeting {
    fn new(name: &str) -> Box<Self> {
        let mut text_id = WidgetId::EMPTY;
        let root = Pane::new().border(Border::SINGLE).child(
            Text::new().content(format!("hello {name}")).id(&mut text_id)
        );
        Box::new(Self { root, text_id })
    }

    fn set_name(&mut self, name: &str) {
        if let Some(text) = self.root.get_widget_mut(self.text_id) {
            text.set_content(format!("hello {name}"));
        }
    }
}

impl DelegateWidget for Greeting {
    tuie::delegate_widget!(root);

    fn override_on_input(&mut self, queue: &mut InputQueue) -> InputResult {
        // you can override or react to your root widget's events and methods
        // using after_* and override_*.
        return self.root.on_input(queue);
    }
}
```

You can also `impl Widget` yourself, which is how all of the default widgets are implemented.

## Examples

- [`tuie-demo`](https://example.com/demo): discoverable, interactive reference covering most widgets and features.
- [`fz`](https://example.com/fz): fuzzy finder built on tuie, a real-world example of a non-trivial application.

## Feature flags

- `harmonious` — enables Lab-space palette generation from the terminal's base16 colors.
- `images` — enables the `Image` widget and `ImageConfig`/`ImageProtocol`/`ImageSource` types; pulls in the `image` crate.
- `gui` — enables `start_gui` and the `gui` module; pulls in `winit`, `wgpu`, `pollster`, `fontdb`, and `freetype-rs` (plus `objc2` on macOS). Implies `harmonious`.

