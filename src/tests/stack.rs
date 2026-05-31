//! Integration tests for the stack widget.

use tuie::prelude::*;
use tuie::emulator::Emulator;

#[test]
fn renders_base_only() {
    let mut root = Stack::new(
        Pane::new().bordered().children([Text::new().content("base")]),
    );
    let term = Emulator::new(&mut *root, Vec2::new(6, 3));
    term.assert_lines([
        "┌────┐",
        "│base│",
        "└────┘",
    ]);
}

#[test]
fn layer_overlays_base() {
    let mut root = Stack::new(
        Pane::new().flex(1).children([Text::new().content("aaaaaa")]),
    )
    .flex(1)
    .children([
        Pane::new().flex(1).children([
            Text::new().content("BB").width(2).height(1).margin_left(2),
        ]),
    ]);
    let term = Emulator::new(&mut *root, Vec2::new(6, 1));
    term.assert_lines([
        "aaBBaa",
    ]);
}

#[test]
fn multiple_layers_z_order() {
    let mut root = Stack::new(
        Pane::new().flex(1).children([Text::new().content("........")]),
    )
    .flex(1)
    .children([
        Pane::new().flex(1).children([
            Text::new().content("LLLL").width(4).height(1).margin_left(1),
        ]),
        Pane::new().flex(1).children([
            Text::new().content("TT").width(2).height(1).margin_left(3),
        ]),
    ]);
    let term = Emulator::new(&mut *root, Vec2::new(8, 1));
    term.assert_lines([
        ".LLTT...",
    ]);
}

#[test]
fn layer_obscures_base_content() {
    let mut root = Stack::new(
        Pane::new().flex(1).children([Text::new().content("xxxxx")]),
    )
    .flex(1)
    .children([
        Pane::new().flex(1).children([Text::new().content("yyyyy")]),
    ]);
    let term = Emulator::new(&mut *root, Vec2::new(5, 1));
    term.assert_lines([
        "yyyyy",
    ]);
}

#[test]
fn resize_drives_base_and_layers() {
    let mut root = Stack::new(
        Pane::new().bordered().flex(1).children([Text::new().content("base")]),
    )
    .flex(1)
    .children([
        Pane::new().flex(1).children([
            Text::new().content("XX").width(2).height(1).margin_left(1).margin_top(1),
        ]),
    ]);
    let mut term = Emulator::new(&mut *root, Vec2::new(8, 3));
    term.assert_lines([
        "┌──────┐",
        "│XXse  │",
        "└──────┘",
    ]);
    term.update(&mut *root, &[RuntimeEvent::Resize(Vec2::new(10, 4))]);
    term.assert_lines([
        "┌────────┐",
        "│XXse    │",
        "│        │",
        "└────────┘",
    ]);
}

#[test]
fn layer_aligned_to_end() {
    let mut root = Stack::new(
        Pane::new().flex(1).children([Text::new().content("..........")]),
    )
    .flex(1)
    .children([
        Pane::new().flex(1).y_place(Place::End).x_place(Place::End).children([
            Text::new().content("END").width(3).height(1),
        ]),
    ]);
    let term = Emulator::new(&mut *root, Vec2::new(10, 1));
    term.assert_lines([
        ".......END",
    ]);
}

#[test]
fn layer_click_takes_precedence_over_base() {
    let base_text = Text::new().content("aaaaaa").width(6).height(1);
    let layer_text = Text::new().content("LL").width(2).height(1).margin_left(2);
    let layer_id = layer_text.get_id();

    let mut root = Stack::new(Pane::new().flex(1).children([base_text]))
        .flex(1)
        .children([Pane::new().flex(1).children([layer_text])]);
    let _term = Emulator::new(&mut *root, Vec2::new(6, 1));

    let mut path: Vec<WidgetId> = Vec::new();
    let hit = root.descendant_at_pos(Vec2::new(3.0, 0.0), Some(&mut path));
    assert_eq!(hit, Some(layer_id.untyped()));
}

#[test]
fn click_outside_layer_hits_base() {
    let base_text = Text::new().content("aaaaaa").width(6).height(1);
    let base_id = base_text.get_id();
    let layer_text = Text::new().content("LL").width(2).height(1).margin_left(2);

    let mut root = Stack::new(Pane::new().flex(1).children([base_text]))
        .flex(1)
        .children([Pane::new().flex(1).children([layer_text])]);
    let _term = Emulator::new(&mut *root, Vec2::new(6, 1));

    let hit = root.descendant_at_pos(Vec2::new(5.0, 0.0), None);
    assert_eq!(hit, Some(base_id.untyped()));
}

#[test]
fn nested_stacks_outer_layer_on_top() {
    let inner = Stack::new(
        Pane::new().flex(1).children([Text::new().content("BBBBBB")]),
    )
    .flex(1)
    .children([
        Pane::new().flex(1).children([
            Text::new().content("ii").width(2).height(1).margin_left(1),
        ]),
    ]);

    let mut root = Stack::new(inner).flex(1).children([
        Pane::new().flex(1).children([
            Text::new().content("OO").width(2).height(1).margin_left(4),
        ]),
    ]);
    let term = Emulator::new(&mut *root, Vec2::new(6, 1));
    term.assert_lines([
        "BiiBOO",
    ]);
}

#[test]
fn clear_and_add_child_round_trip() {
    let mut root = Stack::new(
        Pane::new().flex(1).children([Text::new().content("xxxxx")]),
    )
    .flex(1)
    .children([
        Pane::new().flex(1).children([Text::new().content("yyyyy")]),
    ]);

    let mut term = Emulator::new(&mut *root, Vec2::new(5, 1));
    term.assert_lines(["yyyyy"]);

    root.clear();
    term.update(&mut *root, &[RuntimeEvent::Resize(Vec2::new(5, 1))]);
    term.assert_lines(["xxxxx"]);

    root.add_child(
        Pane::new().flex(1).children([
            Text::new().content("zz").width(2).height(1),
        ]),
    );
    term.update(&mut *root, &[RuntimeEvent::Resize(Vec2::new(5, 1))]);
    term.assert_lines(["zzxxx"]);
}

#[test]
fn remove_child_by_id() {
    let layer = Pane::new().flex(1).children([Text::new().content("yyyyy")]);
    let layer_id = layer.get_id();

    let mut root = Stack::new(
        Pane::new().flex(1).children([Text::new().content("xxxxx")]),
    )
    .flex(1)
    .children([layer]);

    let mut term = Emulator::new(&mut *root, Vec2::new(5, 1));
    term.assert_lines(["yyyyy"]);

    root.remove(layer_id.untyped());
    term.update(&mut *root, &[RuntimeEvent::Resize(Vec2::new(5, 1))]);
    term.assert_lines(["xxxxx"]);
}
