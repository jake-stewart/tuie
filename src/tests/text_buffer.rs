//! Tests for the text buffer, cursor, layout, and document traits.

use tuie::prelude::*;
use tuie::emulator::Emulator;

fn make_text(s: &str) -> Box<Text> {
    Text::new().content(s)
}

#[test]
fn buffer_len_matches_byte_length() {
    let t = make_text("hello");
    assert_eq!(t.len(), 5);
    let t2 = make_text("héllo");
    assert_eq!(t2.len(), 6);
    let t3 = make_text("");
    assert_eq!(t3.len(), 0);
}

#[test]
fn buffer_slice_returns_copied_substring() {
    let t = make_text("hello world");
    assert_eq!(t.slice(0, 5), "hello");
    assert_eq!(t.slice(6, 11), "world");
    assert_eq!(t.slice(5, 6), " ");
    assert_eq!(t.slice(0, 0), "");
}

#[test]
fn buffer_replace_range_inserts_at_start() {
    let mut t = make_text("world");
    t.replace_range(0, 0, "hello ");
    assert_eq!(t.get_string(), "hello world");
    assert_eq!(t.len(), 11);
}

#[test]
fn buffer_replace_range_inserts_at_end() {
    let mut t = make_text("hello");
    let len = t.len();
    t.replace_range(len, len, " world");
    assert_eq!(t.get_string(), "hello world");
}

#[test]
fn buffer_replace_range_deletes_substring() {
    let mut t = make_text("hello world");
    t.replace_range(5, 11, "");
    assert_eq!(t.get_string(), "hello");
}

#[test]
fn buffer_replace_range_substitutes() {
    let mut t = make_text("hello world");
    t.replace_range(6, 11, "rust!");
    assert_eq!(t.get_string(), "hello rust!");
}

#[test]
fn buffer_is_char_boundary_handles_multibyte() {
    let t = make_text("héllo");
    assert!(t.is_char_boundary(0));
    assert!(t.is_char_boundary(1));
    assert!(!t.is_char_boundary(2));
    assert!(t.is_char_boundary(3));
    assert!(t.is_char_boundary(6));
}

#[test]
fn buffer_chunks_yields_full_range_for_string_backed() {
    let t = make_text("hello world");
    let collected: String = t.chunks(0, 11).collect();
    assert_eq!(collected, "hello world");
    let partial: String = t.chunks(6, 11).collect();
    assert_eq!(partial, "world");
}

#[test]
fn buffer_insert_then_delete_round_trips() {
    let mut t = make_text("hello");
    let original = t.get_string();
    t.replace_range(2, 2, "XYZ");
    assert_eq!(t.get_string(), "heXYZllo");
    t.replace_range(2, 5, "");
    assert_eq!(t.get_string(), original);
    assert_eq!(t.len(), 5);
}

#[test]
fn cursor_starts_at_index_constructed_from() {
    let t = make_text("hello");
    let c = t.cursor(0);
    assert_eq!(c.get_index(), 0);
    let c = t.cursor(3);
    assert_eq!(c.get_index(), 3);
}

#[test]
fn cursor_get_char_returns_null_at_end_of_buffer() {
    let t = make_text("ab");
    let c = t.cursor(2);
    assert_eq!(c.get_char(&*t), '\0');
}

#[test]
fn cursor_get_char_on_empty_buffer_is_null() {
    let t = make_text("");
    let c = t.cursor(0);
    assert_eq!(c.get_char(&*t), '\0');
}

#[test]
fn cursor_get_char_at_each_byte() {
    let t = make_text("abc");
    assert_eq!(t.cursor(0).get_char(&*t), 'a');
    assert_eq!(t.cursor(1).get_char(&*t), 'b');
    assert_eq!(t.cursor(2).get_char(&*t), 'c');
}

#[test]
fn cursor_set_index_snaps_to_char_boundary() {
    let t = make_text("héllo");
    let mut c = t.cursor(0);
    c.set_index(&*t, 2);
    assert_eq!(c.get_index(), 1);
    assert_eq!(c.get_char(&*t), 'é');
}

#[test]
fn cursor_matches_checks_prefix() {
    let t = make_text("hello world");
    let c = t.cursor(6);
    assert!(c.matches(&*t, "world"));
    assert!(c.matches(&*t, "w"));
    assert!(!c.matches(&*t, "hello"));
}

#[test]
fn next_char_advances_one_codepoint() {
    let t = make_text("abc");
    let mut c = t.cursor(0);
    c.next_char(&*t);
    assert_eq!(c.get_index(), 1);
    c.next_char(&*t);
    assert_eq!(c.get_index(), 2);
}

#[test]
fn next_char_advances_by_codepoint_byte_width() {
    let t = make_text("héllo");
    let mut c = t.cursor(0);
    c.next_char(&*t);
    assert_eq!(c.get_index(), 1);
    c.next_char(&*t);
    assert_eq!(c.get_index(), 3);
}

#[test]
fn prev_char_retreats_one_codepoint() {
    let t = make_text("abc");
    let mut c = t.cursor(2);
    c.prev_char(&*t);
    assert_eq!(c.get_index(), 1);
    c.prev_char(&*t);
    assert_eq!(c.get_index(), 0);
}

#[test]
fn prev_char_at_zero_is_no_op() {
    let t = make_text("abc");
    let mut c = t.cursor(0);
    c.prev_char(&*t);
    assert_eq!(c.get_index(), 0);
}

#[test]
fn next_char_at_end_is_no_op() {
    let t = make_text("ab");
    let mut c = t.cursor(2);
    c.next_char(&*t);
    assert_eq!(c.get_index(), 2);
}

#[test]
fn next_grapheme_handles_combining_marks() {
    let t = make_text("a\u{301}b");
    assert_eq!(t.len(), 4);
    let mut c = t.cursor(0);
    c.next_grapheme(&*t);
    assert_eq!(c.get_index(), 3);
    c.next_grapheme(&*t);
    assert_eq!(c.get_index(), 4);
}

#[test]
fn prev_grapheme_handles_combining_marks() {
    let t = make_text("a\u{301}b");
    let mut c = t.cursor(4);
    c.prev_grapheme(&*t);
    assert_eq!(c.get_index(), 3);
    c.prev_grapheme(&*t);
    assert_eq!(c.get_index(), 0);
}

#[test]
fn next_grapheme_handles_zwj_emoji_sequence() {
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
    let s = format!("{}X", family);
    let t = make_text(&s);
    let mut c = t.cursor(0);
    c.next_grapheme(&*t);
    assert_eq!(c.get_index(), family.len());
    c.next_grapheme(&*t);
    assert_eq!(c.get_index(), family.len() + 1);
}

#[test]
fn move_grapheme_directional() {
    let t = make_text("abc");
    let mut c = t.cursor(0);
    c.move_grapheme(&*t, Sign::Positive);
    assert_eq!(c.get_index(), 1);
    c.move_grapheme(&*t, Sign::Negative);
    assert_eq!(c.get_index(), 0);
}

#[test]
fn seek_chars_positive_and_negative() {
    let t = make_text("abcdef");
    let mut c = t.cursor(0);
    c.seek_chars(&*t, 3);
    assert_eq!(c.get_index(), 3);
    c.seek_chars(&*t, -2);
    assert_eq!(c.get_index(), 1);
}

#[test]
fn line_start_moves_to_start_of_line() {
    let t = make_text("abc\ndef");
    let mut c = t.cursor(5);
    c.line_start(&*t);
    assert_eq!(c.get_index(), 4);
}

#[test]
fn line_start_at_first_line_goes_to_zero() {
    let t = make_text("abc\ndef");
    let mut c = t.cursor(2);
    c.line_start(&*t);
    assert_eq!(c.get_index(), 0);
}

#[test]
fn line_end_moves_to_newline_or_end_of_buffer() {
    let t = make_text("abc\ndef");
    let mut c = t.cursor(0);
    c.line_end(&*t);
    assert_eq!(c.get_index(), 3);
    let mut c = t.cursor(4);
    c.line_end(&*t);
    assert_eq!(c.get_index(), 7);
}

#[test]
fn next_line_start_skips_past_newline() {
    let t = make_text("abc\ndef\nghi");
    let mut c = t.cursor(0);
    c.next_line_start(&*t);
    assert_eq!(c.get_index(), 4);
    c.next_line_start(&*t);
    assert_eq!(c.get_index(), 8);
}

#[test]
fn next_line_start_at_last_line_goes_to_len() {
    let t = make_text("abc\ndef");
    let mut c = t.cursor(4);
    c.next_line_start(&*t);
    assert_eq!(c.get_index(), 7);
}

#[test]
fn prev_line_start_walks_back_over_lines() {
    let t = make_text("abc\ndef\nghi");
    let mut c = t.cursor(9);
    c.prev_line_start(&*t);
    assert_eq!(c.get_index(), 4);
    c.prev_line_start(&*t);
    assert_eq!(c.get_index(), 0);
    c.prev_line_start(&*t);
    assert_eq!(c.get_index(), 0);
}

#[test]
fn linewise_end_passes_newline_or_to_phantom_newline() {
    let t = make_text("abc\ndef");
    let mut c = t.cursor(0);
    c.linewise_end(&*t);
    assert_eq!(c.get_index(), 4);
    let mut c = t.cursor(4);
    c.linewise_end(&*t);
    assert_eq!(c.get_index(), t.len() + 1);
}

#[test]
fn find_char_forward_locates_and_moves() {
    let t = make_text("abc def");
    let mut c = t.cursor(0);
    c.find_char_forward(&*t, ' ');
    assert_eq!(c.get_index(), 3);
}

#[test]
fn find_char_forward_with_no_match_does_not_move() {
    let t = make_text("abc");
    let mut c = t.cursor(0);
    c.find_char_forward(&*t, 'z');
    assert_eq!(c.get_index(), 0);
}

#[test]
fn find_char_backward_locates_previous() {
    let t = make_text("abc def");
    let mut c = t.cursor(7);
    c.find_char_backward(&*t, 'b');
    assert_eq!(c.get_index(), 1);
}

#[test]
fn find_str_forward_locates_substring() {
    let t = make_text("foo bar baz");
    let mut c = t.cursor(0);
    c.find_str_forward(&*t, "bar");
    assert_eq!(c.get_index(), 4);
    assert!(c.matches(&*t, "bar"));
}

#[test]
fn find_str_backward_locates_substring() {
    let t = make_text("foo bar foo");
    let mut c = t.cursor(11);
    c.find_str_backward(&*t, "foo");
    assert_eq!(c.get_index(), 8);
}

#[test]
fn document_start_zeros_index() {
    let t = make_text("abc");
    let mut c = t.cursor(2);
    c.document_start();
    assert_eq!(c.get_index(), 0);
}

#[test]
fn document_end_reaches_buffer_len() {
    let t = make_text("abc");
    let mut c = t.cursor(0);
    c.document_end(&*t);
    assert_eq!(c.get_index(), t.len());
    assert_eq!(c.get_char(&*t), '\0');
}

#[test]
fn document_end_on_empty_buffer_is_zero() {
    let t = make_text("");
    let mut c = t.cursor(0);
    c.document_end(&*t);
    assert_eq!(c.get_index(), 0);
}

#[test]
fn move_document_end_directional() {
    let t = make_text("abc");
    let mut c = t.cursor(1);
    c.move_document_end(&*t, Sign::Positive);
    assert_eq!(c.get_index(), 3);
    c.move_document_end(&*t, Sign::Negative);
    assert_eq!(c.get_index(), 0);
}

#[test]
fn layout_index_to_pos_maps_single_line() {
    let mut t = make_text("hello");
    let _term = Emulator::new(&mut *t, Vec2::new(10, 1));
    assert_eq!(t.index_to_virtual_pos(0, Sign::Positive), Vec2::new(0, 0));
    assert_eq!(t.index_to_virtual_pos(3, Sign::Positive), Vec2::new(3, 0));
    assert_eq!(t.index_to_virtual_pos(5, Sign::Positive), Vec2::new(5, 0));
}

#[test]
fn layout_index_to_pos_maps_multiple_lines() {
    let mut t = make_text("ab\ncd");
    let _term = Emulator::new(&mut *t, Vec2::new(5, 2));
    assert_eq!(t.index_to_virtual_pos(0, Sign::Positive), Vec2::new(0, 0));
    assert_eq!(t.index_to_virtual_pos(2, Sign::Positive), Vec2::new(2, 0));
    assert_eq!(t.index_to_virtual_pos(3, Sign::Positive), Vec2::new(0, 1));
    assert_eq!(t.index_to_virtual_pos(5, Sign::Positive), Vec2::new(2, 1));
}

#[test]
fn layout_pos_to_index_round_trips() {
    let mut t = make_text("hello\nworld");
    let _term = Emulator::new(&mut *t, Vec2::new(10, 2));
    for i in 0..=t.len() {
        let pos = t.index_to_virtual_pos(i, Sign::Negative);
        let back = t.pos_to_index(pos);
        assert_eq!(back, i, "round-trip failed at index {i} via pos {pos:?}");
    }
}

#[test]
fn layout_get_visible_size_matches_terminal_size() {
    let mut t = make_text("hi");
    let _term = Emulator::new(&mut *t, Vec2::new(20, 5));
    let size = TextLayout::get_visible_size(&*t);
    assert_eq!(size, Vec2::new(20, 5));
}

#[test]
fn insert_then_back_delete_at_cursor_round_trips() {
    let mut t = make_text("abcdef");
    let mut c = t.cursor(3);
    let pos = c.get_index();
    t.replace_range(pos, pos, "XYZ");
    c.set_index(&*t, pos + 3);
    assert_eq!(t.get_string(), "abcXYZdef");
    assert_eq!(c.get_index(), 6);
    let mut start = c.clone();
    for _ in 0..3 {
        start.prev_grapheme(&*t);
    }
    let lo = start.get_index();
    let hi = c.get_index();
    t.replace_range(lo, hi, "");
    c.set_index(&*t, lo);
    assert_eq!(t.get_string(), "abcdef");
    assert_eq!(c.get_index(), 3);
}

#[test]
fn empty_buffer_cursor_stays_at_zero_under_all_motions() {
    let t = make_text("");
    let mut c = t.cursor(0);
    c.next_char(&*t);
    assert_eq!(c.get_index(), 0);
    c.next_grapheme(&*t);
    assert_eq!(c.get_index(), 0);
    c.line_start(&*t);
    assert_eq!(c.get_index(), 0);
    c.line_end(&*t);
    assert_eq!(c.get_index(), 0);
    c.next_line_start(&*t);
    assert_eq!(c.get_index(), 0);
    c.prev_line_start(&*t);
    assert_eq!(c.get_index(), 0);
    c.document_end(&*t);
    assert_eq!(c.get_index(), 0);
}

#[test]
fn cursor_ordering_reflects_byte_index() {
    let t = make_text("abc");
    let a = t.cursor(0);
    let b = t.cursor(1);
    let c = t.cursor(2);
    assert!(a < b);
    assert!(b < c);
    assert!(a < c);
    assert_eq!(a, t.cursor(0));
}

#[test]
fn document_method_returns_cursor_at_requested_position() {
    let t = make_text("hello");
    let c = t.cursor(2);
    assert_eq!(c.get_index(), 2);
    assert_eq!(c.get_char(&*t), 'l');
}

