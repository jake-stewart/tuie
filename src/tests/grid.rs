//! Tests for the [`Grid`] widget.

use chord_macro::chord;
use tuie::prelude::*;
use tuie::emulator::Emulator;

fn cell_text(s: &'static str) -> Box<dyn Widget> {
    Text::new().content(s) as Box<dyn Widget>
}

fn bg_grid(snap: &StyledString) -> Vec<Vec<Option<Color>>> {
    color_grid(snap, |s| s.get_bg())
}

fn fg_grid(snap: &StyledString) -> Vec<Vec<Option<Color>>> {
    color_grid(snap, |s| s.get_fg())
}

fn color_grid(
    snap: &StyledString,
    pick: impl Fn(&tuie::render::style::Style) -> Option<Color>,
) -> Vec<Vec<Option<Color>>> {
    let mut grid: Vec<Vec<Option<Color>>> = Vec::new();
    let mut row: Vec<Option<Color>> = Vec::new();
    for (chunk, style) in snap.iter_chunks(..) {
        let color = pick(&style);
        for ch in chunk.chars() {
            if ch == '\n' {
                grid.push(std::mem::take(&mut row));
            } else {
                row.push(color);
            }
        }
    }
    if !row.is_empty() {
        grid.push(row);
    }
    grid
}

#[test]
fn empty_grid_renders_blank() {
    let mut root = Grid::new();
    let term = Emulator::new(&mut *root, Vec2::new(4, 2));
    term.assert_lines([
        "    ",
        "    ",
    ]);
}

#[test]
fn single_cell_single_track_fills_area() {
    let mut root = Grid::new()
        .columns([Track::grow(1)])
        .rows([Track::grow(1)])
        .child(0, 0, cell_text("hi"));
    let term = Emulator::new(&mut *root, Vec2::new(4, 2));
    term.assert_lines([
        "hi  ",
        "    ",
    ]);
}

#[test]
fn two_fixed_columns_size_to_basis() {
    let mut root = Grid::new()
        .columns([Track::fixed(3), Track::fixed(5)])
        .rows([Track::grow(1)])
        .child(0, 0, cell_text("AAA"))
        .child(0, 1, cell_text("BBBBB"));
    let term = Emulator::new(&mut *root, Vec2::new(10, 1));
    term.assert_lines([
        "AAABBBBB  ",
    ]);
}

#[test]
fn two_equal_flex_columns_split_evenly() {
    let mut root = Grid::new()
        .columns([Track::grow(1), Track::grow(1)])
        .rows([Track::grow(1)])
        .child(0, 0, cell_text("L"))
        .child(0, 1, cell_text("R"));
    let term = Emulator::new(&mut *root, Vec2::new(6, 1));
    term.assert_lines([
        "L  R  ",
    ]);
}

#[test]
fn flex_weights_distribute_proportionally() {
    let mut root = Grid::new()
        .columns([Track::grow(1), Track::grow(3)])
        .rows([Track::grow(1)])
        .child(0, 0, cell_text("a"))
        .child(0, 1, cell_text("b"));
    let term = Emulator::new(&mut *root, Vec2::new(8, 1));
    term.assert_lines([
        "a  b    ",
    ]);
}

#[test]
fn fixed_plus_flex_column() {
    let mut root = Grid::new()
        .columns([Track::fixed(2), Track::grow(1)])
        .rows([Track::grow(1)])
        .child(0, 0, cell_text("XX"))
        .child(0, 1, cell_text("YYYY"));
    let term = Emulator::new(&mut *root, Vec2::new(6, 1));
    term.assert_lines([
        "XXYYYY",
    ]);
}

#[test]
fn two_fixed_rows_size_to_basis() {
    let mut root = Grid::new()
        .columns([Track::grow(1)])
        .rows([Track::fixed(1), Track::fixed(2)])
        .child(0, 0, cell_text("top"))
        .child(1, 0, cell_text("bot"));
    let term = Emulator::new(&mut *root, Vec2::new(4, 4));
    term.assert_lines([
        "top ",
        "bot ",
        "    ",
        "    ",
    ]);
}

#[test]
fn col_gap_inserts_space_between_columns() {
    let mut root = Grid::new()
        .columns([Track::grow(1), Track::grow(1)])
        .rows([Track::grow(1)])
        .col_gap(2)
        .child(0, 0, cell_text("A"))
        .child(0, 1, cell_text("B"));
    let term = Emulator::new(&mut *root, Vec2::new(8, 1));
    term.assert_lines([
        "A    B  ",
    ]);
}

#[test]
fn row_gap_inserts_blank_row_between_rows() {
    let mut root = Grid::new()
        .columns([Track::grow(1)])
        .rows([Track::fixed(1), Track::fixed(1)])
        .row_gap(1)
        .child(0, 0, cell_text("up"))
        .child(1, 0, cell_text("dn"));
    let term = Emulator::new(&mut *root, Vec2::new(3, 3));
    term.assert_lines([
        "up ",
        "   ",
        "dn ",
    ]);
}

#[test]
fn col_span_widens_cell_across_columns() {
    let mut root = Grid::new()
        .columns([Track::grow(1), Track::grow(1), Track::grow(1)])
        .rows([Track::grow(1)])
        .cell(Cell::new(0, 0, cell_text("HEADER")).span(1, 3));
    let term = Emulator::new(&mut *root, Vec2::new(9, 1));
    term.assert_lines([
        "HEADER   ",
    ]);
}

#[test]
fn row_span_extends_cell_across_rows() {
    let mut root = Grid::new()
        .columns([Track::grow(1)])
        .rows([Track::fixed(1), Track::fixed(1), Track::fixed(1)])
        .cell(Cell::new(0, 0, cell_text("S")).span(3, 1));
    let term = Emulator::new(&mut *root, Vec2::new(2, 3));
    term.assert_lines([
        "S ",
        "  ",
        "  ",
    ]);
}

#[test]
fn col_span_includes_inner_gaps() {
    let mut root = Grid::new()
        .columns([Track::fixed(2), Track::fixed(2), Track::fixed(2)])
        .rows([Track::grow(1)])
        .col_gap(1)
        .cell(Cell::new(0, 0, cell_text("ABCDEFGH")).span(1, 3));
    let term = Emulator::new(&mut *root, Vec2::new(10, 1));
    term.assert_lines([
        "ABCDEFGH  ",
    ]);
}

#[test]
fn resize_redistributes_flex() {
    let mut root = Grid::new()
        .columns([Track::grow(1), Track::grow(1)])
        .rows([Track::grow(1)])
        .child(0, 0, cell_text("L"))
        .child(0, 1, cell_text("R"));
    let mut term = Emulator::new(&mut *root, Vec2::new(4, 1));
    term.assert_lines(["L R "]);
    term.update(&mut *root, &[RuntimeEvent::Resize(Vec2::new(8, 1))]);
    term.assert_lines(["L   R   "]);
}

#[test]
fn resize_taller_redistributes_rows() {
    let mut root = Grid::new()
        .columns([Track::grow(1)])
        .rows([Track::grow(1), Track::grow(1)])
        .child(0, 0, cell_text("t"))
        .child(1, 0, cell_text("b"));
    let mut term = Emulator::new(&mut *root, Vec2::new(2, 4));
    term.assert_lines([
        "t ",
        "  ",
        "b ",
        "  ",
    ]);
    term.update(&mut *root, &[RuntimeEvent::Resize(Vec2::new(2, 2))]);
    term.assert_lines([
        "t ",
        "b ",
    ]);
}

#[test]
fn align_items_x_middle_centers_each_child_in_cell() {
    let mut root = Grid::new()
        .columns([Track::fixed(6)])
        .rows([Track::grow(1)])
        .x_place(Place::Center)
        .child(0, 0, cell_text("ab"));
    let term = Emulator::new(&mut *root, Vec2::new(6, 1));
    term.assert_lines([
        "  ab  ",
    ]);
}

#[test]
fn align_items_x_end_right_aligns_child_in_cell() {
    let mut root = Grid::new()
        .columns([Track::fixed(6)])
        .rows([Track::grow(1)])
        .x_place(Place::End)
        .child(0, 0, cell_text("xy"));
    let term = Emulator::new(&mut *root, Vec2::new(6, 1));
    term.assert_lines([
        "    xy",
    ]);
}

#[test]
fn align_items_y_middle_centers_child_vertically() {
    let mut root = Grid::new()
        .columns([Track::grow(1)])
        .rows([Track::fixed(3)])
        .y_place(Place::Center)
        .child(0, 0, cell_text("z"));
    let term = Emulator::new(&mut *root, Vec2::new(2, 3));
    term.assert_lines([
        "  ",
        "z ",
        "  ",
    ]);
}

#[test]
fn align_items_y_end_bottoms_child_in_cell() {
    let mut root = Grid::new()
        .columns([Track::grow(1)])
        .rows([Track::fixed(3)])
        .y_place(Place::End)
        .child(0, 0, cell_text("z"));
    let term = Emulator::new(&mut *root, Vec2::new(2, 3));
    term.assert_lines([
        "  ",
        "  ",
        "z ",
    ]);
}

#[test]
fn justify_content_x_apart_pushes_tracks_to_edges() {
    let mut root = Grid::new()
        .columns([Track::fixed(2), Track::fixed(2)])
        .rows([Track::grow(1)])
        .x_place(Place::Apart)
        .child(0, 0, cell_text("AA"))
        .child(0, 1, cell_text("BB"));
    let term = Emulator::new(&mut *root, Vec2::new(8, 1));
    term.assert_lines([
        "AA    BB",
    ]);
}

#[test]
fn justify_content_y_apart_separates_rows_vertically() {
    let mut root = Grid::new()
        .columns([Track::grow(1)])
        .rows([Track::fixed(1), Track::fixed(1)])
        .y_place(Place::Apart)
        .child(0, 0, cell_text("T"))
        .child(1, 0, cell_text("B"));
    let term = Emulator::new(&mut *root, Vec2::new(2, 4));
    term.assert_lines([
        "T ",
        "  ",
        "  ",
        "B ",
    ]);
}

#[test]
fn add_child_after_creation_renders() {
    let mut root = Grid::new()
        .columns([Track::grow(1), Track::grow(1)])
        .rows([Track::grow(1)]);
    root.add_child(0, 0, cell_text("A"));
    root.add_child(0, 1, cell_text("B"));
    let term = Emulator::new(&mut *root, Vec2::new(6, 1));
    term.assert_lines(["A  B  "]);
}

#[test]
fn remove_at_drops_child_at_origin() {
    let mut root = Grid::new()
        .columns([Track::grow(1), Track::grow(1)])
        .rows([Track::grow(1)])
        .child(0, 0, cell_text("XX"))
        .child(0, 1, cell_text("YY"));
    let mut term = Emulator::new(&mut *root, Vec2::new(6, 1));
    term.assert_lines(["XX YY "]);
    let removed = root.remove_at(0, 0);
    assert!(removed.is_some());
    term.update(&mut *root, &[RuntimeEvent::Resize(Vec2::new(6, 1))]);
    term.assert_lines(["  YY  "]);
}

#[test]
fn remove_at_nonexistent_returns_none() {
    let mut root = Grid::new()
        .columns([Track::grow(1)])
        .rows([Track::grow(1)])
        .child(0, 0, cell_text("z"));
    assert!(root.remove_at(1, 0).is_none());
    assert!(root.remove_at(0, 1).is_none());
}

#[test]
fn remove_by_id_drops_correct_child() {
    let target = Text::new().content("GONE");
    let target_id = target.get_id();
    let mut root = Grid::new()
        .columns([Track::grow(1), Track::grow(1)])
        .rows([Track::grow(1)])
        .child(0, 0, target as Box<dyn Widget>)
        .child(0, 1, cell_text("KEEP"));
    let mut term = Emulator::new(&mut *root, Vec2::new(10, 1));
    term.assert_lines(["GONE KEEP "]);
    let removed = root.remove(target_id);
    assert!(removed.is_some());
    term.update(&mut *root, &[RuntimeEvent::Resize(Vec2::new(10, 1))]);
    term.assert_lines(["   KEEP   "]);
}

#[test]
fn remove_by_unknown_id_returns_none() {
    let stray = Text::new().content("not in grid");
    let stray_id = stray.get_id();
    let mut root = Grid::new()
        .columns([Track::grow(1)])
        .rows([Track::grow(1)])
        .child(0, 0, cell_text("a"));
    assert!(root.remove(stray_id).is_none());
    drop(stray);
}

#[test]
fn clear_removes_all_children() {
    let mut root = Grid::new()
        .columns([Track::grow(1), Track::grow(1)])
        .rows([Track::grow(1)])
        .child(0, 0, cell_text("A"))
        .child(0, 1, cell_text("B"));
    let mut term = Emulator::new(&mut *root, Vec2::new(6, 1));
    term.assert_lines(["A  B  "]);
    root.clear();
    term.update(&mut *root, &[RuntimeEvent::Resize(Vec2::new(6, 1))]);
    term.assert_lines(["      "]);
}

#[test]
fn set_columns_replaces_tracks() {
    let mut root = Grid::new()
        .columns([Track::grow(1)])
        .rows([Track::grow(1)])
        .child(0, 0, cell_text("a"))
        .child(0, 1, cell_text("b"));
    let mut term = Emulator::new(&mut *root, Vec2::new(6, 1));
    root.set_columns(vec![Track::grow(1), Track::grow(1)]);
    term.update(&mut *root, &[RuntimeEvent::Resize(Vec2::new(6, 1))]);
    term.assert_lines(["a  b  "]);
}

#[test]
fn set_rows_replaces_tracks() {
    let mut root = Grid::new()
        .columns([Track::grow(1)])
        .rows([Track::grow(1)])
        .child(0, 0, cell_text("a"))
        .child(1, 0, cell_text("b"));
    let mut term = Emulator::new(&mut *root, Vec2::new(2, 4));
    root.set_rows(vec![Track::grow(1), Track::grow(1)]);
    term.update(&mut *root, &[RuntimeEvent::Resize(Vec2::new(2, 4))]);
    term.assert_lines([
        "a ",
        "  ",
        "b ",
        "  ",
    ]);
}

#[test]
fn descendant_at_pos_hits_correct_cell() {
    let a = Text::new().content("AA");
    let a_id = a.get_id();
    let b = Text::new().content("BB");
    let b_id = b.get_id();
    let mut root = Grid::new()
        .columns([Track::grow(1), Track::grow(1)])
        .rows([Track::grow(1)])
        .child(0, 0, a as Box<dyn Widget>)
        .child(0, 1, b as Box<dyn Widget>);
    let _term = Emulator::new(&mut *root, Vec2::new(4, 1));
    let hit_left = root.descendant_at_pos(Vec2::new(0.0, 0.0), None);
    let hit_right = root.descendant_at_pos(Vec2::new(3.0, 0.0), None);
    assert_eq!(hit_left, Some(a_id.untyped()));
    assert_eq!(hit_right, Some(b_id.untyped()));
}

#[test]
fn find_descendant_locates_descendant_id() {
    let buried = Text::new().content("needle");
    let buried_id = buried.get_id();
    let root = Grid::new()
        .columns([Track::grow(1)])
        .rows([Track::grow(1)])
        .child(
            0,
            0,
            Pane::new().children([buried as Box<dyn Widget>]) as Box<dyn Widget>,
        );
    let found = root.find_descendant(&|w| w.get_id() == buried_id.untyped(), None);
    assert_eq!(found, Some(buried_id.untyped()));
}

#[test]
fn each_child_iterates_all_cells() {
    let root = Grid::new()
        .columns([Track::grow(1), Track::grow(1)])
        .rows([Track::grow(1), Track::grow(1)])
        .child(0, 0, cell_text("a"))
        .child(0, 1, cell_text("b"))
        .child(1, 0, cell_text("c"))
        .child(1, 1, cell_text("d"));
    let mut count = 0;
    root.each_child(&mut |_| count += 1, Sign::Positive);
    assert_eq!(count, 4);
}

#[test]
fn builder_chain_composes_full_grid() {
    let mut root = Grid::new()
        .columns([Track::fixed(2), Track::grow(1)])
        .rows([Track::fixed(1), Track::grow(1)])
        .col_gap(1)
        .row_gap(1)
        .child(0, 0, cell_text("a"))
        .child(0, 1, cell_text("bb"))
        .child(1, 0, cell_text("c"))
        .child(1, 1, cell_text("dd"));
    let term = Emulator::new(&mut *root, Vec2::new(8, 4));
    term.assert_lines([
        "a  bb   ",
        "        ",
        "c  dd   ",
        "        ",
    ]);
}

#[test]
fn spans_cross_row_gap_correctly() {
    let mut root = Grid::new()
        .columns([Track::grow(1)])
        .rows([Track::fixed(1), Track::fixed(1)])
        .row_gap(1)
        .cell(Cell::new(0, 0, cell_text("X")).span(2, 1));
    let term = Emulator::new(&mut *root, Vec2::new(2, 3));
    term.assert_lines([
        "X ",
        "  ",
        "  ",
    ]);
}

#[test]
fn nested_grids_compose() {
    let inner = Grid::new()
        .columns([Track::grow(1), Track::grow(1)])
        .rows([Track::grow(1)])
        .child(0, 0, cell_text("aa"))
        .child(0, 1, cell_text("bb"));
    let mut root = Grid::new()
        .columns([Track::grow(1)])
        .rows([Track::fixed(1), Track::fixed(1)])
        .child(0, 0, inner as Box<dyn Widget>)
        .child(1, 0, cell_text("ZZ"));
    let term = Emulator::new(&mut *root, Vec2::new(4, 2));
    term.assert_lines([
        "aabb",
        "ZZ  ",
    ]);
}

#[test]
fn click_in_cell_routes_to_child() {
    let target = Text::new().content("hit me");
    let target_id = target.get_id();
    let mut root = Grid::new()
        .columns([Track::grow(1), Track::grow(1)])
        .rows([Track::grow(1)])
        .child(0, 0, cell_text("left"))
        .child(0, 1, target as Box<dyn Widget>);
    let _term = Emulator::new(&mut *root, Vec2::new(12, 1));
    let hit = root.descendant_at_pos(Vec2::new(10.0, 0.0), None);
    assert_eq!(hit, Some(target_id.untyped()));
}

#[test]
fn input_routes_into_cell_widget() {
    let mut root = Grid::new()
        .columns([Track::grow(1)])
        .rows([Track::grow(1)])
        .child(0, 0, Input::new().flex(1) as Box<dyn Widget>);
    let mut term = Emulator::new(&mut *root, Vec2::new(6, 1));
    term.update(
        &mut *root,
        &[
            RuntimeEvent::input_at(chord!(LeftClick), Vec2::new(0, 0)),
            chord!('h').into(),
            chord!('i').into(),
        ],
    );
    let snap = term.get_snapshot_text();
    assert!(snap.contains("hi"), "expected 'hi' in snapshot, got:\n{snap}");
}

#[test]
fn track_basis_is_lower_bound_for_fixed() {
    let mut root = Grid::new()
        .columns([Track::fixed(4)])
        .rows([Track::grow(1)])
        .child(0, 0, cell_text("ab"));
    let term = Emulator::new(&mut *root, Vec2::new(6, 1));
    term.assert_lines([
        "ab    ",
    ]);
}

#[test]
fn multiple_children_at_same_origin_both_render() {
    let mut root = Grid::new()
        .columns([Track::grow(1)])
        .rows([Track::grow(1)])
        .child(0, 0, cell_text("AAA"))
        .child(0, 0, cell_text("B"));
    let term = Emulator::new(&mut *root, Vec2::new(4, 1));
    term.assert_lines([
        "BAA ",
    ]);
}

#[test]
fn remove_at_only_removes_first_match() {
    let mut root = Grid::new()
        .columns([Track::grow(1)])
        .rows([Track::grow(1)])
        .child(0, 0, cell_text("first"))
        .child(0, 0, cell_text("second"));
    assert!(root.remove_at(0, 0).is_some());
    let mut term = Emulator::new(&mut *root, Vec2::new(7, 1));
    term.update(&mut *root, &[RuntimeEvent::Resize(Vec2::new(7, 1))]);
    let snap = term.get_snapshot_text();
    assert!(snap.contains("second"), "expected 'second' to remain, got:\n{snap}");
    assert!(!snap.contains("first"), "expected 'first' gone, got:\n{snap}");
}

#[test]
fn border_external_only() {
    let mut root = Grid::new()
        .columns([Track::grow(1)])
        .rows([Track::grow(1)])
        .border(Border::SINGLE)
        .child(0, 0, cell_text("X"));
    let term = Emulator::new(&mut *root, Vec2::new(3, 3));
    term.assert_lines([
        "┌─┐",
        "│X│",
        "└─┘",
    ]);
}

#[test]
fn border_compact_rows_separates() {
    let mut root = Grid::new()
        .columns([Track::grow(1)])
        .rows([Track::fixed(1), Track::fixed(1)])
        .row_borders(Border::SINGLE)
        .child(0, 0, cell_text("A"))
        .child(1, 0, cell_text("B"));
    let term = Emulator::new(&mut *root, Vec2::new(1, 3));
    term.assert_lines([
        "A",
        "─",
        "B",
    ]);
}

#[test]
fn border_row_override_replaces_compact() {
    let mut root = Grid::new()
        .columns([Track::grow(1)])
        .rows([Track::fixed(1), Track::fixed(1)])
        .row_borders(Border::SINGLE)
        .row_top(1, Border::THICK)
        .child(0, 0, cell_text("A"))
        .child(1, 0, cell_text("B"));
    let term = Emulator::new(&mut *root, Vec2::new(1, 3));
    term.assert_lines([
        "A",
        "━",
        "B",
    ]);
}

#[test]
fn border_cell_beats_row() {
    let mut root = Grid::new()
        .columns([Track::grow(1)])
        .rows([Track::fixed(1), Track::fixed(1)])
        .row_top(1, Border::SINGLE)
        .child(0, 0, cell_text("A"))
        .cell(Cell::new(1, 0, cell_text("B")).border(Border::THICK));
    let term = Emulator::new(&mut *root, Vec2::new(3, 3));
    term.assert_lines([
        " A ",
        "┏━┓",
        "┃B┃",
    ]);
}

#[test]
fn border_bottom_beats_top_tiebreak() {
    let mut root = Grid::new()
        .columns([Track::grow(1)])
        .rows([Track::fixed(1), Track::fixed(1)])
        .row_bottom(0, Border::SINGLE)
        .row_top(1, Border::THICK)
        .child(0, 0, cell_text("A"))
        .child(1, 0, cell_text("B"));
    let term = Emulator::new(&mut *root, Vec2::new(1, 3));
    term.assert_lines([
        "A",
        "─",
        "B",
    ]);
}

#[test]
fn border_junction_glyph_internal_cross() {
    let mut root = Grid::new()
        .columns([Track::fixed(1), Track::fixed(1)])
        .rows([Track::fixed(1), Track::fixed(1)])
        .border(Border::SINGLE)
        .row_borders(Border::SINGLE)
        .col_borders(Border::SINGLE)
        .child(0, 0, cell_text("A"))
        .child(0, 1, cell_text("B"))
        .child(1, 0, cell_text("C"))
        .child(1, 1, cell_text("D"));
    let term = Emulator::new(&mut *root, Vec2::new(5, 5));
    term.assert_lines([
        "┌─┬─┐",
        "│A│B│",
        "├─┼─┤",
        "│C│D│",
        "└─┴─┘",
    ]);
}

#[test]
fn border_external_only_grows_extent() {
    let mut root = Grid::new()
        .columns([Track::grow(1)])
        .rows([Track::grow(1)])
        .border(Border::SINGLE)
        .child(0, 0, cell_text("abc"));
    let term = Emulator::new(&mut *root, Vec2::new(5, 3));
    term.assert_lines([
        "┌───┐",
        "│abc│",
        "└───┘",
    ]);
}

#[test]
fn border_junction_thick_external_light_internal_vertical() {
    let mut root = Grid::new()
        .columns([Track::fixed(1), Track::fixed(1)])
        .rows([Track::fixed(1)])
        .border(Border::THICK)
        .col_borders(Border::SINGLE)
        .child(0, 0, cell_text("A"))
        .child(0, 1, cell_text("B"));
    let term = Emulator::new(&mut *root, Vec2::new(5, 3));
    term.assert_lines([
        "┏━┯━┓",
        "┃A│B┃",
        "┗━┷━┛",
    ]);
}

#[test]
fn border_junction_thick_external_light_internal_horizontal() {
    let mut root = Grid::new()
        .columns([Track::fixed(1)])
        .rows([Track::fixed(1), Track::fixed(1)])
        .border(Border::THICK)
        .row_borders(Border::SINGLE)
        .child(0, 0, cell_text("A"))
        .child(1, 0, cell_text("B"));
    let term = Emulator::new(&mut *root, Vec2::new(3, 5));
    term.assert_lines([
        "┏━┓",
        "┃A┃",
        "┠─┨",
        "┃B┃",
        "┗━┛",
    ]);
}

#[test]
fn border_junctions_connect_with_gap() {
    let mut root = Grid::new()
        .columns([Track::fixed(1), Track::fixed(1)])
        .rows([Track::fixed(1), Track::fixed(1)])
        .col_gap(1)
        .row_gap(1)
        .border(Border::SINGLE)
        .row_borders(Border::SINGLE)
        .col_borders(Border::SINGLE)
        .child(0, 0, cell_text("A"))
        .child(0, 1, cell_text("B"))
        .child(1, 0, cell_text("C"))
        .child(1, 1, cell_text("D"));
    let term = Emulator::new(&mut *root, Vec2::new(6, 6));
    term.assert_lines([
        "┌─┬──┐",
        "│A│ B│",
        "├─┼──┤",
        "│ │  │",
        "│C│ D│",
        "└─┴──┘",
    ]);
}

#[test]
fn border_junction_thick_row_through_light_grid() {
    let mut root = Grid::new()
        .columns([Track::fixed(1), Track::fixed(1)])
        .rows([Track::fixed(1), Track::fixed(1)])
        .border(Border::SINGLE)
        .row_borders(Border::SINGLE)
        .col_borders(Border::SINGLE)
        .row_top(1, Border::THICK)
        .child(0, 0, cell_text("A"))
        .child(0, 1, cell_text("B"))
        .child(1, 0, cell_text("C"))
        .child(1, 1, cell_text("D"));
    let term = Emulator::new(&mut *root, Vec2::new(5, 5));
    term.assert_lines([
        "┌─┬─┐",
        "│A│B│",
        "┝━┿━┥",
        "│C│D│",
        "└─┴─┘",
    ]);
}

#[test]
fn track_auto_sizes_to_child_pref() {
    let mut root = Grid::new()
        .columns([Track::auto(), Track::grow(1)])
        .rows([Track::grow(1)])
        .child(0, 0, cell_text("hello"))
        .child(0, 1, cell_text("R"));
    let term = Emulator::new(&mut *root, Vec2::new(10, 1));
    term.assert_lines([
        "helloR    ",
    ]);
}

#[test]
fn track_auto_with_no_children_is_zero() {
    let mut root = Grid::new()
        .columns([Track::auto(), Track::grow(1)])
        .rows([Track::grow(1)])
        .child(0, 1, cell_text("X"));
    let term = Emulator::new(&mut *root, Vec2::new(6, 1));
    term.assert_lines([
        "X     ",
    ]);
}

#[test]
fn track_auto_with_min_floor() {
    let mut root = Grid::new()
        .columns([Track::auto().min(8), Track::grow(1)])
        .rows([Track::grow(1)])
        .child(0, 0, cell_text("hi"))
        .child(0, 1, cell_text("R"));
    let term = Emulator::new(&mut *root, Vec2::new(12, 1));
    term.assert_lines([
        "hi      R   ",
    ]);
}

#[test]
fn track_auto_with_max_clamps() {
    let mut root = Grid::new()
        .columns([Track::auto().max(3), Track::grow(1)])
        .rows([Track::grow(1)])
        .child(0, 0, cell_text("abcdef"))
        .child(0, 1, cell_text("R"));
    let term = Emulator::new(&mut *root, Vec2::new(10, 1));
    term.assert_lines([
        "abcR      ",
    ]);
}

#[test]
fn track_fixed_overrides_child() {
    let mut root = Grid::new()
        .columns([Track::fixed(2), Track::grow(1)])
        .rows([Track::grow(1)])
        .child(0, 0, cell_text("abcdef"))
        .child(0, 1, cell_text("R"));
    let term = Emulator::new(&mut *root, Vec2::new(10, 1));
    term.assert_lines([
        "abR       ",
    ]);
}

#[test]
fn track_flex_grows_from_child_pref() {
    let mut root = Grid::new()
        .columns([Track::grow(1)])
        .rows([Track::grow(1)])
        .child(0, 0, cell_text("hi"));
    let term = Emulator::new(&mut *root, Vec2::new(8, 1));
    term.assert_lines([
        "hi      ",
    ]);
}

#[test]
fn track_flex_with_min() {
    let mut root = Grid::new()
        .columns([Track::grow(1).min(5), Track::grow(1)])
        .rows([Track::grow(1)])
        .child(0, 0, cell_text("a"))
        .child(0, 1, cell_text("b"));
    let term = Emulator::new(&mut *root, Vec2::new(6, 1));
    term.assert_lines([
        "a    b",
    ]);
}

#[test]
fn track_flex_with_max_caps_growth() {
    let mut root = Grid::new()
        .columns([Track::grow(1).max(4), Track::grow(1)])
        .rows([Track::grow(1)])
        .child(0, 0, cell_text("L"))
        .child(0, 1, cell_text("R"));
    let term = Emulator::new(&mut *root, Vec2::new(12, 1));
    term.assert_lines([
        "L   R       ",
    ]);
}

#[test]
fn track_pref_overrides_child_pref() {
    let mut root = Grid::new()
        .columns([Track::auto().pref(6), Track::grow(1)])
        .rows([Track::grow(1)])
        .child(0, 0, cell_text("hi"))
        .child(0, 1, cell_text("R"));
    let term = Emulator::new(&mut *root, Vec2::new(10, 1));
    term.assert_lines([
        "hi    R   ",
    ]);
}

#[test]
fn track_two_auto_cols_take_widest_child_pref() {
    let mut root = Grid::new()
        .columns([Track::auto(), Track::auto(), Track::grow(1)])
        .rows([Track::grow(1)])
        .child(0, 0, cell_text("aa"))
        .child(0, 1, cell_text("bbbb"))
        .child(0, 2, cell_text("R"));
    let term = Emulator::new(&mut *root, Vec2::new(10, 1));
    term.assert_lines([
        "aabbbbR   ",
    ]);
}

#[test]
fn track_mixed_fixed_auto_flex() {
    let mut root = Grid::new()
        .columns([Track::fixed(3), Track::auto(), Track::grow(1)])
        .rows([Track::grow(1)])
        .child(0, 0, cell_text("XYZ"))
        .child(0, 1, cell_text("auto"))
        .child(0, 2, cell_text("F"));
    let term = Emulator::new(&mut *root, Vec2::new(12, 1));
    term.assert_lines([
        "XYZautoF    ",
    ]);
}

#[test]
fn track_measure_constraints_reports_pref() {
    let mut root = Grid::new()
        .columns([Track::auto()])
        .rows([Track::auto()])
        .child(0, 0, cell_text("hello"));
    let c = root.measure_constraints();
    assert_eq!(c.preferred_size.x, 5, "expected col pref 5, got {:?}", c.preferred_size);
    assert_eq!(c.preferred_size.y, 1, "expected row pref 1, got {:?}", c.preferred_size);
}

#[test]
fn track_auto_row_sizes_to_child_pref() {
    let mut root = Grid::new()
        .columns([Track::grow(1)])
        .rows([Track::auto(), Track::grow(1)])
        .child(0, 0, cell_text("hi"))
        .child(1, 0, cell_text("bot"));
    let term = Emulator::new(&mut *root, Vec2::new(3, 4));
    term.assert_lines([
        "hi ",
        "bot",
        "   ",
        "   ",
    ]);
}

#[test]
fn track_auto_row_with_min_floor() {
    let mut root = Grid::new()
        .columns([Track::grow(1)])
        .rows([Track::auto().min(3), Track::grow(1)])
        .child(0, 0, cell_text("hi"))
        .child(1, 0, cell_text("b"));
    let term = Emulator::new(&mut *root, Vec2::new(3, 5));
    term.assert_lines([
        "hi ",
        "   ",
        "   ",
        "b  ",
        "   ",
    ]);
}

#[test]
fn track_fixed_row_overrides_child() {
    let mut root = Grid::new()
        .columns([Track::grow(1)])
        .rows([Track::fixed(1), Track::grow(1)])
        .child(0, 0, cell_text("hi"))
        .child(1, 0, cell_text("b"));
    let term = Emulator::new(&mut *root, Vec2::new(3, 4));
    term.assert_lines([
        "hi ",
        "b  ",
        "   ",
        "   ",
    ]);
}

#[test]
fn cell_padding_default_with_per_cell_override() {
    let mut root = Grid::new()
        .columns([Track::fixed(3), Track::fixed(3)])
        .rows([Track::fixed(3), Track::fixed(3)])
        .cell(Cell::new(0, 0, cell_text("A")).padding(Spacing::new()))
        .child(0, 1, cell_text("B"))
        .child(1, 0, cell_text("C"))
        .child(1, 1, cell_text("D"))
        .padding_left(1)
        .padding_top(1);
    let term = Emulator::new(&mut *root, Vec2::new(8, 8));
    term.assert_lines([
        "A       ",
        "     B  ",
        "        ",
        "        ",
        "        ",
        " C   D  ",
        "        ",
        "        ",
    ]);
}

#[test]
fn row_style_bordered_paints_gaps_and_external_v_borders() {
    let r_bg = Color::Indexed(2);
    let c_bg = Color::Indexed(4);
    let mut root = Grid::new()
        .columns([Track::fixed(2), Track::fixed(2)])
        .rows([Track::fixed(1), Track::fixed(1)])
        .col_gap(1)
        .row_gap(1)
        .border(Border::SINGLE)
        .row_borders(Border::SINGLE)
        .col_borders(Border::SINGLE)
        .child(0, 0, cell_text("AA"))
        .child(0, 1, cell_text("BB"))
        .child(1, 0, cell_text("CC"))
        .child(1, 1, cell_text("DD"))
        .row_style(1, Style::new().bg(r_bg))
        .col_style(1, Style::new().bg(c_bg));
    let term = Emulator::new(&mut *root, Vec2::new(8, 6));
    let bg = bg_grid(&term.get_snapshot());
    let r = Some(r_bg);
    let c = Some(c_bg);
    let n = None;
    let expected = vec![
        vec![n, n, n, n, n, n, n, n],
        vec![n, n, n, n, n, c, c, n],
        vec![n, n, n, n, n, c, c, n],
        vec![n, n, n, n, n, c, c, n],
        vec![n, r, r, r, r, r, r, n],
        vec![n, n, n, n, n, n, n, n],
    ];
    assert_eq!(bg, expected, "snapshot text:\n{}", term.get_snapshot_text());
}

#[test]
fn cell_style_overrides_combined_row_col_fill() {
    let r_bg = Color::Indexed(2);
    let c_bg = Color::Indexed(4);
    let cell_bg = Color::Indexed(5);
    let mut root = Grid::new()
        .columns([Track::fixed(2), Track::fixed(2)])
        .rows([Track::fixed(1), Track::fixed(1)])
        .cell(Cell::new(0, 0, cell_text("AA")).style(Style::new().bg(cell_bg)))
        .child(0, 1, cell_text("BB"))
        .child(1, 0, cell_text("CC"))
        .child(1, 1, cell_text("DD"))
        .row_style(0, Style::new().bg(r_bg))
        .col_style(0, Style::new().bg(c_bg));
    let term = Emulator::new(&mut *root, Vec2::new(4, 2));
    let bg = bg_grid(&term.get_snapshot());
    let r = Some(r_bg);
    let c = Some(c_bg);
    let x = Some(cell_bg);
    let expected = vec![
        vec![x, x, r, r],
        vec![c, c, None, None],
    ];
    assert_eq!(bg, expected, "snapshot text:\n{}", term.get_snapshot_text());
}

#[test]
fn per_track_border_style_colors_inner_glyphs() {
    let r_fg = Color::Indexed(1);
    let c_fg = Color::Indexed(3);
    let mut root = Grid::new()
        .columns([Track::fixed(1), Track::fixed(1)])
        .rows([Track::fixed(1), Track::fixed(1)])
        .border(Border::SINGLE)
        .row_borders(Border::SINGLE)
        .col_borders(Border::SINGLE)
        .child(0, 0, cell_text("A"))
        .child(0, 1, cell_text("B"))
        .child(1, 0, cell_text("C"))
        .child(1, 1, cell_text("D"))
        .row_border_style(1, Style::new().fg(r_fg))
        .col_border_style(1, Style::new().fg(c_fg));
    let term = Emulator::new(&mut *root, Vec2::new(5, 5));
    let fg = fg_grid(&term.get_snapshot());
    let r = Some(r_fg);
    let c = Some(c_fg);
    let n = None;
    let expected = vec![
        vec![n, n, n, c, n],
        vec![n, n, c, n, c],
        vec![n, r, n, r, n],
        vec![r, n, c, n, c],
        vec![n, r, n, r, n],
    ];
    assert_eq!(fg, expected, "snapshot text:\n{}", term.get_snapshot_text());
}

#[test]
fn col_span_footer_no_row_borders_bug() {
    let mut root = Grid::new()
        .columns([Track::fixed(1), Track::fixed(1), Track::fixed(1)])
        .rows([Track::fixed(1), Track::fixed(1)])
        .border(Border::SINGLE)
        .col_borders(Border::SINGLE)
        .child(0, 0, cell_text("A"))
        .child(0, 1, cell_text("B"))
        .child(0, 2, cell_text("C"))
        .cell(Cell::new(1, 0, cell_text("X")).span(1, 2))
        .child(1, 2, cell_text("Z"));
    let term = Emulator::new(&mut *root, Vec2::new(7, 4));
    term.assert_lines([
        "┌─┬─┬─┐",
        "│A│B│C│",
        "│X  │Z│",
        "└───┴─┘",
    ]);
}

#[test]
fn col_span_footer_with_row_borders_bug() {
    let mut root = Grid::new()
        .columns([Track::fixed(1), Track::fixed(1), Track::fixed(1)])
        .rows([Track::fixed(1), Track::fixed(1)])
        .border(Border::SINGLE)
        .col_borders(Border::SINGLE)
        .row_borders(Border::SINGLE)
        .child(0, 0, cell_text("A"))
        .child(0, 1, cell_text("B"))
        .child(0, 2, cell_text("C"))
        .cell(Cell::new(1, 0, cell_text("X")).span(1, 2))
        .child(1, 2, cell_text("Z"));
    let term = Emulator::new(&mut *root, Vec2::new(7, 5));
    term.assert_lines([
        "┌─┬─┬─┐",
        "│A│B│C│",
        "├─┴─┼─┤",
        "│X  │Z│",
        "└───┴─┘",
    ]);
}

