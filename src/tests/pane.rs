use tuie::prelude::*;
use tuie::emulator::Emulator;

#[test]
fn empty_pane_renders_blank() {
    let mut root = Pane::new();
    let term = Emulator::new(&mut *root, Vec2::new(4, 2));
    term.assert_lines([
        "    ",
        "    ",
    ]);
}

#[test]
fn single_child_renders() {
    let mut root = Pane::new().children([
        Text::new().content("hi"),
    ]);
    let term = Emulator::new(&mut *root, Vec2::new(4, 2));
    term.assert_lines([
        "hi  ",
        "    ",
    ]);
}

#[test]
fn vertical_stacks_children_top_to_bottom() {
    let mut root = Pane::new().vertical().children([
        Text::new().content("aa"),
        Text::new().content("bb"),
        Text::new().content("cc"),
    ]);
    let term = Emulator::new(&mut *root, Vec2::new(3, 4));
    term.assert_lines([
        "aa ",
        "bb ",
        "cc ",
        "   ",
    ]);
}

#[test]
fn horizontal_lays_children_left_to_right() {
    let mut root = Pane::new().horizontal().children([
        Text::new().content("ab"),
        Text::new().content("cd"),
    ]);
    let term = Emulator::new(&mut *root, Vec2::new(6, 1));
    term.assert_lines([
        "abcd  ",
    ]);
}

#[test]
fn border_renders_around_content() {
    let mut root = Pane::new()
        .bordered()
        .children([Text::new().content("hey")]);
    let term = Emulator::new(&mut *root, Vec2::new(5, 3));
    term.assert_lines([
        "┌───┐",
        "│hey│",
        "└───┘",
    ]);
}

#[test]
fn border_with_title_renders_in_top_edge() {
    let mut root = Pane::new()
        .bordered()
        .title("T")
        .children([Text::new().content("ab")]);
    let term = Emulator::new(&mut *root, Vec2::new(6, 3));
    let snap = term.get_snapshot_text();
    assert!(snap.contains('T'), "expected title 'T' in snapshot, got:\n{snap}");
    assert!(snap.starts_with('┌'), "expected top-left border corner, got:\n{snap}");
    assert!(snap.contains("ab"), "expected content 'ab' in snapshot, got:\n{snap}");
}

#[test]
fn padding_inserts_space_around_content() {
    let mut root = Pane::new()
        .padding(Spacing::balanced(1))
        .children([Text::new().content("x")]);
    let term = Emulator::new(&mut *root, Vec2::new(3, 3));
    term.assert_lines([
        " x ",
        "   ",
        "   ",
    ]);
}

#[test]
fn horizontal_gap_inserts_space_between_children() {
    let mut root = Pane::new().horizontal().gap(2).children([
        Text::new().content("a"),
        Text::new().content("b"),
    ]);
    let term = Emulator::new(&mut *root, Vec2::new(5, 1));
    term.assert_lines([
        "a  b ",
    ]);
}

#[test]
fn align_middle_centers_children_on_main_axis() {
    let mut root = Pane::new()
        .horizontal()
        .x_place(Place::Middle)
        .children([Text::new().content("ab")]);
    let term = Emulator::new(&mut *root, Vec2::new(6, 1));
    term.assert_lines([
        "  ab  ",
    ]);
}

#[test]
fn align_end_pushes_children_to_end_of_main_axis() {
    let mut root = Pane::new()
        .horizontal()
        .x_place(Place::End)
        .children([Text::new().content("xy")]);
    let term = Emulator::new(&mut *root, Vec2::new(6, 1));
    term.assert_lines([
        "    xy",
    ]);
}

#[test]
fn fit_items_middle_centers_child_on_cross_axis() {
    let mut root = Pane::new()
        .vertical()
        .x_place(Place::Middle)
        .children([Text::new().content("z")]);
    let term = Emulator::new(&mut *root, Vec2::new(5, 2));
    term.assert_lines([
        "  z  ",
        "     ",
    ]);
}

#[test]
fn resize_reflows_children() {
    let mut root = Pane::new().horizontal().children([
        Text::new().content("ab"),
        Text::new().content("cd"),
    ]);
    let mut term = Emulator::new(&mut *root, Vec2::new(6, 1));
    term.assert_lines(["abcd  "]);
    term.update(&mut *root, &[RuntimeEvent::Resize(Vec2::new(4, 2))]);
    term.assert_lines([
        "abcd",
        "    ",
    ]);
}

#[test]
fn nested_panes_compose() {
    let inner = Pane::new()
        .bordered()
        .children([Text::new().content("hi")]);
    let mut root = Pane::new()
        .padding(Spacing::balanced(1))
        .children([inner]);
    let term = Emulator::new(&mut *root, Vec2::new(6, 5));
    term.assert_lines([
        " ┌──┐ ",
        " │hi│ ",
        " └──┘ ",
        "      ",
        "      ",
    ]);
}

#[test]
fn shrink_wrap_pane_sizes_correctly_around_word_wrapped_text_and_fixed_height_child() {
    let inner = Pane::new().vertical().height(3).bordered();
    let section = Pane::new()
        .vertical()
        .children([
            Text::new().content("a b c d").word_wrap().margin_bottom(1) as Box<dyn Widget>,
            inner,
        ]);
    let outer = Pane::new().vertical().bordered().child(section);
    let wrapper = Pane::new().vertical().bordered().height(11).child(outer);
    let mut root = Pane::new().vertical().child(wrapper);
    let term = Emulator::new(&mut *root, Vec2::new(7, 11));
    term.assert_lines([
        "┌─────┐",
        "│┌───┐│",
        "││a b││",
        "││c d││",
        "││   ││",
        "││┌─┐││",
        "│││ │││",
        "││└─┘││",
        "│└───┘│",
        "│     │",
        "└─────┘",
    ]);
}

fn chip(label: &'static str) -> Box<dyn Widget> {
    Text::new().content(label) as Box<dyn Widget>
}

fn wrap() -> Box<Pane> {
    Pane::new().horizontal().wrap()
}

#[test]
fn renders_empty_wrap() {
    let mut w = wrap();
    let term = Emulator::new(&mut *w, Vec2::new(4, 2));
    term.assert_lines([
        "    ",
        "    ",
    ]);
}

#[test]
fn single_row_fits_in_width() {
    let mut w = wrap()
        .y_place(Place::Start)
        .child(chip("AA"))
        .child(chip("BB"))
        .child(chip("CC"));
    let term = Emulator::new(&mut *w, Vec2::new(8, 2));
    term.assert_lines([
        "AABBCC  ",
        "        ",
    ]);
}

#[test]
fn children_wrap_when_exceeding_width() {
    let mut w = wrap()
        .y_place(Place::Start)
        .child(chip("AA"))
        .child(chip("BB"))
        .child(chip("CC"));
    let term = Emulator::new(&mut *w, Vec2::new(4, 3));
    term.assert_lines([
        "AABB",
        "CC  ",
        "    ",
    ]);
}

#[test]
fn vertical_orientation_wraps_into_columns() {
    let mut w = Pane::new()
        .vertical()
        .wrap()
        .x_place(Place::Start)
        .child(chip("A"))
        .child(chip("B"))
        .child(chip("C"))
        .child(chip("D"));
    let term = Emulator::new(&mut *w, Vec2::new(3, 3));
    term.assert_lines([
        "AD ",
        "B  ",
        "C  ",
    ]);
}

#[test]
fn resize_narrower_adds_rows() {
    let mut w = wrap()
        .y_place(Place::Start)
        .child(chip("AA"))
        .child(chip("BB"))
        .child(chip("CC"))
        .child(chip("DD"));
    let mut term = Emulator::new(&mut *w, Vec2::new(8, 3));
    term.assert_lines([
        "AABBCCDD",
        "        ",
        "        ",
    ]);
    term.update(&mut *w, &[RuntimeEvent::Resize(Vec2::new(4, 3))]);
    term.assert_lines([
        "AABB",
        "CCDD",
        "    ",
    ]);
}

#[test]
fn resize_wider_collapses_rows() {
    let mut w = wrap()
        .y_place(Place::Start)
        .child(chip("AA"))
        .child(chip("BB"))
        .child(chip("CC"));
    let mut term = Emulator::new(&mut *w, Vec2::new(2, 4));
    term.assert_lines([
        "AA",
        "BB",
        "CC",
        "  ",
    ]);
    term.update(&mut *w, &[RuntimeEvent::Resize(Vec2::new(6, 4))]);
    term.assert_lines([
        "AABBCC",
        "      ",
        "      ",
        "      ",
    ]);
}

#[test]
fn wrap_align_middle_centers_each_row() {
    let mut w = wrap()
        .y_place(Place::Start)
        .x_place(Place::Middle)
        .child(chip("AA"))
        .child(chip("BB"))
        .child(chip("CC"));
    let term = Emulator::new(&mut *w, Vec2::new(6, 2));
    term.assert_lines([
        "AABBCC",
        "      ",
    ]);
}

#[test]
fn wrap_align_end_pushes_children_to_far_edge() {
    let mut w = wrap()
        .y_place(Place::Start)
        .x_place(Place::End)
        .child(chip("AA"))
        .child(chip("BB"))
        .child(chip("CC"));
    let term = Emulator::new(&mut *w, Vec2::new(6, 2));
    term.assert_lines([
        "AABBCC",
        "      ",
    ]);
}

#[test]
fn wrap_gap_inserts_spacing_between_children() {
    let mut w = wrap()
        .y_place(Place::Start)
        .gap(1)
        .child(chip("AA"))
        .child(chip("BB"))
        .child(chip("CC"));
    let term = Emulator::new(&mut *w, Vec2::new(10, 2));
    term.assert_lines([
        "AA BB CC  ",
        "          ",
    ]);
}

#[test]
fn cross_gap_separates_wrapped_rows() {
    let mut w = wrap()
        .y_place(Place::Start)
        .cross_gap(1)
        .child(chip("AA"))
        .child(chip("BB"))
        .child(chip("CC"));
    let term = Emulator::new(&mut *w, Vec2::new(4, 4));
    term.assert_lines([
        "AABB",
        "    ",
        "CC  ",
        "    ",
    ]);
}

#[test]
fn varying_width_children_pack_greedily() {
    let mut w = wrap()
        .y_place(Place::Start)
        .child(chip("AAA"))
        .child(chip("B"))
        .child(chip("CC"))
        .child(chip("DDDD"));
    let term = Emulator::new(&mut *w, Vec2::new(6, 3));
    term.assert_lines([
        "AAABCC",
        "DDDD  ",
        "      ",
    ]);
}

#[test]
fn nested_wraps_each_wrap_independently() {
    let inner = wrap()
        .y_place(Place::Start)
        .child(chip("a"))
        .child(chip("b"))
        .child(chip("c"));
    let mut outer = wrap()
        .y_place(Place::Start)
        .child(inner as Box<dyn Widget>)
        .child(chip("XX"));
    let term = Emulator::new(&mut *outer, Vec2::new(4, 3));
    term.assert_lines([
        "abc ",
        "XX  ",
        "    ",
    ]);
}

/// Returns the [`Style`] of the first byte of `text` in `snap`.
fn style_at_text(snap: &StyledString, text: &str) -> Style {
    let byte = snap.text.find(text).expect("text not found in snapshot");
    let mut used = 0;
    for span in &snap.spans {
        if byte < used + span.len {
            return span.style;
        }
        used += span.len;
    }
    panic!("byte offset past end of spans");
}

#[test]
fn child_inherits_reverse_as_concrete_swapped_colors() {
    let mut root = Pane::new()
        .style(Style::new().fg(Color::Base256(4)).reverse())
        .children([Text::new().content("Hi")]);
    let term = Emulator::new(&mut *root, Vec2::new(4, 1));
    let snap = term.get_snapshot();
    let style = style_at_text(&snap, "Hi");
    assert_eq!(style.fg, Some(Color::Background));
    assert_eq!(style.bg, Some(Color::Base256(4)));
    assert!(!style.has_reverse());
}

#[test]
fn child_keeps_its_own_reverse() {
    let mut root = Pane::new().children([
        Text::new().content("Hi".reverse().fg(Color::Base256(4))),
    ]);
    let term = Emulator::new(&mut *root, Vec2::new(4, 1));
    let snap = term.get_snapshot();
    let style = style_at_text(&snap, "Hi");
    assert!(style.has_reverse());
    assert_eq!(style.fg, Some(Color::Base256(4)));
}
