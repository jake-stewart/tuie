use chord_macro::chord;
use tuie::prelude::*;
use tuie::emulator::Emulator;

fn pane_with_text(s: &str) -> Box<Pane> {
    Pane::new().children([Text::new().content(s.to_string())])
}

#[test]
fn renders_two_pane_horizontal_split() {
    let mut split = Split::new(
        SplitPane::horizontal().children([
            SplitPaneChild::from(pane_with_text("L")),
            SplitPaneChild::from(pane_with_text("R")),
        ]),
    );
    let term = Emulator::new(&mut *split, Vec2::new(10, 3));
    let snap = term.get_snapshot_text();
    let rows: Vec<&str> = snap.split('\n').collect();
    assert_eq!(rows.len(), 3);
    for row in &rows {
        assert_eq!(row.chars().count(), 10);
    }
    for row in &rows {
        assert!(row.contains('│'), "vertical divider on every row: {:?}", row);
    }
    assert!(snap.contains('L'), "left pane content: {:?}", snap);
    assert!(snap.contains('R'), "right pane content: {:?}", snap);
}

#[test]
fn renders_two_pane_vertical_split() {
    let mut split = Split::new(
        SplitPane::vertical().children([
            SplitPaneChild::from(pane_with_text("top")),
            SplitPaneChild::from(pane_with_text("bot")),
        ]),
    );
    let term = Emulator::new(&mut *split, Vec2::new(10, 8));
    let snap = term.get_snapshot_text();
    let rows: Vec<&str> = snap.split('\n').collect();
    assert_eq!(rows.len(), 8);
    assert!(snap.contains("top"), "top pane content: {:?}", snap);
    assert!(snap.contains("bot"), "bottom pane content: {:?}", snap);
    let any_horizontal_divider = rows.iter().any(|r| r.chars().filter(|&c| c == '─').count() >= 4);
    assert!(any_horizontal_divider, "expected at least one horizontal divider row, got {:?}", rows);
}

#[test]
fn renders_three_pane_horizontal_split() {
    let mut split = Split::new(
        SplitPane::horizontal().children([
            SplitPaneChild::from(pane_with_text("A")),
            SplitPaneChild::from(pane_with_text("B")),
            SplitPaneChild::from(pane_with_text("C")),
        ]),
    );
    let term = Emulator::new(&mut *split, Vec2::new(15, 3));
    let snap = term.get_snapshot_text();
    assert!(snap.contains('A'));
    assert!(snap.contains('B'));
    assert!(snap.contains('C'));
    for row in snap.split('\n') {
        let dividers = row.chars().filter(|&c| c == '│').count();
        assert_eq!(dividers, 2, "two vertical dividers between three panes: {:?}", row);
    }
}

#[test]
fn flex_ratio_distributes_widths() {
    let left = Pane::new().flex(1).children([Text::new().content("L")]);
    let right = Pane::new().flex(3).children([Text::new().content("R")]);
    let mut split = Split::new(
        SplitPane::horizontal().children([
            SplitPaneChild::from(left),
            SplitPaneChild::from(right),
        ]),
    );
    let term = Emulator::new(&mut *split, Vec2::new(20, 3));
    let top = term.get_snapshot_text().split('\n').next().unwrap().to_string();
    let pivot_col = top.chars().position(|c| c == '│').expect("vertical divider on top row");
    let left_cells = pivot_col;
    let right_cells = top.chars().count() - pivot_col - 1;
    assert!(
        right_cells > left_cells * 2,
        "right (flex=3) should be much wider than left (flex=1), got L={} R={} top={:?}",
        left_cells,
        right_cells,
        top
    );
}

#[test]
fn nested_splits_render() {
    let inner = SplitPane::vertical().children([
        SplitPaneChild::from(pane_with_text("TR")),
        SplitPaneChild::from(pane_with_text("BR")),
    ]);
    let mut split = Split::new(
        SplitPane::horizontal().children([
            SplitPaneChild::from(pane_with_text("L")),
            SplitPaneChild::from(inner),
        ]),
    );
    let term = Emulator::new(&mut *split, Vec2::new(14, 7));
    let snap = term.get_snapshot_text();
    assert!(snap.contains('L'));
    assert!(snap.contains("TR"));
    assert!(snap.contains("BR"));
    let any_t_left = snap.chars().any(|c| c == '├');
    assert!(any_t_left, "expected ├ junction where inner vertical divider meets outer vertical divider: {:?}", snap);
}

#[test]
fn resize_reflows_split() {
    let mut split = Split::new(
        SplitPane::horizontal().children([
            SplitPaneChild::from(pane_with_text("L")),
            SplitPaneChild::from(pane_with_text("R")),
        ]),
    );
    let mut term = Emulator::new(&mut *split, Vec2::new(10, 3));
    let small_top = term.get_snapshot_text().split('\n').next().unwrap().to_string();
    assert_eq!(small_top.chars().count(), 10);

    term.update(&mut *split, &[RuntimeEvent::Resize(Vec2::new(30, 5))]);
    let big_snap = term.get_snapshot_text();
    let big_rows: Vec<&str> = big_snap.split('\n').collect();
    assert_eq!(big_rows.len(), 5);
    for row in &big_rows {
        assert_eq!(row.chars().count(), 30);
    }
    assert!(big_snap.contains('L'));
    assert!(big_snap.contains('R'));
}

#[test]
fn mouse_drag_moves_divider() {
    let left = Pane::new().flex(1).children([Text::new().content("L")]);
    let right = Pane::new().flex(1).children([Text::new().content("R")]);
    let mut split = Split::new(
        SplitPane::horizontal().children([
            SplitPaneChild::from(left),
            SplitPaneChild::from(right),
        ]),
    );
    let mut term = Emulator::new(&mut *split, Vec2::new(20, 5));
    let before_top = term.get_snapshot_text().split('\n').next().unwrap().to_string();
    let before_pivot = before_top.chars().position(|c| c == '│').expect("vertical divider on top row") as i32;

    term.update(&mut *split, &[
        RuntimeEvent::input_at(chord!(LeftClick), Vec2::new(before_pivot, 2)),
        RuntimeEvent::input_at(chord!(LeftDrag), Vec2::new(before_pivot + 4, 2)),
        RuntimeEvent::input_at(chord!(LeftRelease), Vec2::new(before_pivot + 4, 2)),
    ]);

    let after_top = term.get_snapshot_text().split('\n').next().unwrap().to_string();
    let after_pivot = after_top
        .chars()
        .enumerate()
        .filter_map(|(i, c)| (c == '│').then_some(i))
        .last()
        .expect("vertical divider still on top row") as i32;
    assert!(
        after_pivot > before_pivot,
        "divider should have moved right after drag, before={} after={} top={:?}",
        before_pivot,
        after_pivot,
        after_top
    );
}

#[test]
fn outer_border_wraps_split() {
    let mut split = Split::new(
        SplitPane::horizontal().children([
            SplitPaneChild::from(pane_with_text("L")).borderless(),
            SplitPaneChild::from(pane_with_text("R")).borderless(),
        ]),
    )
    .bordered()
    .border(Border::SINGLE);
    let term = Emulator::new(&mut *split, Vec2::new(10, 3));
    term.assert_lines([
        "┌────────┐",
        "│LR      │",
        "└────────┘",
    ]);
}

#[test]
fn minimum_width_constraint_respected() {
    let left = Pane::new().min_width(8).children([Text::new().content("L")]);
    let right = Pane::new().min_width(8).children([Text::new().content("R")]);
    let mut split = Split::new(
        SplitPane::horizontal().children([
            SplitPaneChild::from(left).borderless(),
            SplitPaneChild::from(right).borderless(),
        ]),
    );
    let constraints = split.measure_constraints();
    assert!(
        constraints.min_size.x >= 16,
        "min width should sum the two min_width=8 panes, got {}",
        constraints.min_size.x,
    );
}

#[test]
fn remove_collapses_pane() {
    let left = pane_with_text("L");
    let mid = pane_with_text("M");
    let right = pane_with_text("R");
    let left_id = left.get_id();
    let mid_id = mid.get_id();
    let right_id = right.get_id();
    let mut split = Split::new(
        SplitPane::horizontal().children([
            SplitPaneChild::from(left),
            SplitPaneChild::from(mid),
            SplitPaneChild::from(right),
        ]),
    );
    let term = Emulator::new(&mut *split, Vec2::new(15, 3));
    let before = term.get_snapshot_text();
    assert!(before.contains('M'));
    assert!(split.contains(mid_id));

    let removed = split.remove(mid_id);
    assert!(removed.is_some());
    assert!(!split.contains(mid_id));
    assert!(split.contains(left_id));
    assert!(split.contains(right_id));

    let term = Emulator::new(&mut *split, Vec2::new(15, 3));
    let after = term.get_snapshot_text();
    assert!(after.contains('L'));
    assert!(after.contains('R'));
    assert!(!after.contains('M'), "removed pane should not render: {:?}", after);
    let top = after.split('\n').next().unwrap();
    let dividers = top.chars().filter(|&c| c == '│').count();
    assert_eq!(dividers, 1, "exactly one vertical divider left after removing middle: {:?}", top);
}

#[test]
fn split_root_adds_pane_at_runtime() {
    let first = pane_with_text("1");
    let mut split = Split::new(
        SplitPane::horizontal().children([SplitPaneChild::from(first)]),
    );
    let mut term = Emulator::new(&mut *split, Vec2::new(20, 3));
    assert!(term.get_snapshot_text().contains('1'));

    split.split_root(
        SplitPaneChild::from(pane_with_text("2")),
        Axis2D::X,
        Sign::Positive,
    );
    split.redistribute();
    term.update(&mut *split, &[RuntimeEvent::Resize(Vec2::new(20, 3))]);
    let after = term.get_snapshot_text();
    assert!(after.contains('1'), "old pane content after split_root: {:?}", after);
    assert!(after.contains('2'), "new pane content after split_root: {:?}", after);
    let top = after.split('\n').next().unwrap();
    assert!(top.contains('│'), "vertical divider appears after adding second pane: {:?}", top);
}
