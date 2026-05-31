//! Integration tests for tooltip rendering.

use tuie::prelude::*;
use tuie::emulator::Emulator;

fn anchor_text(s: &str) -> Box<Text> {
    Text::new().content(s.to_string()).width(s.chars().count() as u16).height(1)
}

fn body_pane(s: &str) -> Box<Pane> {
    Pane::new().children([Text::new().content(s.to_string())]).bordered()
}

#[test]
fn hidden_by_default_renders_only_anchor() {
    let mut root = Stack::new(
        Pane::new().children([
            Tooltip::new(anchor_text("AA"))
                .content(body_pane("BODY")),
        ]),
    );
    let term = Emulator::new(&mut *root, Vec2::new(8, 3));
    term.assert_lines([
        "AA      ",
        "        ",
        "        ",
    ]);
}

#[test]
fn visible_builder_shows_body() {
    let mut root = Stack::new(
        Pane::new().children([
            Tooltip::new(anchor_text("AA"))
                .content(body_pane("BODY"))
                .placement(Placement::side(Direction2D::Down, Sign::Positive, Align::Start))
                .visible(),
        ]),
    );
    let term = Emulator::new(&mut *root, Vec2::new(8, 4));
    let snap = term.get_snapshot_text();
    assert!(snap.contains("AA"), "anchor present: {snap:?}");
    assert!(snap.contains("BODY"), "body present: {snap:?}");
    assert!(snap.contains('┌') && snap.contains('┘'), "border present: {snap:?}");
}

#[test]
fn set_visible_toggles_body() {
    let tooltip = Tooltip::new(anchor_text("AA"))
        .content(body_pane("HI"))
        .placement(Placement::side(Direction2D::Down, Sign::Positive, Align::Start));
    let tooltip_id = tooltip.get_id();

    let mut root = Stack::new(Pane::new().children([tooltip]));
    let mut term = Emulator::new(&mut *root, Vec2::new(8, 4));

    assert!(!term.get_snapshot_text().contains("HI"));

    if let Some(t) = root.get_widget_mut(tooltip_id) {
        t.set_visible(true);
    }
    term.update(&mut *root, &[RuntimeEvent::Resize(Vec2::new(8, 4))]);
    assert!(term.get_snapshot_text().contains("HI"));

    if let Some(t) = root.get_widget_mut(tooltip_id) {
        t.set_visible(false);
    }
    term.update(&mut *root, &[RuntimeEvent::Resize(Vec2::new(8, 4))]);
    assert!(!term.get_snapshot_text().contains("HI"));
}

#[test]
fn placement_below_anchor() {
    let mut root = Stack::new(
        Pane::new().children([
            Tooltip::new(anchor_text("AA"))
                .content(body_pane("X"))
                .placement(Placement::side(Direction2D::Down, Sign::Positive, Align::Start))
                .visible(),
        ]),
    );
    let term = Emulator::new(&mut *root, Vec2::new(8, 5));
    term.assert_lines([
        "AA      ",
        "┌─┐     ",
        "│X│     ",
        "└─┘     ",
        "        ",
    ]);
}

#[test]
fn placement_above_anchor() {
    let mut root = Stack::new(
        Pane::new().padding_top(3).children([
            Tooltip::new(anchor_text("AA"))
                .content(body_pane("X"))
                .placement(Placement::side(Direction2D::Up, Sign::Positive, Align::Start))
                .visible(),
        ]),
    );
    let term = Emulator::new(&mut *root, Vec2::new(8, 5));
    term.assert_lines([
        "┌─┐     ",
        "│X│     ",
        "└─┘     ",
        "AA      ",
        "        ",
    ]);
}

#[test]
fn placement_right_of_anchor() {
    let mut root = Stack::new(
        Pane::new().children([
            Tooltip::new(anchor_text("AA"))
                .content(body_pane("X"))
                .placement(Placement::side(Direction2D::Right, Sign::Positive, Align::Start))
                .visible(),
        ]),
    );
    let term = Emulator::new(&mut *root, Vec2::new(8, 3));
    term.assert_lines([
        "AA┌─┐   ",
        "  │X│   ",
        "  └─┘   ",
    ]);
}

#[test]
fn placement_left_of_anchor() {
    let mut root = Stack::new(
        Pane::new().padding_left(4).children([
            Tooltip::new(anchor_text("AA"))
                .content(body_pane("X"))
                .placement(Placement::side(Direction2D::Left, Sign::Positive, Align::Start))
                .visible(),
        ]),
    );
    let term = Emulator::new(&mut *root, Vec2::new(8, 3));
    term.assert_lines([
        " ┌─┐AA  ",
        " │X│    ",
        " └─┘    ",
    ]);
}

#[test]
fn body_with_offset_placement() {
    let placement = Placement::side(Direction2D::Down, Sign::Positive, Align::Start)
        .offset(Vec2::new(2, 0));
    let mut root = Stack::new(
        Pane::new().children([
            Tooltip::new(anchor_text("AA"))
                .content(body_pane("X"))
                .placement(placement)
                .visible(),
        ]),
    );
    let term = Emulator::new(&mut *root, Vec2::new(8, 5));
    term.assert_lines([
        "AA      ",
        "  ┌─┐   ",
        "  │X│   ",
        "  └─┘   ",
        "        ",
    ]);
}

#[test]
fn body_renders_over_other_content() {
    let mut root = Stack::new(
        Pane::new().children([Text::new().content("........")]),
    )
    .children([
        Pane::new().children([
            Tooltip::new(anchor_text("AA"))
                .content(body_pane("XX"))
                .placement(Placement::side(Direction2D::Down, Sign::Positive, Align::Start))
                .on_top()
                .visible(),
        ]),
    ]);
    let term = Emulator::new(&mut *root, Vec2::new(8, 4));
    term.assert_lines([
        "AA......",
        "┌──┐    ",
        "│XX│    ",
        "└──┘    ",
    ]);
}

#[test]
fn multiline_body_content() {
    let body = Pane::new().vertical().children([
        Text::new().content("line1") as Box<dyn Widget>,
        Text::new().content("line2"),
    ]).bordered();

    let mut root = Stack::new(
        Pane::new().children([
            Tooltip::new(anchor_text("AA"))
                .content(body)
                .placement(Placement::side(Direction2D::Down, Sign::Positive, Align::Start))
                .visible(),
        ]),
    );
    let term = Emulator::new(&mut *root, Vec2::new(9, 5));
    term.assert_lines([
        "AA       ",
        "┌─────┐  ",
        "│line1│  ",
        "│line2│  ",
        "└─────┘  ",
    ]);
}

#[test]
fn anchor_visible_without_body() {
    let mut root = Stack::new(
        Pane::new().children([Tooltip::new(anchor_text("HELLO"))]),
    );
    let term = Emulator::new(&mut *root, Vec2::new(8, 2));
    term.assert_lines([
        "HELLO   ",
        "        ",
    ]);
}

#[test]
fn is_visible_reflects_state() {
    let mut tooltip = Tooltip::new(anchor_text("AA"))
        .content(body_pane("X"));
    assert!(!tooltip.is_visible());

    tooltip.set_visible(true);
    assert!(tooltip.is_visible());

    tooltip.set_visible(false);
    assert!(!tooltip.is_visible());
}

#[test]
fn on_top_changes_z_order() {
    let tooltip = Tooltip::new(anchor_text("AA"))
        .content(body_pane("X"))
        .placement(Placement::side(Direction2D::Down, Sign::Positive, Align::Start))
        .visible();
    assert!(!tooltip.is_on_top());

    let tooltip_top = Tooltip::new(anchor_text("AA"))
        .content(body_pane("X"))
        .placement(Placement::side(Direction2D::Down, Sign::Positive, Align::Start))
        .on_top()
        .visible();
    assert!(tooltip_top.is_on_top());
}
