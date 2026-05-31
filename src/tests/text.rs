//! Integration tests for the text widget.

use tuie::prelude::*;
use tuie::emulator::Emulator;

#[test]
fn renders_empty() {
    let mut root = Pane::new().children([Text::new()]);
    let term = Emulator::new(&mut *root, Vec2::new(4, 1));
    term.assert_lines(["    "]);
}

#[test]
fn renders_single_line() {
    let mut root = Pane::new().children([Text::new().content("hello")]);
    let term = Emulator::new(&mut *root, Vec2::new(7, 1));
    term.assert_lines(["hello  "]);
}

#[test]
fn renders_literal_newlines_as_multiple_lines() {
    let mut root = Pane::new().children([Text::new().content("ab\ncd")]);
    let term = Emulator::new(&mut *root, Vec2::new(4, 2));
    term.assert_lines([
        "ab  ",
        "cd  ",
    ]);
}

#[test]
fn truncate_clips_without_marker() {
    let mut root = Pane::new().children([
        Text::new().content("abcdefghij").truncate(),
    ]);
    let term = Emulator::new(&mut *root, Vec2::new(5, 1));
    term.assert_lines(["abcde"]);
}

#[test]
fn ellipsis_marks_overflow() {
    let mut root = Pane::new().children([
        Text::new().content("hello world").ellipsis(),
    ]);
    let term = Emulator::new(&mut *root, Vec2::new(7, 1));
    let snap = term.get_snapshot_text();
    assert_eq!(snap.chars().count(), 7);
    assert!(snap.contains('…'), "expected ellipsis in {snap:?}");
    assert!(snap.starts_with("hello"), "expected to start with hello, got {snap:?}");
}

#[test]
fn wrap_breaks_at_grapheme_boundary() {
    let mut root = Pane::new().children([
        Text::new().content("abcdef").wrap(),
    ]);
    let term = Emulator::new(&mut *root, Vec2::new(3, 2));
    term.assert_lines([
        "abc",
        "def",
    ]);
}

#[test]
fn word_wrap_breaks_at_word_boundaries() {
    let mut root = Pane::new().children([
        Text::new().content("hello world").word_wrap(),
    ]);
    let term = Emulator::new(&mut *root, Vec2::new(6, 2));
    term.assert_lines([
        "hello ",
        "world ",
    ]);
}

#[test]
fn styled_content_renders_without_panic() {
    let mut root = Pane::new().children([
        Text::new().content("hi".fg(Color::RED).bold()),
    ]);
    let term = Emulator::new(&mut *root, Vec2::new(4, 1));
    term.assert_lines(["hi  "]);
}

#[test]
fn align_left_center_right() {
    let mut left = Pane::new().children([
        Text::new().content("hi").truncate().left(),
    ]);
    let term = Emulator::new(&mut *left, Vec2::new(6, 1));
    term.assert_lines(["hi    "]);

    let mut center = Pane::new().children([
        Text::new().content("hi").truncate().center(),
    ]);
    let term = Emulator::new(&mut *center, Vec2::new(6, 1));
    term.assert_lines(["  hi  "]);

    let mut right = Pane::new().children([
        Text::new().content("hi").truncate().right(),
    ]);
    let term = Emulator::new(&mut *right, Vec2::new(6, 1));
    term.assert_lines(["    hi"]);
}

#[test]
fn wide_unicode_takes_two_columns() {
    let mut root = Pane::new().children([
        Text::new().content("漢a").truncate(),
    ]);
    let term = Emulator::new(&mut *root, Vec2::new(5, 1));
    let snap = term.get_snapshot_text();
    assert!(snap.starts_with("漢a"), "got {snap:?}");
    assert_eq!(snap.chars().filter(|c| *c == ' ').count(), 2);
}

#[test]
fn resize_reflows_wrapped_text() {
    let mut root = Pane::new().children([
        Text::new().content("abcdef").wrap(),
    ]);
    let mut term = Emulator::new(&mut *root, Vec2::new(6, 2));
    term.assert_lines([
        "abcdef",
        "      ",
    ]);
    term.update(&mut *root, &[RuntimeEvent::Resize(Vec2::new(3, 2))]);
    term.assert_lines([
        "abc",
        "def",
    ]);
}

#[test]
fn resize_retruncates_with_ellipsis() {
    let mut root = Pane::new().children([
        Text::new().content("hello world").ellipsis(),
    ]);
    let mut term = Emulator::new(&mut *root, Vec2::new(11, 1));
    term.assert_lines(["hello world"]);
    term.update(&mut *root, &[RuntimeEvent::Resize(Vec2::new(6, 1))]);
    let snap = term.get_snapshot_text();
    assert_eq!(snap.chars().count(), 6);
    assert!(snap.contains('…'), "expected ellipsis, got {snap:?}");
}

#[test]
fn content_builder_sets_initial_text() {
    let mut root = Pane::new().children([Text::new().content("first")]);
    let term = Emulator::new(&mut *root, Vec2::new(7, 1));
    term.assert_lines(["first  "]);
}

#[test]
fn get_str_returns_underlying_text() {
    let text = Text::new().content("payload");
    assert_eq!(text.get_str(), "payload");
    assert_eq!(text.len(), 7);
}
