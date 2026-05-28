use tuie::render::color::Color;
use tuie::render::style::{AnsiStyleParser, Span, Style, StyleAttribute, StyledString, StyledStr, Stylize};
use tuie::render::underline::UnderlineType;

#[test]
fn apply_later_overrides_earlier_colors() {
    let a = Style::new().fg(Color::RED).bg(Color::BLUE);
    let b = Style::new().fg(Color::GREEN);
    let merged = a.apply(b);
    assert_eq!(merged.fg, Some(Color::GREEN));
    assert_eq!(merged.bg, Some(Color::BLUE));
}

#[test]
fn apply_preserves_untouched_fields() {
    let base = Style::new().bold().fg(Color::RED);
    let overlay = Style::new().italic();
    let merged = base.apply(overlay);
    assert!(merged.has_bold());
    assert!(merged.has_italic());
    assert_eq!(merged.fg, Some(Color::RED));
}

#[test]
fn apply_explicitly_off_overrides_on() {
    let base = Style::new().bold();
    let overlay = Style::new().bold_if(false);
    let merged = base.apply(overlay);
    assert!(!merged.has_bold());
    assert!(merged.get_attrs_mask() & (StyleAttribute::Bold as u8) != 0);
}

#[test]
fn apply_underline_color_overlays() {
    let base = Style::new().underline(UnderlineType::Single).underline_color(Color::RED);
    let overlay = Style::new().underline(UnderlineType::Curly);
    let merged = base.apply(overlay);
    assert_eq!(merged.underline, Some(UnderlineType::Curly));
    assert_eq!(merged.underline_color, Some(Color::RED));
}

#[test]
fn blend_is_clamped_to_100() {
    let s = Style::new().blend(200);
    assert_eq!(s.get_blend(), Some(100));
}

#[test]
fn blend_apply_overlays() {
    let a = Style::new().blend(20);
    let b = Style::new().blend(80);
    assert_eq!(a.apply(b).get_blend(), Some(80));
    assert_eq!(b.apply(Style::new()).get_blend(), Some(80));
}

#[test]
fn stylize_trait_on_str() {
    let s = "hello".bold();
    assert!(s.style.has_bold());
    assert_eq!(s.text, "hello");

    let s = "y".italic().fg(Color::BLUE).bg(Color::YELLOW);
    assert!(s.style.has_italic());
    assert_eq!(s.style.fg, Some(Color::BLUE));
    assert_eq!(s.style.bg, Some(Color::YELLOW));

    let s = "u".underline(UnderlineType::Curly);
    assert_eq!(s.style.underline, Some(UnderlineType::Curly));

    let s = "b".red_bg();
    assert_eq!(s.style.bg, Some(Color::RED));
}

fn parse_one(input: &str) -> Style {
    let mut p = AnsiStyleParser::new();
    let out = p.parse_line(input);
    let last = out.spans.last().copied().unwrap_or(Span::new(0, Style::new()));
    last.style
}

#[test]
fn ansi_reset_clears_style() {
    let s = parse_one("\x1b[1;31mhi\x1b[0m");
    assert_eq!(s, Style::new());
}

#[test]
fn ansi_attribute_codes_set_and_clear() {
    assert!(parse_one("\x1b[1mx").has_bold());
    assert!(parse_one("\x1b[2mx").has_dim());
    assert!(parse_one("\x1b[3mx").has_italic());
    assert_eq!(parse_one("\x1b[4mx").underline, Some(UnderlineType::Single));
    assert!(parse_one("\x1b[7mx").has_reverse());
    assert!(parse_one("\x1b[9mx").has_strikethrough());

    let s = parse_one("\x1b[1;2;3;4;7;9;22;23;24;27;29mx");
    assert!(!s.has_bold());
    assert!(!s.has_dim());
    assert!(!s.has_italic());
    assert_eq!(s.underline, None);
    assert!(!s.has_reverse());
    assert!(!s.has_strikethrough());
}

#[test]
fn ansi_16_color_fg_bg_including_bright() {
    assert_eq!(parse_one("\x1b[31mx").fg, Some(Color::Base256(1)));
    assert_eq!(parse_one("\x1b[47mx").bg, Some(Color::Base256(7)));
    assert_eq!(parse_one("\x1b[90mx").fg, Some(Color::Base256(8)));
    assert_eq!(parse_one("\x1b[107mx").bg, Some(Color::Base256(15)));
}

#[test]
fn ansi_extended_color_formats() {
    assert_eq!(parse_one("\x1b[38;5;200mx").fg, Some(Color::Base256(200)));
    assert_eq!(parse_one("\x1b[48;5;42mx").bg, Some(Color::Base256(42)));
    assert_eq!(parse_one("\x1b[38;2;10;20;30mx").fg, Some(Color::Rgb(10, 20, 30)));
    assert_eq!(parse_one("\x1b[48;2;255;128;0mx").bg, Some(Color::Rgb(255, 128, 0)));
}

#[test]
fn ansi_default_color_codes() {
    let mut p = AnsiStyleParser::new();
    p.parse_line("\x1b[31;41mtext");
    let out = p.parse_line("\x1b[39;49mnext");
    let last = out.spans.last().unwrap();
    assert_eq!(last.style.fg, None);
    assert_eq!(last.style.bg, None);
}

#[test]
fn ansi_parser_carries_state_across_lines() {
    let mut p = AnsiStyleParser::new();
    let _ = p.parse_line("\x1b[1mbold");
    let out = p.parse_line("still");
    assert!(out.spans.last().unwrap().style.has_bold());
    assert_eq!(out.text, "still");
}

#[test]
fn ansi_span_sizes_match_text_byte_lengths() {
    let mut p = AnsiStyleParser::new();
    let out = p.parse_line("\x1b[31mabc\x1b[32mde");
    let total: usize = out.spans.iter().map(|s| s.len).sum();
    assert_eq!(total, out.text.len());
}

#[test]
fn ansi_plain_text_has_no_spans() {
    let out = StyledString::from_ansi("plain");
    assert_eq!(out.text, "plain");
    assert!(out.spans.is_empty());
}

fn parse(s: &str) -> Style {
    s.parse::<Style>().unwrap()
}
fn parse_err(s: &str) {
    assert!(s.parse::<Style>().is_err(), "expected '{s}' to fail to parse");
}

#[test]
fn parse_all_attrs() {
    let s = parse("bold-italic-dim-strikethrough-reverse");
    assert!(s.has_bold());
    assert!(s.has_italic());
    assert!(s.has_dim());
    assert!(s.has_strikethrough());
    assert!(s.has_reverse());
}

#[test]
fn parse_bold_red_either_order() {
    let a = parse("bold-red");
    let b = parse("red-bold");
    assert_eq!(a, b);
    assert!(a.has_bold());
    assert_eq!(a.fg, Some(Color::RED));
}

#[test]
fn parse_blend_in_bg_piece() {
    let s = parse("red-on-50%-blue");
    assert_eq!(s.fg, Some(Color::RED));
    assert_eq!(s.bg, Some(Color::BLUE));
    assert_eq!(s.get_blend(), Some(50));
}

#[test]
fn parse_blend_outside_bg_piece_errors() {
    parse_err("50%-red");
    parse_err("red-50%");
}

#[test]
fn parse_bright_colour() {
    let s = parse("bright-red");
    assert_eq!(s.fg, Some(Color::BRIGHT_RED));
}

#[test]
fn parse_fg_bg_keywords_are_terminal_defaults() {
    let s = parse("bg-on-fg");
    assert_eq!(s.fg, Some(Color::Background));
    assert_eq!(s.bg, Some(Color::Foreground));
}

#[test]
fn parse_underline_without_colour() {
    let bare = parse("underline");
    assert_eq!(bare.underline, Some(UnderlineType::Single));
    assert_eq!(bare.underline_color, None);

    let with_line_style = parse("curly-underline");
    assert_eq!(with_line_style.underline, Some(UnderlineType::Curly));
    assert_eq!(with_line_style.underline_color, None);
}

#[test]
fn parse_line_style_with_colour() {
    let s = parse("single-red-underline");
    assert_eq!(s.underline, Some(UnderlineType::Single));
    assert_eq!(s.underline_color, Some(Color::RED));
}

#[test]
fn parse_full_three_piece() {
    let s = parse("red-on-blue-single-green-underline");
    assert_eq!(s.fg, Some(Color::RED));
    assert_eq!(s.bg, Some(Color::BLUE));
    assert_eq!(s.underline, Some(UnderlineType::Single));
    assert_eq!(s.underline_color, Some(Color::GREEN));
}

#[test]
fn parse_underline_then_fg_keeps_default_underline_colour() {
    let s = parse("underline-red");
    assert_eq!(s.underline, Some(UnderlineType::Single));
    assert_eq!(s.underline_color, None);
    assert_eq!(s.fg, Some(Color::RED));
}

#[test]
fn parse_two_fg_colours_last_wins() {
    let s = parse("red-blue");
    assert_eq!(s.fg, Some(Color::BLUE));
}

#[test]
fn parse_on_red_blue_closes_bg_after_colour() {
    let s = parse("on-red-blue");
    assert_eq!(s.bg, Some(Color::RED));
    assert_eq!(s.fg, Some(Color::BLUE));
}

#[test]
fn parse_separator_equivalence() {
    let canonical = parse("bold-red-on-blue");
    assert_eq!(parse("bold_red_on_blue"), canonical);
    assert_eq!(parse("bold red on blue"), canonical);
    assert_eq!(parse("bold_red on-blue"), canonical);
    assert_eq!(parse("  --bold--red--on--blue--  "), canonical);
}

#[test]
fn parse_empty_is_default_style() {
    assert_eq!(parse(""), Style::new());
    assert_eq!(parse("   "), Style::new());
    assert_eq!(parse("---"), Style::new());
}

#[test]
fn parse_attr_in_non_fg_piece_errors() {
    parse_err("on-bold-blue");
    parse_err("single-bold-underline");
}

#[test]
fn parse_line_style_not_closed_errors() {
    parse_err("single-red");
    parse_err("curly");
}

#[test]
fn parse_bg_not_closed_errors() {
    parse_err("red-on");
    parse_err("on-50%");
}

#[test]
fn parse_bad_blend_errors() {
    parse_err("on-200%-blue");
    parse_err("on-abc%-blue");
    parse_err("on-50%-30%-blue");
}

#[test]
fn parse_bright_without_colour_errors() {
    parse_err("bright");
}

#[test]
fn parse_extra_colour_in_underline_piece_errors() {
    parse_err("single-red-blue-underline");
}

#[test]
fn parse_unknown_token_errors() {
    parse_err("nonsense");
}

#[test]
fn parse_line_style_inside_underline_errors() {
    parse_err("single-double-underline");
}

#[test]
fn parse_no_writes_explicit_false() {
    let s = parse("bold-red-no-bold");
    assert_eq!(s.fg, Some(Color::RED));
    assert!(!s.has_bold());
    assert!(s.get_attrs_mask() & (StyleAttribute::Bold as u8) != 0);
    assert_eq!(parse("no-underline").underline, Some(UnderlineType::None));
}

#[test]
fn parse_no_unknown_target_errors() {
    parse_err("no-red");
    parse_err("no");
}

#[test]
fn parse_plain_writes_concrete_defaults() {
    let s = parse("plain-red");
    assert_eq!(s.fg, Some(Color::RED));
    assert_eq!(s.bg, Some(Color::Background));
    assert_eq!(s.underline, Some(UnderlineType::None));
    let all_attrs = (StyleAttribute::Bold as u8)
        | (StyleAttribute::Italic as u8)
        | (StyleAttribute::Dim as u8)
        | (StyleAttribute::Strikethrough as u8)
        | (StyleAttribute::Reverse as u8);
    assert_eq!(s.get_attrs_mask(), all_attrs);
    assert!(!s.has_bold());
}

#[test]
fn styled_string_push_str_default_style() {
    let mut s = StyledString::new();
    s.push_str("hi");
    assert_eq!(s.text, "hi");
    assert!(s.spans.is_empty());
}

#[test]
fn styled_string_push_span_default_does_not_allocate_spans() {
    let mut s = StyledString::new();
    s.push_span(StyledStr::new("hi"));
    assert_eq!(s.text, "hi");
    assert!(s.spans.is_empty());
}

#[test]
fn styled_string_push_span_styled_creates_spans() {
    let mut s = StyledString::new();
    s.push_span("hi".red());
    assert_eq!(s.text, "hi");
    let total: usize = s.spans.iter().map(|sp| sp.len).sum();
    assert_eq!(total, s.text.len() + 1);
    assert_eq!(s.spans[0].style.fg, Some(Color::RED));
}

#[test]
fn styled_string_push_span_merges_adjacent_equal_styles() {
    let mut s = StyledString::new();
    s.push_span("ab".red());
    s.push_span("cd".red());
    let red_runs: Vec<_> = s.spans.iter().filter(|sp| sp.style.fg == Some(Color::RED)).collect();
    assert_eq!(red_runs.len(), 1);
    assert_eq!(red_runs[0].len, 4);
}

#[test]
fn styled_string_style_range_applies_to_substring() {
    let mut s = StyledString::new();
    s.push_str("hello world");
    s.style_range(0..5, |st| st.set_bold(true));
    let total: usize = s.spans.iter().map(|sp| sp.len).sum();
    assert_eq!(total, s.text.len() + 1);
    let bold_len: usize = s
        .spans
        .iter()
        .filter(|sp| sp.style.has_bold())
        .map(|sp| sp.len)
        .sum();
    assert_eq!(bold_len, 5);
}

#[test]
fn styled_string_trim_left_drops_bytes_and_spans() {
    let mut s = StyledString::new();
    s.push_span("abc".red());
    s.push_span("def".blue());
    s.trim_left(3);
    assert_eq!(s.text, "def");
    let total: usize = s.spans.iter().map(|sp| sp.len).sum();
    assert_eq!(total, s.text.len() + 1);
    assert!(s.spans.iter().any(|sp| sp.style.fg == Some(Color::BLUE)));
    assert!(!s.spans.iter().any(|sp| sp.style.fg == Some(Color::RED)));
}

#[test]
fn styled_string_trim_left_zero_is_noop() {
    let mut s = StyledString::new();
    s.push_str("hello");
    s.trim_left(0);
    assert_eq!(s.text, "hello");
}

#[test]
fn styled_string_collapse_spans_merges_equal_neighbours() {
    let mut s = StyledString {
        text: "abcdef".to_string(),
        spans: vec![
            Span { style: Style::new().bold(), len: 2 },
            Span { style: Style::new().bold(), len: 2 },
            Span { style: Style::new().bold(), len: 2 },
            Span::new(1, Style::new()),
        ],
    };
    s.collapse_spans();
    let bold: Vec<_> = s.spans.iter().filter(|sp| sp.style.has_bold()).collect();
    assert_eq!(bold.len(), 1);
    assert_eq!(bold[0].len, 6);
}

#[test]
fn styled_string_collapse_drops_zero_sized_spans() {
    let mut s = StyledString {
        text: "ab".to_string(),
        spans: vec![
            Span { style: Style::new().bold(), len: 0 },
            Span { style: Style::new(), len: 2 },
            Span::new(1, Style::new()),
        ],
    };
    s.collapse_spans();
    assert!(s.spans.iter().all(|sp| sp.len > 0));
}

#[test]
fn styled_str_to_styled_string_default_skips_spans() {
    let ss: StyledString = StyledStr::new("plain").into();
    assert_eq!(ss.text, "plain");
    assert!(ss.spans.is_empty());
}

#[test]
fn styled_str_to_styled_string_styled_has_spans() {
    let ss: StyledString = "x".bold().into();
    assert_eq!(ss.text, "x");
    let total: usize = ss.spans.iter().map(|sp| sp.len).sum();
    assert_eq!(total, ss.text.len() + 1);
    assert!(ss.spans[0].style.has_bold());
}

