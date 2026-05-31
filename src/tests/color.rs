//! Integration tests for color parsing and representation.

use std::str::FromStr;
use tuie::render::color::{Color, ColorParseError};

#[test]
fn named_constants_match_base256_indices() {
    assert_eq!(Color::BLACK, Color::Base256(0));
    assert_eq!(Color::RED, Color::Base256(1));
    assert_eq!(Color::GREEN, Color::Base256(2));
    assert_eq!(Color::YELLOW, Color::Base256(3));
    assert_eq!(Color::BLUE, Color::Base256(4));
    assert_eq!(Color::MAGENTA, Color::Base256(5));
    assert_eq!(Color::CYAN, Color::Base256(6));
    assert_eq!(Color::WHITE, Color::Base256(7));
    assert_eq!(Color::BRIGHT_BLACK, Color::Base256(8));
    assert_eq!(Color::BRIGHT_RED, Color::Base256(9));
    assert_eq!(Color::BRIGHT_GREEN, Color::Base256(10));
    assert_eq!(Color::BRIGHT_YELLOW, Color::Base256(11));
    assert_eq!(Color::BRIGHT_BLUE, Color::Base256(12));
    assert_eq!(Color::BRIGHT_MAGENTA, Color::Base256(13));
    assert_eq!(Color::BRIGHT_CYAN, Color::Base256(14));
    assert_eq!(Color::BRIGHT_WHITE, Color::Base256(15));
}

#[test]
fn is_default_only_for_foreground_and_background() {
    assert!(Color::Foreground.is_default());
    assert!(Color::Background.is_default());
    assert!(!Color::RED.is_default());
    assert!(!Color::Rgb(1, 2, 3).is_default());
    assert!(!Color::Base256(200).is_default());
}

#[test]
fn color256_cube_indices() {
    assert_eq!(Color::color256(0, 0, 0), Color::Base256(16));
    assert_eq!(Color::color256(5, 5, 5), Color::Base256(231));
    assert_eq!(Color::color256(1, 2, 3), Color::Base256(16 + 36 + 12 + 3));
}

#[test]
fn grey256_ramp_indices() {
    assert_eq!(Color::grey256(0), Color::Base256(232));
    assert_eq!(Color::grey256(23), Color::Base256(255));
    assert_eq!(Color::grey256(7), Color::Base256(239));
}

#[test]
fn parse_six_digit_hex_no_prefix() {
    assert_eq!(Color::from_str("000000").ok(), Some(Color::Rgb(0, 0, 0)));
    assert_eq!(Color::from_str("ffffff").ok(), Some(Color::Rgb(255, 255, 255)));
    assert_eq!(Color::from_str("FFFFFF").ok(), Some(Color::Rgb(255, 255, 255)));
    assert_eq!(Color::from_str("123456").ok(), Some(Color::Rgb(0x12, 0x34, 0x56)));
}

#[test]
fn parse_six_digit_hex_with_prefixes() {
    assert_eq!(Color::from_str("#ff8000").ok(), Some(Color::Rgb(0xff, 0x80, 0x00)));
    assert_eq!(Color::from_str("0xff8000").ok(), Some(Color::Rgb(0xff, 0x80, 0x00)));
    assert_eq!(Color::from_str("0Xff8000").ok(), Some(Color::Rgb(0xff, 0x80, 0x00)));
}

#[test]
fn parse_three_digit_hex_requires_prefix_and_expands() {
    assert_eq!(Color::from_str("#abc").ok(), Some(Color::Rgb(0xaa, 0xbb, 0xcc)));
    assert_eq!(Color::from_str("0xabc").ok(), Some(Color::Rgb(0xaa, 0xbb, 0xcc)));
    assert_eq!(Color::from_str("0Xfff").ok(), Some(Color::Rgb(0xff, 0xff, 0xff)));
    assert_eq!(Color::from_str("#f0a").ok(), Some(Color::Rgb(0xff, 0x00, 0xaa)));
    assert_eq!(Color::from_str("abc").ok(), None);
}

#[test]
fn parse_two_digit_hex_with_prefix_is_base256() {
    assert_eq!(Color::from_str("#00").ok(), Some(Color::Base256(0)));
    assert_eq!(Color::from_str("#ff").ok(), Some(Color::Base256(255)));
    assert_eq!(Color::from_str("0x7f").ok(), Some(Color::Base256(0x7f)));
    assert_eq!(Color::from_str("ff").ok(), None);
}

#[test]
fn parse_trims_whitespace() {
    assert_eq!(Color::from_str("   #ff8000  ").ok(), Some(Color::Rgb(0xff, 0x80, 0x00)));
    assert_eq!(Color::from_str("\tffffff\n").ok(), Some(Color::Rgb(0xff, 0xff, 0xff)));
}

#[test]
fn parse_rejects_invalid_lengths_and_chars() {
    assert_eq!(Color::from_str("").ok(), None);
    assert_eq!(Color::from_str("#").ok(), None);
    assert_eq!(Color::from_str("#f").ok(), None);
    assert_eq!(Color::from_str("#fffff").ok(), None);
    assert_eq!(Color::from_str("#fffffff").ok(), None);
    assert_eq!(Color::from_str("#xyz").ok(), None);
    assert_eq!(Color::from_str("#gggggg").ok(), None);
    assert_eq!(Color::from_str("not-a-color").ok(), None);
}

#[test]
fn from_str_accepts_integer_as_base256() {
    let c: Color = "0".parse().unwrap();
    assert_eq!(c, Color::Base256(0));
    let c: Color = "255".parse().unwrap();
    assert_eq!(c, Color::Base256(255));
    let c: Color = "42".parse().unwrap();
    assert_eq!(c, Color::Base256(42));
}

#[test]
fn from_str_rejects_out_of_range_integer() {
    let r: Result<Color, ColorParseError> = "256".parse();
    assert!(r.is_err());
}

#[test]
fn from_str_accepts_hex_forms() {
    let c: Color = "#ff0000".parse().unwrap();
    assert_eq!(c, Color::Rgb(0xff, 0, 0));
    let c: Color = "0x00ff00".parse().unwrap();
    assert_eq!(c, Color::Rgb(0, 0xff, 0));
    let c: Color = "abcdef".parse().unwrap();
    assert_eq!(c, Color::Rgb(0xab, 0xcd, 0xef));
}

#[test]
fn from_str_named_colors() {
    let cases: &[(&str, Color)] = &[
        ("black", Color::BLACK),
        ("red", Color::RED),
        ("green", Color::GREEN),
        ("yellow", Color::YELLOW),
        ("blue", Color::BLUE),
        ("magenta", Color::MAGENTA),
        ("cyan", Color::CYAN),
        ("white", Color::WHITE),
        ("bright-black", Color::BRIGHT_BLACK),
        ("bright-red", Color::BRIGHT_RED),
        ("bright-green", Color::BRIGHT_GREEN),
        ("bright-yellow", Color::BRIGHT_YELLOW),
        ("bright-blue", Color::BRIGHT_BLUE),
        ("bright-magenta", Color::BRIGHT_MAGENTA),
        ("bright-cyan", Color::BRIGHT_CYAN),
        ("bright-white", Color::BRIGHT_WHITE),
        ("fg", Color::Foreground),
        ("foreground", Color::Foreground),
        ("bg", Color::Background),
        ("background", Color::Background),
    ];
    for (s, expected) in cases {
        let parsed: Color = s.parse().unwrap_or_else(|_| panic!("failed to parse {s:?}"));
        assert_eq!(parsed, *expected, "parse({s:?})");
    }
}

#[test]
fn from_str_named_colors_case_insensitive() {
    let r: Color = "RED".parse().unwrap();
    assert_eq!(r, Color::RED);
    let r: Color = "Red".parse().unwrap();
    assert_eq!(r, Color::RED);
    let r: Color = "rEd".parse().unwrap();
    assert_eq!(r, Color::RED);
}

#[test]
fn from_str_named_colors_accept_underscore_or_hyphen() {
    let a: Color = "bright_red".parse().unwrap();
    let b: Color = "bright-red".parse().unwrap();
    assert_eq!(a, Color::BRIGHT_RED);
    assert_eq!(b, Color::BRIGHT_RED);
    let a: Color = "BRIGHT_BLUE".parse().unwrap();
    assert_eq!(a, Color::BRIGHT_BLUE);
}

#[test]
fn from_str_trims_whitespace() {
    let c: Color = "   red  ".parse().unwrap();
    assert_eq!(c, Color::RED);
    let c: Color = "\t#000000\n".parse().unwrap();
    assert_eq!(c, Color::Rgb(0, 0, 0));
    let c: Color = "  42  ".parse().unwrap();
    assert_eq!(c, Color::Base256(42));
}

#[test]
fn from_str_rejects_unknown_names() {
    let r: Result<Color, ColorParseError> = "puce".parse();
    assert!(r.is_err());
    let r: Result<Color, ColorParseError> = "".parse();
    assert!(r.is_err());
    let r: Result<Color, ColorParseError> = "#zzz".parse();
    assert!(r.is_err());
}

#[test]
fn into_bits_tag_layout() {
    assert_eq!(Color::Foreground.into_bits(), 0);
    assert_eq!(Color::Base256(0).into_bits(), 1u32 << 24);
    assert_eq!(Color::Base256(0xab).into_bits(), (1u32 << 24) | 0xab);
    assert_eq!(
        Color::Rgb(0x12, 0x34, 0x56).into_bits(),
        (2u32 << 24) | (0x12 << 16) | (0x34 << 8) | 0x56,
    );
    assert_eq!(Color::Background.into_bits(), 3u32 << 24);
}

#[test]
fn round_trip_bits_for_every_variant() {
    let samples = [
        Color::Foreground,
        Color::Background,
        Color::Base256(0),
        Color::Base256(1),
        Color::Base256(127),
        Color::Base256(255),
        Color::Rgb(0, 0, 0),
        Color::Rgb(255, 255, 255),
        Color::Rgb(0x12, 0x34, 0x56),
        Color::Rgb(1, 2, 3),
    ];
    for c in samples {
        let bits = c.into_bits();
        assert_eq!(Color::from_bits(bits), c, "round trip for {c}");
    }
}

#[test]
fn from_bits_ignores_high_bits_above_26() {
    let base = Color::Rgb(0x12, 0x34, 0x56).into_bits();
    let polluted = base | 0xFC00_0000;
    assert_eq!(Color::from_bits(polluted), Color::Rgb(0x12, 0x34, 0x56));
}

#[test]
fn display_format_matches_variant_shape() {
    assert_eq!(format!("{}", Color::Foreground), "Foreground");
    assert_eq!(format!("{}", Color::Background), "Background");
    assert_eq!(format!("{}", Color::RED), "Base256(1)");
    assert_eq!(format!("{}", Color::Base256(200)), "Base256(200)");
    assert_eq!(format!("{}", Color::Rgb(1, 2, 3)), "Rgb(1, 2, 3)");
}

#[test]
fn parse_boundary_values() {
    assert_eq!(Color::from_str("#000000").ok(), Some(Color::Rgb(0, 0, 0)));
    assert_eq!(Color::from_str("#ffffff").ok(), Some(Color::Rgb(255, 255, 255)));
    assert_eq!(Color::from_str("#FFFFFF").ok(), Some(Color::Rgb(255, 255, 255)));
    assert_eq!(Color::from_str("#000").ok(), Some(Color::Rgb(0, 0, 0)));
    assert_eq!(Color::from_str("#fff").ok(), Some(Color::Rgb(255, 255, 255)));
    assert_eq!(Color::from_str("#00").ok(), Some(Color::Base256(0)));
    assert_eq!(Color::from_str("#ff").ok(), Some(Color::Base256(255)));
}

