//! Integration tests for the ANSI input parser.

use tuie::ansi::input::Parser;
use tuie::ansi::{ColorEntry, ColorScheme, ColorType, ParsedEvent, MouseInput};
use tuie::prelude::*;


/// Feeds `bytes` to a fresh [`Parser`], flushes any pending ESC, and returns all emitted events.
fn parse_all(bytes: &[u8]) -> Vec<ParsedEvent> {
    let mut p = Parser::new();
    p.feed_all(bytes);
    p.flush_escape();
    let mut out = Vec::new();
    while let Some(e) = p.next() {
        out.push(e);
    }
    out
}

/// Feeds `bytes` and asserts exactly one [`ParsedEvent`] is produced, returning it.
fn parse_one(bytes: &[u8]) -> ParsedEvent {
    let events = parse_all(bytes);
    assert_eq!(events.len(), 1, "expected one event, got {events:?}");
    events.into_iter().next().unwrap()
}

fn key(k: Key, modifiers: Modifiers) -> ParsedEvent {
    ParsedEvent::Key(Chord { trigger: Trigger::Key(k), modifiers })
}

fn no_mods() -> Modifiers {
    Modifiers::new()
}

#[test]
fn plain_chars() {
    assert_eq!(parse_one(b"a"), key(Key::Char('a'), no_mods()));
    assert_eq!(parse_one(b"A"), key(Key::Char('A'), no_mods()));
}

#[test]
fn control_bytes() {
    assert_eq!(parse_one(b"\r"), key(Key::Enter, no_mods()));
    assert_eq!(parse_one(b"\t"), key(Key::Tab, no_mods()));
    assert_eq!(parse_one(b"\x7F"), key(Key::Backspace, no_mods()));
    assert_eq!(
        parse_one(b"\x01"),
        key(Key::Char('a'), no_mods().with(Modifier::Ctrl))
    );
    assert_eq!(
        parse_one(b"\x00"),
        key(Key::Char(' '), no_mods().with(Modifier::Ctrl))
    );
}

#[test]
fn lone_esc_flushes_to_escape() {
    assert_eq!(parse_one(b"\x1B"), key(Key::Esc, no_mods()));
}

#[test]
fn lone_esc_then_flush_explicit() {
    let mut p = Parser::new();
    p.feed(0x1B);
    assert!(p.next().is_none());
    p.flush_escape();
    assert_eq!(p.next(), Some(key(Key::Esc, no_mods())));
}

#[test]
fn alt_char() {
    assert_eq!(
        parse_one(b"\x1Bc"),
        key(Key::Char('c'), no_mods().with(Modifier::Alt))
    );
}

#[test]
fn alt_ctrl_char() {
    assert_eq!(
        parse_one(b"\x1B\x14"),
        key(
            Key::Char('t'),
            no_mods().with(Modifier::Alt).with(Modifier::Ctrl)
        )
    );
}

#[test]
fn double_esc() {
    assert_eq!(parse_one(b"\x1B\x1B"), key(Key::Esc, no_mods()));
}

#[test]
fn alt_chars_that_double_as_introducers() {
    let cases = [
        (b"\x1BP".as_slice(), 'P'),
        (b"\x1BO", 'O'),
        (b"\x1B[", '['),
        (b"\x1B]", ']'),
        (b"\x1B_", '_'),
    ];
    for (bytes, c) in cases {
        assert_eq!(
            parse_one(bytes),
            key(Key::Char(c), no_mods().with(Modifier::Alt)),
            "for M-{c}"
        );
    }
}

#[test]
fn ss3_arrows_and_fkeys() {
    assert_eq!(parse_one(b"\x1BOA"), key(Key::Arrow(Direction2D::Up), no_mods()));
    assert_eq!(parse_one(b"\x1BOP"), key(Key::F(1), no_mods()));
    assert_eq!(parse_one(b"\x1BOS"), key(Key::F(4), no_mods()));
}

#[test]
fn csi_arrows() {
    assert_eq!(parse_one(b"\x1B[A"), key(Key::Arrow(Direction2D::Up), no_mods()));
    assert_eq!(parse_one(b"\x1B[D"), key(Key::Arrow(Direction2D::Left), no_mods()));
    assert_eq!(parse_one(b"\x1B[H"), key(Key::Home, no_mods()));
}

#[test]
fn csi_backtab() {
    assert_eq!(parse_one(b"\x1B[Z"), key(Key::Tab, no_mods().with(Modifier::Shift)));
}

#[test]
fn csi_modified_arrow() {
    assert_eq!(
        parse_one(b"\x1B[1;2D"),
        key(Key::Arrow(Direction2D::Left), no_mods().with(Modifier::Shift))
    );
    assert_eq!(
        parse_one(b"\x1B[1;5A"),
        key(Key::Arrow(Direction2D::Up), no_mods().with(Modifier::Ctrl))
    );
}

#[test]
fn csi_special_keys() {
    assert_eq!(parse_one(b"\x1B[3~"), key(Key::Delete, no_mods()));
    assert_eq!(parse_one(b"\x1B[5~"), key(Key::PageUp, no_mods()));
    assert_eq!(parse_one(b"\x1B[2~"), key(Key::Insert, no_mods()));
    assert_eq!(parse_one(b"\x1B[15~"), key(Key::F(5), no_mods()));
    assert_eq!(
        parse_one(b"\x1B[3;2~"),
        key(Key::Delete, no_mods().with(Modifier::Shift))
    );
}

#[test]
fn csi_u_basic() {
    assert_eq!(parse_one(b"\x1B[97u"), key(Key::Char('a'), no_mods()));
    assert_eq!(
        parse_one(b"\x1B[97;5u"),
        key(Key::Char('a'), no_mods().with(Modifier::Ctrl))
    );
    assert_eq!(parse_one(b"\x1B[27u"), key(Key::Esc, no_mods()));
    assert_eq!(parse_one(b"\x1B[13u"), key(Key::Enter, no_mods()));
}

#[test]
fn csi_u_shifted_alternate() {
    assert_eq!(
        parse_one(b"\x1B[57:40;4u"),
        key(Key::Char('('), no_mods().with(Modifier::Alt))
    );
}

#[test]
fn csi_u_unsupported_dropped() {
    assert!(parse_all(b"\x1B[57441u").is_empty());
}

#[test]
fn csi_tilde_fkeys_full_range() {
    let cases = [
        (b"\x1B[15~".as_slice(), 5u8),
        (b"\x1B[17~", 6),
        (b"\x1B[18~", 7),
        (b"\x1B[19~", 8),
        (b"\x1B[20~", 9),
        (b"\x1B[21~", 10),
        (b"\x1B[23~", 11),
        (b"\x1B[24~", 12),
        (b"\x1B[25~", 13),
        (b"\x1B[26~", 14),
        (b"\x1B[28~", 15),
        (b"\x1B[29~", 16),
        (b"\x1B[31~", 17),
        (b"\x1B[32~", 18),
        (b"\x1B[33~", 19),
        (b"\x1B[34~", 20),
    ];
    for (bytes, n) in cases {
        assert_eq!(parse_one(bytes), key(Key::F(n), no_mods()), "param for F{n}");
    }
}

#[test]
fn trailing_esc_flushes_after_queued_event() {
    let mut p = Parser::new();
    p.feed(b'a');
    p.feed(0x1B);
    p.flush_escape();
    assert_eq!(p.next(), Some(key(Key::Char('a'), no_mods())));
    assert_eq!(p.next(), Some(key(Key::Esc, no_mods())));
    assert!(p.next().is_none());
}

#[test]
fn focus_events() {
    assert_eq!(parse_one(b"\x1B[I"), ParsedEvent::Focus(true));
    assert_eq!(parse_one(b"\x1B[O"), ParsedEvent::Focus(false));
}

#[test]
fn sgr_mouse_press_and_release() {
    assert_eq!(
        parse_one(b"\x1B[<0;20;10M"),
        ParsedEvent::Mouse(MouseInput {
            trigger: Trigger::MouseDown(MouseButton::Left),
            column: 19,
            row: 9,
            modifiers: no_mods(),
        })
    );
    assert_eq!(
        parse_one(b"\x1B[<0;20;10m"),
        ParsedEvent::Mouse(MouseInput {
            trigger: Trigger::MouseUp(MouseButton::Left),
            column: 19,
            row: 9,
            modifiers: no_mods(),
        })
    );
}

#[test]
fn sgr_mouse_scroll_and_drag() {
    assert_eq!(
        parse_one(b"\x1B[<64;5;5M"),
        ParsedEvent::Mouse(MouseInput {
            trigger: Trigger::MouseScroll(Direction2D::Up),
            column: 4,
            row: 4,
            modifiers: no_mods(),
        })
    );
    assert_eq!(
        parse_one(b"\x1B[<32;5;5M"),
        ParsedEvent::Mouse(MouseInput {
            trigger: Trigger::MouseDrag(MouseButton::Left),
            column: 4,
            row: 4,
            modifiers: no_mods(),
        })
    );
}

#[test]
fn normal_mouse() {
    assert_eq!(
        parse_one(b"\x1B[M \x21\x21"),
        ParsedEvent::Mouse(MouseInput {
            trigger: Trigger::MouseDown(MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: no_mods(),
        })
    );
}

#[test]
fn bracketed_paste() {
    assert_eq!(
        parse_one(b"\x1B[200~hello world\x1B[201~"),
        ParsedEvent::Paste("hello world".to_string())
    );
    assert_eq!(
        parse_one(b"\x1B[200~a\x1B[2Db\x1B[201~"),
        ParsedEvent::Paste("a\x1B[2Db".to_string())
    );
}

#[test]
fn utf8_multibyte() {
    assert_eq!(parse_one("ñ".as_bytes()), key(Key::Char('ñ'), no_mods()));
    assert_eq!(parse_one("𐌼".as_bytes()), key(Key::Char('𐌼'), no_mods()));
}

#[test]
fn split_across_feeds() {
    let mut p = Parser::new();
    for &b in b"\x1B[1;5A" {
        p.feed(b);
    }
    assert_eq!(
        p.next(),
        Some(key(Key::Arrow(Direction2D::Up), no_mods().with(Modifier::Ctrl)))
    );
    assert!(p.next().is_none());
}

#[test]
fn da1_reply() {
    assert_eq!(
        parse_one(b"\x1B[?64;1;2c"),
        ParsedEvent::PrimaryDeviceAttributes(vec![64, 1, 2])
    );
    assert_eq!(
        parse_one(b"\x1B[?c"),
        ParsedEvent::PrimaryDeviceAttributes(vec![])
    );
}

#[test]
fn xtversion_reply() {
    assert_eq!(
        parse_one(b"\x1BP>|kitty 0.36.4\x1B\\"),
        ParsedEvent::XtVersion("kitty 0.36.4".to_string())
    );
}

#[test]
fn dcs_reply_split_across_feeds() {
    let mut p = Parser::new();
    p.feed_all(b"\x1BP>|kit");
    p.flush_escape();
    p.feed_all(b"ty 0.36.4\x1B\\");
    assert_eq!(
        p.next(),
        Some(ParsedEvent::XtVersion("kitty 0.36.4".to_string()))
    );
    assert!(p.next().is_none());
}

#[test]
fn kitty_graphics_reply() {
    assert_eq!(
        parse_one(b"\x1B_Gi=31;OK\x1B\\"),
        ParsedEvent::KittyGraphicsReply { id: 31, ok: true }
    );
}

#[test]
fn cell_pixel_size_reply() {
    assert_eq!(
        parse_one(b"\x1B[6;20;10t"),
        ParsedEvent::CellPixelSize { width: 10, height: 20 }
    );
}

#[test]
fn decrpm_reply() {
    assert_eq!(
        parse_one(b"\x1B[?1016;1$y"),
        ParsedEvent::DecModeReport { mode: 1016, status: 1 }
    );
    assert_eq!(
        parse_one(b"\x1B[?1016;0$y"),
        ParsedEvent::DecModeReport { mode: 1016, status: 0 }
    );
}

#[test]
fn color_scheme_reply() {
    assert_eq!(parse_one(b"\x1B[?997;1n"), ParsedEvent::ColorScheme(ColorScheme::Dark));
    assert_eq!(parse_one(b"\x1B[?997;2n"), ParsedEvent::ColorScheme(ColorScheme::Light));
}

#[test]
fn osc_color_reply() {
    assert_eq!(
        parse_one(b"\x1B]11;rgb:ab/cd/ef\x1B\\"),
        ParsedEvent::Color(ColorEntry {
            color_type: ColorType::Background,
            r: 0xab,
            g: 0xcd,
            b: 0xef,
        })
    );
    assert_eq!(
        parse_one(b"\x1B]4;1;rgb:ffff/0000/0000\x07"),
        ParsedEvent::Color(ColorEntry {
            color_type: ColorType::Palette(1),
            r: 0xff,
            g: 0x00,
            b: 0x00,
        })
    );
}
