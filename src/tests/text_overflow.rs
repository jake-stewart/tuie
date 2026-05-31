//! Tests for [`TextOverflowLineIterator`].

use tuie::prelude::{Align, Vec2};
use tuie::util::text_overflow::{TextOverflow, TextOverflowLineIterator};

#[derive(Debug, PartialEq)]
struct Line {
    content: String,
    width: usize,
    marker: String,
    marker_width: usize,
    trailing_whitespace: bool,
    y: usize,
}

fn collect(
    overflow: &TextOverflow,
    text: &str,
    max: Vec2<usize>,
    align: Align,
    tabstop: Option<u8>,
) -> Vec<Line> {
    TextOverflowLineIterator::new(*overflow, max, text, align, tabstop)
        .map(|r| Line {
            content: r.content.to_string(),
            width: r.width,
            marker: r.marker.to_string(),
            marker_width: r.marker_width,
            trailing_whitespace: r.trailing_whitespace,
            y: r.y,
        })
        .collect()
}

fn collect_wrap(text: &str, width: usize) -> Vec<Line> {
    collect(
        TextOverflow::WRAP,
        text,
        Vec2::new(width, usize::MAX),
        Align::Start,
        None,
    )
}

fn collect_word_wrap(text: &str, width: usize) -> Vec<Line> {
    collect(
        TextOverflow::WORD_WRAP,
        text,
        Vec2::new(width, usize::MAX),
        Align::Start,
        None,
    )
}

#[test]
fn empty_input_yields_one_empty_line() {
    let lines = collect_wrap("", 10);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].content, "");
    assert_eq!(lines[0].width, 0);
    assert!(!lines[0].trailing_whitespace);
}

#[test]
fn zero_width_collapses_to_one_empty_line() {
    let lines = collect_wrap("hello world", 0);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].content, "");
    assert_eq!(lines[0].width, 0);
}

#[test]
fn one_cell_width_breaks_each_grapheme() {
    let lines = collect_wrap("abc", 1);
    assert_eq!(lines.len(), 3);
    for (i, expected) in ["a", "b", "c"].iter().enumerate() {
        assert_eq!(lines[i].content, *expected);
        assert_eq!(lines[i].width, 1);
        assert_eq!(lines[i].y, i);
    }
}

#[test]
fn content_under_width_is_one_unbroken_line() {
    let lines = collect_wrap("hello", 10);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].content, "hello");
    assert_eq!(lines[0].width, 5);
    assert_eq!(lines[0].marker_width, 0);
}

#[test]
fn content_at_exact_width_is_one_unbroken_line() {
    let lines = collect_wrap("hello", 5);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].content, "hello");
    assert_eq!(lines[0].width, 5);
}

#[test]
fn newline_forces_break() {
    let lines = collect_wrap("ab\ncd", 10);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].content, "ab");
    assert_eq!(lines[0].y, 0);
    assert!(lines[0].trailing_whitespace);
    assert_eq!(lines[1].content, "cd");
    assert_eq!(lines[1].y, 1);
    assert!(!lines[1].trailing_whitespace);
}

#[test]
fn trailing_newline_yields_empty_final_line() {
    let lines = collect_wrap("ab\n", 10);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].content, "ab");
    assert_eq!(lines[1].content, "");
    assert_eq!(lines[1].width, 0);
}

#[test]
fn consecutive_newlines_yield_blank_lines() {
    let lines = collect_wrap("a\n\nb", 10);
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0].content, "a");
    assert_eq!(lines[1].content, "");
    assert_eq!(lines[2].content, "b");
}

#[test]
fn wrap_breaks_at_exact_width_when_no_word_boundary() {
    let lines = collect_wrap("abcdef", 3);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].content, "abc");
    assert_eq!(lines[0].width, 3);
    assert_eq!(lines[1].content, "def");
    assert_eq!(lines[1].width, 3);
}

#[test]
fn wrap_breaks_mid_word_for_long_runs() {
    let lines = collect_wrap("abcdefghij", 4);
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0].content, "abcd");
    assert_eq!(lines[1].content, "efgh");
    assert_eq!(lines[2].content, "ij");
}

#[test]
fn word_wrap_prefers_whitespace_boundary() {
    let lines = collect_word_wrap("hello world", 8);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].content, "hello");
    assert_eq!(lines[0].width, 5);
    assert_eq!(lines[0].marker_width, 0);
    assert_eq!(lines[1].content, "world");
    assert_eq!(lines[1].width, 5);
}

#[test]
fn word_wrap_hard_breaks_unbreakable_run_with_marker() {
    let lines = collect_word_wrap("abcdefgh", 4);
    assert!(lines.len() >= 2);
    assert_eq!(lines[0].content, "abc");
    assert_eq!(lines[0].width, 3);
    assert_eq!(lines[0].marker, "-");
    assert_eq!(lines[0].marker_width, 1);
    let joined: String = lines.iter().map(|l| l.content.as_str()).collect();
    assert_eq!(joined, "abcdefgh");
}

#[test]
fn word_wrap_trims_extra_inter_word_whitespace() {
    let lines = collect_word_wrap("a  bb", 3);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].content, "a");
    assert_eq!(lines[1].content, "bb");
}

#[test]
fn cjk_wide_chars_each_take_two_cells() {
    let lines = collect_wrap("\u{6f22}\u{5b57}", 3);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].content, "\u{6f22}");
    assert_eq!(lines[0].width, 2);
    assert_eq!(lines[1].content, "\u{5b57}");
    assert_eq!(lines[1].width, 2);
}

#[test]
fn cjk_wide_char_does_not_split_to_satisfy_exact_width() {
    let lines = collect_wrap("\u{6f22}", 2);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].content, "\u{6f22}");
    assert_eq!(lines[0].width, 2);
}

#[test]
fn combining_mark_glues_to_base_grapheme() {
    let lines = collect_wrap("a\u{301}b", 2);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].content, "a\u{301}b");
    assert_eq!(lines[0].width, 2);
}

#[test]
fn combining_mark_at_width_boundary_wraps_with_base() {
    let lines = collect_wrap("a\u{301}b", 1);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].content, "a\u{301}");
    assert_eq!(lines[1].content, "b");
}

#[test]
fn truncate_clips_without_visible_marker() {
    let lines = collect(
        TextOverflow::TRUNCATE,
        "hello world",
        Vec2::new(5, usize::MAX),
        Align::Start,
        None,
    );
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].content, "hello");
    assert_eq!(lines[0].width, 5);
    assert_eq!(lines[0].marker, "");
    assert_eq!(lines[0].marker_width, 0);
}

#[test]
fn ellipsis_truncates_at_word_boundary_with_marker() {
    let lines = collect(
        TextOverflow::ELLIPSIS,
        "hello world",
        Vec2::new(8, usize::MAX),
        Align::Start,
        None,
    );
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].content, "hello");
    assert_eq!(lines[0].width, 5);
    assert_eq!(lines[0].marker, "\u{2026}");
    assert_eq!(lines[0].marker_width, 1);
}

#[test]
fn ellipsis_marker_dropped_when_width_too_small() {
    let lines = collect(
        TextOverflow::ELLIPSIS,
        "hello",
        Vec2::new(1, usize::MAX),
        Align::Start,
        None,
    );
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].marker, "");
    assert_eq!(lines[0].marker_width, 0);
}

#[test]
fn visible_emits_overflowing_line_without_clipping_in_iterator() {
    let lines = collect(
        TextOverflow::VISIBLE,
        "hello world",
        Vec2::new(3, usize::MAX),
        Align::Start,
        None,
    );
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].content, "hello world");
    assert_eq!(lines[0].width, 11);
}

#[test]
fn middle_align_truncate_keeps_middle_slice() {
    let lines = collect(
        TextOverflow::TRUNCATE,
        "abcdefghij",
        Vec2::new(4, usize::MAX),
        Align::Middle,
        None,
    );
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].content, "defg");
    assert_eq!(lines[0].width, 4);
}

#[test]
fn end_align_truncate_clips_from_the_left() {
    let lines = collect(
        TextOverflow::TRUNCATE,
        "abcdef",
        Vec2::new(3, usize::MAX),
        Align::End,
        None,
    );
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].content, "def");
    assert_eq!(lines[0].width, 3);
}

#[test]
fn trailing_whitespace_flag_set_only_at_real_newline() {
    let with_newline = collect_wrap("a\nb", 10);
    assert!(with_newline[0].trailing_whitespace);
    assert!(!with_newline[1].trailing_whitespace);

    let no_newline = collect_wrap("ab", 10);
    assert!(!no_newline[0].trailing_whitespace);
}

#[test]
fn tab_with_tabstop_expands_to_next_stop() {
    let lines = collect(
        TextOverflow::WRAP,
        "a\tb",
        Vec2::new(10, usize::MAX),
        Align::Start,
        Some(4),
    );
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].content, "a\tb");
    assert_eq!(lines[0].width, 5);
}

#[test]
fn tab_without_tabstop_renders_as_two_cell_caret_escape() {
    let lines = collect(
        TextOverflow::WRAP,
        "a\tb",
        Vec2::new(10, usize::MAX),
        Align::Start,
        None,
    );
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].width, 4);
}

#[test]
fn tab_wrapping_at_tab_boundary() {
    let lines = collect(
        TextOverflow::WRAP,
        "a\tb",
        Vec2::new(4, usize::MAX),
        Align::Start,
        Some(4),
    );
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].content, "a\t");
    assert_eq!(lines[0].width, 4);
    assert_eq!(lines[1].content, "b");
    assert_eq!(lines[1].width, 1);
}

#[test]
fn height_limit_flips_truncate_marker_mode_for_overflow_line() {
    let lines = collect(
        TextOverflow::WRAP,
        "abcdefghij",
        Vec2::new(3, 2),
        Align::Start,
        None,
    );
    assert!(lines.len() >= 2);
    assert_eq!(lines[0].content, "abc");
}

#[test]
fn offsets_advance_monotonically_across_wrap() {
    let it = TextOverflowLineIterator::new(
        *TextOverflow::WRAP,
        Vec2::new(3, usize::MAX),
        "abcdefghij",
        Align::Start,
        None,
    );
    let mut prev = 0usize;
    let mut count = 0usize;
    for line in it {
        assert!(line.offset >= prev,
            "offset went backwards: {} -> {}", prev, line.offset);
        prev = line.offset;
        count += 1;
    }
    assert!(count >= 3);
}

#[test]
fn offsets_skip_past_newlines() {
    let it = TextOverflowLineIterator::new(
        *TextOverflow::WRAP,
        Vec2::new(10, usize::MAX),
        "ab\ncd\nef",
        Align::Start,
        None,
    );
    let offsets: Vec<usize> = it.map(|l| l.offset).collect();
    assert_eq!(offsets, vec![0, 3, 6]);
}
