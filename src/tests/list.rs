use chord_macro::chord;
use tuie::prelude::*;
use tuie::emulator::Emulator;

fn make_list(len: usize) -> Box<List> {
    let mut list = List::new();
    list.set_renderer((), |_: &mut (), idx: usize| -> Option<Box<dyn Widget>> {
        Some(Text::new().content(format!("item {}", idx)) as Box<dyn Widget>)
    });
    list.set_item_count(len);
    list
}

#[test]
fn renders_empty_list() {
    let mut list = make_list(0);
    let term = Emulator::new(&mut *list, Vec2::new(8, 3));
    term.assert_lines([
        "        ",
        "        ",
        "        ",
    ]);
}

#[test]
fn renders_items_top_to_bottom() {
    let mut list = make_list(5);
    let term = Emulator::new(&mut *list, Vec2::new(8, 3));
    term.assert_lines([
        "item 0  ",
        "item 1  ",
        "item 2  ",
    ]);
}

#[test]
fn scroll_method_advances_view() {
    let mut list = make_list(20);
    let mut term = Emulator::new(&mut *list, Vec2::new(8, 3));
    term.assert_lines([
        "item 0  ",
        "item 1  ",
        "item 2  ",
    ]);
    list.scroll_by(2);
    term.update(&mut *list, &[RuntimeEvent::Resize(Vec2::new(8, 3))]);
    term.assert_lines([
        "item 2  ",
        "item 3  ",
        "item 4  ",
    ]);
}

#[test]
fn mouse_scroll_down_advances_view() {
    let mut list = make_list(20);
    let mut term = Emulator::new(&mut *list, Vec2::new(8, 3));
    for _ in 0..3 {
        term.update(
            &mut *list,
            &[RuntimeEvent::input_at(
                chord!(MouseScroll(Direction2D::Down)),
                Vec2::new(2, 1),
            )],
        );
    }
    term.assert_lines([
        "item 3  ",
        "item 4  ",
        "item 5  ",
    ]);
}

#[test]
fn ensure_visible_scrolls_far_index_into_view() {
    let mut list = make_list(50);
    let mut term = Emulator::new(&mut *list, Vec2::new(8, 4));
    list.ensure_visible(30);
    term.update(&mut *list, &[RuntimeEvent::Resize(Vec2::new(8, 4))]);
    let visible = list.get_visible_range();
    assert!(visible.contains(&30), "expected 30 in visible range, got {:?}", visible);
    let text = term.get_snapshot_text();
    assert!(text.contains("item 30"), "snapshot missing 'item 30':\n{}", text);
}

#[test]
fn set_scroll_progress_jumps_to_end() {
    let mut list = make_list(100);
    let mut term = Emulator::new(&mut *list, Vec2::new(8, 3));
    list.set_scroll_progress(Axis2D::Y, 1.0);
    term.update(&mut *list, &[RuntimeEvent::Resize(Vec2::new(8, 3))]);
    let visible = list.get_visible_range();
    assert!(visible.end == 100, "expected end at 100, got {:?}", visible);
    term.assert_lines([
        "item 97 ",
        "item 98 ",
        "item 99 ",
    ]);
}

#[test]
fn resize_grows_visible_range() {
    let mut list = make_list(10);
    let mut term = Emulator::new(&mut *list, Vec2::new(8, 2));
    term.assert_lines([
        "item 0  ",
        "item 1  ",
    ]);
    term.update(&mut *list, &[RuntimeEvent::Resize(Vec2::new(8, 5))]);
    term.assert_lines([
        "item 0  ",
        "item 1  ",
        "item 2  ",
        "item 3  ",
        "item 4  ",
    ]);
}

#[test]
fn set_len_shrink_clamps_anchor() {
    let mut list = make_list(50);
    let mut term = Emulator::new(&mut *list, Vec2::new(8, 3));
    list.set_scroll_progress(Axis2D::Y, 1.0);
    term.update(&mut *list, &[RuntimeEvent::Resize(Vec2::new(8, 3))]);
    list.set_item_count(4);
    term.update(&mut *list, &[RuntimeEvent::Resize(Vec2::new(8, 3))]);
    assert_eq!(list.get_item_count(), 4);
    term.assert_lines([
        "item 1  ",
        "item 2  ",
        "item 3  ",
    ]);
}

#[test]
fn horizontal_orientation_lays_items_across() {
    let mut list = List::new().horizontal();
    list.set_renderer((), |_: &mut (), idx: usize| -> Option<Box<dyn Widget>> {
        Some(Text::new().content(idx.to_string()).min_width(2) as Box<dyn Widget>)
    });
    list.set_item_count(6);
    let term = Emulator::new(&mut *list, Vec2::new(12, 1));
    term.assert_lines([
        "0 1 2 3 4 5 ",
    ]);
}

#[test]
fn gap_inserts_blank_rows_between_items() {
    let mut list = make_list(3).gap(1);
    let term = Emulator::new(&mut *list, Vec2::new(8, 5));
    term.assert_lines([
        "item 0  ",
        "        ",
        "item 1  ",
        "        ",
        "item 2  ",
    ]);
}

#[test]
fn multiline_items_stack_vertically() {
    let mut list = List::new();
    list.set_renderer((), |_: &mut (), idx: usize| -> Option<Box<dyn Widget>> {
        Some(Text::new().content(format!("a{}\nb{}", idx, idx)) as Box<dyn Widget>)
    });
    list.set_item_count(3);
    let term = Emulator::new(&mut *list, Vec2::new(4, 6));
    term.assert_lines([
        "a0  ",
        "b0  ",
        "a1  ",
        "b1  ",
        "a2  ",
        "b2  ",
    ]);
}

#[test]
fn multiline_item_clips_at_viewport_end() {
    let mut list = List::new();
    list.set_renderer((), |_: &mut (), idx: usize| -> Option<Box<dyn Widget>> {
        Some(Text::new().content(format!("a{}\nb{}", idx, idx)) as Box<dyn Widget>)
    });
    list.set_item_count(3);
    let term = Emulator::new(&mut *list, Vec2::new(4, 5));
    term.assert_lines([
        "a0  ",
        "b0  ",
        "a1  ",
        "b1  ",
        "a2  ",
    ]);
}

#[test]
fn mixed_height_items_pack_tightly() {
    let mut list = List::new();
    list.set_renderer((), |_: &mut (), idx: usize| -> Option<Box<dyn Widget>> {
        let content = if idx == 1 {
            format!("a{}\nb{}", idx, idx)
        } else {
            format!("a{}", idx)
        };
        Some(Text::new().content(content) as Box<dyn Widget>)
    });
    list.set_item_count(3);
    let term = Emulator::new(&mut *list, Vec2::new(4, 4));
    term.assert_lines([
        "a0  ",
        "a1  ",
        "b1  ",
        "a2  ",
    ]);
}

#[test]
fn scroll_past_multiline_first_row() {
    let mut list = List::new();
    list.set_renderer((), |_: &mut (), idx: usize| -> Option<Box<dyn Widget>> {
        Some(Text::new().content(format!("a{}\nb{}", idx, idx)) as Box<dyn Widget>)
    });
    list.set_item_count(3);
    let mut term = Emulator::new(&mut *list, Vec2::new(4, 4));
    list.scroll_by(1);
    term.update(&mut *list, &[RuntimeEvent::Resize(Vec2::new(4, 4))]);
    term.assert_lines([
        "b0  ",
        "a1  ",
        "b1  ",
        "a2  ",
    ]);
}

#[test]
fn invalidate_all_rerenders_items_with_new_context() {
    let mut list = List::new();
    list.set_renderer(
        String::from("a"),
        |ctx: &mut String, idx: usize| -> Option<Box<dyn Widget>> {
            Some(Text::new().content(format!("{}{}", ctx, idx)) as Box<dyn Widget>)
        },
    );
    list.set_item_count(2);
    let mut term = Emulator::new(&mut *list, Vec2::new(4, 2));
    term.assert_lines([
        "a0  ",
        "a1  ",
    ]);
    *list.get_context_mut::<String>().unwrap() = String::from("z");
    list.invalidate_all();
    term.update(&mut *list, &[RuntimeEvent::Resize(Vec2::new(4, 2))]);
    term.assert_lines([
        "z0  ",
        "z1  ",
    ]);
}

#[test]
fn pending_page_blocks_scroll_until_loaded() {
    let mut list = List::new();
    list.set_renderer(
        false,
        |loading: &mut bool, idx: usize| -> Option<Box<dyn Widget>> {
            if idx >= 3 && *loading {
                return None;
            }
            Some(Text::new().content(format!("item {}", idx)) as Box<dyn Widget>)
        },
    );
    list.set_item_count(20);
    let mut term = Emulator::new(&mut *list, Vec2::new(8, 3));
    term.assert_lines([
        "item 0  ",
        "item 1  ",
        "item 2  ",
    ]);

    *list.get_context_mut::<bool>().unwrap() = true;
    list.set_scroll_progress(Axis2D::Y, 1.0);
    term.update(&mut *list, &[]);
    term.assert_lines([
        "item 0  ",
        "item 1  ",
        "item 2  ",
    ]);

    *list.get_context_mut::<bool>().unwrap() = false;
    list.invalidate_all();
    term.update(&mut *list, &[]);
    term.assert_lines([
        "item 17 ",
        "item 18 ",
        "item 19 ",
    ]);
}
