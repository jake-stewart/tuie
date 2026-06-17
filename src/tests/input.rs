//! Integration tests for the input widget.

use chord_macro::chord;
use tuie::emulator::Emulator;
use tuie::prelude::*;

#[test]
fn renders_empty() {
    let mut input = Input::new();
    let term = Emulator::new(&mut *input, Vec2::new(5, 1));
    term.assert_lines(["     "]);
}

#[test]
fn renders_initial_content() {
    let mut input = Input::new().content("hello");
    let term = Emulator::new(&mut *input, Vec2::new(7, 1));
    term.assert_lines(["hello  "]);
    assert_eq!(input.get_string(), "hello");
}

#[test]
fn renders_placeholder_when_empty() {
    let mut input = Input::new().placeholder(Text::new().content("Name"));
    let term = Emulator::new(&mut *input, Vec2::new(6, 1));
    term.assert_lines(["Name  "]);
}

#[test]
fn placeholder_hidden_when_content_present() {
    let mut input = Input::new()
        .placeholder(Text::new().content("Name"))
        .content("a");
    let term = Emulator::new(&mut *input, Vec2::new(6, 1));
    term.assert_lines(["a     "]);
}

#[test]
fn typing_inserts_characters() {
    let mut input = Input::new();
    let mut term = Emulator::new(&mut *input, Vec2::new(6, 1));
    term.update(&mut *input, &[chord!('h').into(), chord!('i').into()]);
    assert_eq!(input.get_string(), "hi");
    term.assert_lines(["hi    "]);
}

#[test]
fn backspace_deletes_previous_character() {
    let mut input = Input::new().content("abc");
    let mut term = Emulator::new(&mut *input, Vec2::new(5, 1));
    term.update(&mut *input, &[chord!(End).into(), chord!(Backspace).into()]);
    assert_eq!(input.get_string(), "ab");
    term.assert_lines(["ab   "]);
}

#[test]
fn delete_removes_forward_character() {
    let mut input = Input::new().content("abc");
    let mut term = Emulator::new(&mut *input, Vec2::new(5, 1));
    term.update(&mut *input, &[chord!(Home).into(), chord!(Delete).into()]);
    assert_eq!(input.get_string(), "bc");
    term.assert_lines(["bc   "]);
}

#[test]
fn home_and_end_move_cursor() {
    let mut input = Input::new().content("abc");
    let mut term = Emulator::new(&mut *input, Vec2::new(6, 1));
    term.update(&mut *input, &[chord!(Home).into(), chord!('X').into()]);
    assert_eq!(input.get_string(), "Xabc");
    term.update(&mut *input, &[chord!(End).into(), chord!('Y').into()]);
    assert_eq!(input.get_string(), "XabcY");
    term.assert_lines(["XabcY "]);
}

#[test]
fn arrow_keys_move_cursor() {
    let mut input = Input::new().content("abc");
    let mut term = Emulator::new(&mut *input, Vec2::new(6, 1));
    term.update(&mut *input, &[chord!(Left).into(), chord!('Z').into()]);
    assert_eq!(input.get_string(), "abZc");
    term.update(&mut *input, &[chord!(Left).into(), chord!(Left).into(), chord!('Q').into()]);
    assert_eq!(input.get_string(), "aQbZc");
}

#[test]
fn resize_re_renders() {
    let mut input = Input::new().content("hello");
    let mut term = Emulator::new(&mut *input, Vec2::new(8, 1));
    term.assert_lines(["hello   "]);
    term.update(&mut *input, &[RuntimeEvent::Resize(Vec2::new(4, 1))]);
    let snap = term.get_snapshot_text();
    assert_eq!(snap.chars().count(), 4);
}

#[test]
fn multiline_wraps_content() {
    let mut input = Input::new().multiline().content("abcdef");
    let term = Emulator::new(&mut *input, Vec2::new(3, 2));
    term.assert_lines(["ab ", "cd "]);
}

#[test]
fn single_line_strips_newlines_from_content() {
    let mut input = Input::new().content("a\nb\nc");
    let term = Emulator::new(&mut *input, Vec2::new(5, 1));
    term.assert_lines(["abc  "]);
    assert_eq!(input.get_string(), "abc");
}

#[test]
fn popup_input_reports_terminal_cursor() {
    let popup_pos = Vec2::new(4, 1);
    let mut root = Pane::new().children([Text::new().content("root") as Box<dyn Widget>]);
    let mut term = Emulator::new(&mut *root, Vec2::new(20, 5));

    tuie::open_popup(
        Popup::new(
            Input::new()
                .content("abc")
                .bindings(ModernBindings::new) as Box<dyn Widget>,
        )
        .placement(
            Placement::center()
                .anchor_point(Vec2::of(Align::Start))
                .popup_point(Vec2::of(Align::Start))
                .offset(popup_pos),
        ),
    );
    term.update(&mut *root, &[]);
    assert_eq!(term.get_cursor(), Some((CursorShape::Beam, Vec2::new(7, 1))));
}

#[test]
fn ctrl_backspace_deletes_word() {
    let mut input = Input::new().content("hello world");
    let mut term = Emulator::new(&mut *input, Vec2::new(13, 1));
    term.update(&mut *input, &[chord!(End).into(), chord!(Ctrl + Backspace).into()]);
    assert_eq!(input.get_string(), "hello ");
}
