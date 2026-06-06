use chord_macro::chord;
use tuie::prelude::*;
use tuie::emulator::Emulator;

fn style_with_thumb(thumb: ScrollbarThumb) -> ScrollbarStyle {
    ScrollbarStyle {
        thumb: Some(thumb),
        ..ScrollbarStyle::new()
    }
}

fn vert(thumb: ScrollbarThumb, rows: u16) -> Box<Pane> {
    let mut pane = Pane::new()
        .vertical()
        .y_scroll(Scrollbar::Visible)
        .scrollbar_style(style_with_thumb(thumb));
    for i in 0..rows {
        pane.add_child(Text::new().content((i % 10).to_string()));
    }
    pane
}

fn horiz(thumb: ScrollbarThumb, cols: u16) -> Box<Pane> {
    let mut pane = Pane::new()
        .horizontal()
        .x_scroll(Scrollbar::Visible)
        .scrollbar_style(style_with_thumb(thumb));
    for i in 0..cols {
        pane.add_child(Text::new().content((i % 10).to_string()));
    }
    pane
}

fn both(thumb: ScrollbarThumb) -> Box<Pane> {
    Pane::new()
        .vertical()
        .y_scroll(Scrollbar::Visible)
        .x_scroll(Scrollbar::Visible)
        .scrollbar_style(style_with_thumb(thumb))
        .children([Text::new().content("0123456789").width(10).height(10)])
}

fn click(pos: Vec2<i32>) -> RuntimeEvent {
    RuntimeEvent::input_at(chord!(LeftClick), pos)
}
fn drag(pos: Vec2<i32>) -> RuntimeEvent {
    RuntimeEvent::input_at(chord!(LeftDrag), pos)
}
fn release(pos: Vec2<i32>) -> RuntimeEvent {
    RuntimeEvent::input_at(chord!(LeftRelease), pos)
}

#[test]
fn vertical_thumb_quarter_viewport_one_cell() {
    let mut pane = vert(ScrollbarThumb::SINGLE, 16);
    let term = Emulator::new(&mut *pane, Vec2::new(2, 4));
    term.assert_lines([
        "0│",
        "1 ",
        "2 ",
        "3 ",
    ]);
}

#[test]
fn vertical_single_thumb_glyphs_at_top() {
    let mut pane = vert(ScrollbarThumb::SINGLE, 8);
    let term = Emulator::new(&mut *pane, Vec2::new(2, 4));
    term.assert_lines([
        "0│",
        "1│",
        "2 ",
        "3 ",
    ]);
}

#[test]
fn vertical_thick_thumb_glyphs_at_top() {
    let mut pane = vert(ScrollbarThumb::THICK, 8);
    let term = Emulator::new(&mut *pane, Vec2::new(2, 4));
    term.assert_lines([
        "0┃",
        "1┃",
        "2 ",
        "3 ",
    ]);
}

#[test]
fn vertical_double_thumb_glyphs_at_top() {
    let mut pane = vert(ScrollbarThumb::DOUBLE, 8);
    let term = Emulator::new(&mut *pane, Vec2::new(2, 4));
    term.assert_lines([
        "0║",
        "1║",
        "2 ",
        "3 ",
    ]);
}

#[test]
fn vertical_ascii_thumb_glyphs_at_top() {
    let mut pane = vert(ScrollbarThumb::ASCII, 8);
    let term = Emulator::new(&mut *pane, Vec2::new(2, 4));
    term.assert_lines([
        "0|",
        "1|",
        "2 ",
        "3 ",
    ]);
}

#[test]
fn vertical_dashed_thumb_glyphs_at_top() {
    let mut pane = vert(ScrollbarThumb::DASHED, 8);
    let term = Emulator::new(&mut *pane, Vec2::new(2, 4));
    term.assert_lines([
        "0┊",
        "1┊",
        "2 ",
        "3 ",
    ]);
}

#[test]
fn vertical_single_thumb_half_cell_stubs_at_quarter_progress() {
    let mut pane = vert(ScrollbarThumb::SINGLE, 8);
    let mut term = Emulator::new(&mut *pane, Vec2::new(2, 4));
    term.update(&mut *pane, &[click(Vec2::new(1, 3)), drag(Vec2::new(1, 1)), release(Vec2::new(1, 1))]);
    term.assert_lines([
        "1╷",
        "2│",
        "3╵",
        "4 ",
    ]);
}

#[test]
fn vertical_thick_thumb_half_cell_stubs_at_quarter_progress() {
    let mut pane = vert(ScrollbarThumb::THICK, 8);
    let mut term = Emulator::new(&mut *pane, Vec2::new(2, 4));
    term.update(&mut *pane, &[click(Vec2::new(1, 3)), drag(Vec2::new(1, 1)), release(Vec2::new(1, 1))]);
    term.assert_lines([
        "1╻",
        "2┃",
        "3╹",
        "4 ",
    ]);
}

#[test]
fn vertical_block_thumb_partial_cells_at_quarter_progress() {
    let mut pane = vert(ScrollbarThumb::BLOCK, 8);
    let mut term = Emulator::new(&mut *pane, Vec2::new(2, 4));
    term.update(&mut *pane, &[click(Vec2::new(1, 3)), drag(Vec2::new(1, 1)), release(Vec2::new(1, 1))]);
    term.assert_lines([
        "1▄",
        "2 ",
        "3▄",
        "4 ",
    ]);
}

#[test]
fn vertical_double_thumb_no_half_cell_at_quarter_progress() {
    let mut pane = vert(ScrollbarThumb::DOUBLE, 8);
    let mut term = Emulator::new(&mut *pane, Vec2::new(2, 4));
    term.update(&mut *pane, &[click(Vec2::new(1, 3)), drag(Vec2::new(1, 1)), release(Vec2::new(1, 1))]);
    term.assert_lines([
        "1 ",
        "2║",
        "3║",
        "4 ",
    ]);
}

#[test]
fn vertical_ascii_thumb_no_half_cell_at_quarter_progress() {
    let mut pane = vert(ScrollbarThumb::ASCII, 8);
    let mut term = Emulator::new(&mut *pane, Vec2::new(2, 4));
    term.update(&mut *pane, &[click(Vec2::new(1, 3)), drag(Vec2::new(1, 1)), release(Vec2::new(1, 1))]);
    term.assert_lines([
        "1 ",
        "2|",
        "3|",
        "4 ",
    ]);
}

#[test]
fn horizontal_single_thumb_half_viewport_two_cells() {
    let mut pane = horiz(ScrollbarThumb::SINGLE, 8);
    let term = Emulator::new(&mut *pane, Vec2::new(4, 2));
    term.assert_lines([
        "0123",
        "──  ",
    ]);
}

#[test]
fn horizontal_thick_thumb_glyphs_at_left() {
    let mut pane = horiz(ScrollbarThumb::THICK, 8);
    let term = Emulator::new(&mut *pane, Vec2::new(4, 2));
    term.assert_lines([
        "0123",
        "━━  ",
    ]);
}

#[test]
fn horizontal_double_thumb_glyphs_at_left() {
    let mut pane = horiz(ScrollbarThumb::DOUBLE, 8);
    let term = Emulator::new(&mut *pane, Vec2::new(4, 2));
    term.assert_lines([
        "0123",
        "══  ",
    ]);
}

#[test]
fn horizontal_block_thumb_glyphs_at_left() {
    let mut pane = horiz(ScrollbarThumb::BLOCK, 8);
    let term = Emulator::new(&mut *pane, Vec2::new(4, 2));
    term.assert_lines([
        "0123",
        "▄▄  ",
    ]);
}

#[test]
fn horizontal_single_thumb_half_cell_stubs_at_quarter_progress() {
    let mut pane = horiz(ScrollbarThumb::SINGLE, 8);
    let mut term = Emulator::new(&mut *pane, Vec2::new(4, 2));
    term.update(&mut *pane, &[click(Vec2::new(3, 1)), drag(Vec2::new(1, 1)), release(Vec2::new(1, 1))]);
    term.assert_lines([
        "1234",
        "╶─╴ ",
    ]);
}

#[test]
fn horizontal_thick_thumb_half_cell_stubs_at_quarter_progress() {
    let mut pane = horiz(ScrollbarThumb::THICK, 8);
    let mut term = Emulator::new(&mut *pane, Vec2::new(4, 2));
    term.update(&mut *pane, &[click(Vec2::new(3, 1)), drag(Vec2::new(1, 1)), release(Vec2::new(1, 1))]);
    term.assert_lines([
        "1234",
        "╺━╸ ",
    ]);
}

#[test]
fn horizontal_double_thumb_no_half_cell_at_quarter_progress() {
    let mut pane = horiz(ScrollbarThumb::DOUBLE, 8);
    let mut term = Emulator::new(&mut *pane, Vec2::new(4, 2));
    term.update(&mut *pane, &[click(Vec2::new(3, 1)), drag(Vec2::new(1, 1)), release(Vec2::new(1, 1))]);
    term.assert_lines([
        "1234",
        " ══ ",
    ]);
}

#[test]
fn both_scrollbars_single_corner_merges_at_end() {
    let mut pane = both(ScrollbarThumb::SINGLE);
    let mut term = Emulator::new(&mut *pane, Vec2::new(5, 5));
    term.update(&mut *pane, &[click(Vec2::new(4, 3)), release(Vec2::new(4, 3))]);
    term.update(&mut *pane, &[click(Vec2::new(3, 4)), release(Vec2::new(3, 4))]);
    let snap = term.get_snapshot_text();
    let last_row = snap.split('\n').last().unwrap();
    let corner = last_row.chars().last().unwrap();
    assert_eq!(corner, '┘', "expected SINGLE corner glyph at bottom-right, full snap:\n{snap}");
}

#[test]
fn both_scrollbars_thick_corner_merges_at_end() {
    let mut pane = both(ScrollbarThumb::THICK);
    let mut term = Emulator::new(&mut *pane, Vec2::new(5, 5));
    term.update(&mut *pane, &[click(Vec2::new(4, 3)), release(Vec2::new(4, 3))]);
    term.update(&mut *pane, &[click(Vec2::new(3, 4)), release(Vec2::new(3, 4))]);
    let snap = term.get_snapshot_text();
    let corner = snap.split('\n').last().unwrap().chars().last().unwrap();
    assert_eq!(corner, '┛', "expected THICK corner glyph at bottom-right, full snap:\n{snap}");
}

#[test]
fn both_scrollbars_double_does_not_share_corner() {
    let mut pane = both(ScrollbarThumb::DOUBLE);
    let mut term = Emulator::new(&mut *pane, Vec2::new(5, 5));
    term.update(&mut *pane, &[click(Vec2::new(4, 3)), release(Vec2::new(4, 3))]);
    term.update(&mut *pane, &[click(Vec2::new(3, 4)), release(Vec2::new(3, 4))]);
    let snap = term.get_snapshot_text();
    let corner = snap.split('\n').last().unwrap().chars().last().unwrap();
    assert_eq!(corner, ' ', "DOUBLE has no stubs and must not paint a corner glyph, full snap:\n{snap}");
}

#[test]
fn both_scrollbars_block_does_not_merge_at_corner() {
    let mut pane = both(ScrollbarThumb::BLOCK);
    let mut term = Emulator::new(&mut *pane, Vec2::new(5, 5));
    term.update(&mut *pane, &[click(Vec2::new(4, 3)), release(Vec2::new(4, 3))]);
    term.update(&mut *pane, &[click(Vec2::new(3, 4)), release(Vec2::new(3, 4))]);
    let snap = term.get_snapshot_text();
    let corner = snap.split('\n').last().unwrap().chars().last().unwrap();
    assert!(
        !matches!(corner, '┘' | '┛' | '╝' | '+'),
        "BLOCK must not paint a border junction at the corner, got {:?}\nfull snap:\n{snap}",
        corner,
    );
}

#[test]
fn vertical_drag_thumb_changes_scroll_progress() {
    let mut pane = vert(ScrollbarThumb::SINGLE, 8);
    let mut term = Emulator::new(&mut *pane, Vec2::new(2, 4));
    assert_eq!(pane.get_scroll_progress(Axis2D::Y), 0.0);
    term.update(&mut *pane, &[
        click(Vec2::new(1, 0)),
        drag(Vec2::new(1, 2)),
        release(Vec2::new(1, 2)),
    ]);
    assert_eq!(pane.get_scroll_progress(Axis2D::Y), 1.0);
}

#[test]
fn vertical_click_track_jumps_thumb() {
    let mut pane = vert(ScrollbarThumb::SINGLE, 8);
    let mut term = Emulator::new(&mut *pane, Vec2::new(2, 4));
    assert_eq!(pane.get_scroll_progress(Axis2D::Y), 0.0);
    term.update(&mut *pane, &[click(Vec2::new(1, 3)), release(Vec2::new(1, 3))]);
    assert_eq!(pane.get_scroll_progress(Axis2D::Y), 1.0);
}

#[test]
fn horizontal_drag_thumb_changes_scroll_progress() {
    let mut pane = horiz(ScrollbarThumb::SINGLE, 8);
    let mut term = Emulator::new(&mut *pane, Vec2::new(4, 2));
    assert_eq!(pane.get_scroll_progress(Axis2D::X), 0.0);
    term.update(&mut *pane, &[
        click(Vec2::new(0, 1)),
        drag(Vec2::new(2, 1)),
        release(Vec2::new(2, 1)),
    ]);
    assert_eq!(pane.get_scroll_progress(Axis2D::X), 1.0);
}

#[test]
fn horizontal_click_track_jumps_thumb() {
    let mut pane = horiz(ScrollbarThumb::SINGLE, 8);
    let mut term = Emulator::new(&mut *pane, Vec2::new(4, 2));
    assert_eq!(pane.get_scroll_progress(Axis2D::X), 0.0);
    term.update(&mut *pane, &[click(Vec2::new(3, 1)), release(Vec2::new(3, 1))]);
    assert_eq!(pane.get_scroll_progress(Axis2D::X), 1.0);
}

fn subcell_event(chord: Chord, pos: Vec2<f32>) -> RuntimeEvent {
    RuntimeEvent::Input(InputEvent { chord, pos, count: 1 })
}

fn enable_subcell_events(term: &mut Emulator) {
    term.update_terminal_info(|info| {
        info.cell_size = Some(Vec2::new(10, 20));
        info.subcell_events = true;
    });
}

fn fractional_drag(pane: &mut Pane, term: &mut Emulator) {
    term.update(pane, &[
        subcell_event(chord!(LeftClick), Vec2::new(1.5, 0.5)),
        subcell_event(chord!(LeftDrag), Vec2::new(1.5, 1.125)),
        release(Vec2::new(1, 1)),
    ]);
}

#[test]
fn tty_subcell_drag_still_renders_content() {
    let mut pane = vert(ScrollbarThumb::SINGLE, 8);
    let mut term = Emulator::new(&mut *pane, Vec2::new(2, 4));
    enable_subcell_events(&mut term);
    fractional_drag(&mut pane, &mut term);
    term.assert_lines([
        "1╷",
        "2│",
        "3╵",
        "4 ",
    ]);
}

#[test]
fn tty_subcell_drag_round_trips_progress() {
    let mut pane = vert(ScrollbarThumb::SINGLE, 8);
    let mut term = Emulator::new(&mut *pane, Vec2::new(2, 4));
    enable_subcell_events(&mut term);
    fractional_drag(&mut pane, &mut term);
    assert_eq!(pane.get_scroll_progress(Axis2D::Y), 0.3125);
}

#[test]
fn tty_subcell_drag_hit_test_matches_drawn_cells() {
    let mut pane = Pane::new()
        .vertical()
        .y_scroll(Scrollbar::Visible)
        .scrollbar_style(style_with_thumb(ScrollbarThumb::SINGLE));
    let mut ids = Vec::new();
    for i in 0..8 {
        let text = Text::new().content(i.to_string());
        ids.push(text.get_id());
        pane.add_child(text);
    }
    let mut term = Emulator::new(&mut *pane, Vec2::new(2, 4));
    enable_subcell_events(&mut term);
    fractional_drag(&mut pane, &mut term);
    let hit = pane.descendant_at_pos(Vec2::new(0.5, 0.9), None);
    assert_eq!(hit, Some(ids[1].untyped()));
}
