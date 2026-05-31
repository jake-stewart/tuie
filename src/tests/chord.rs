//! Integration tests for chord parsing and the `chord!` proc macro.

use chord_macro::chord;
use tuie::input::chord::parse_chords;
use tuie::prelude::*;

fn key(k: Key) -> Chord {
    Chord::new(Trigger::Key(k), Modifiers::new())
}

fn key_mods(k: Key, m: Modifiers) -> Chord {
    Chord::new(Trigger::Key(k), m)
}

#[test]
fn parser_single_literal_char() {
    let chords = parse_chords("a");
    assert_eq!(chords, vec![key(Key::Char('a'))]);
}

#[test]
fn parser_uppercase_literal_is_char() {
    let chords = parse_chords("A");
    assert_eq!(chords, vec![key(Key::Char('A'))]);
}

#[test]
fn parser_digits_and_punctuation() {
    let chords = parse_chords("0?");
    assert_eq!(chords, vec![key(Key::Char('0')), key(Key::Char('?'))]);
}

#[test]
fn parser_ctrl_a() {
    let chords = parse_chords("<C-a>");
    assert_eq!(chords, vec![key_mods(Key::Char('a'), Modifiers::new().with(Modifier::Ctrl))]);
}

#[test]
fn parser_modifier_letters_case_insensitive() {
    assert_eq!(parse_chords("<c-x>"), parse_chords("<C-x>"));
    assert_eq!(parse_chords("<a-x>"), parse_chords("<A-x>"));
    assert_eq!(parse_chords("<s-Up>"), parse_chords("<S-Up>"));
    assert_eq!(parse_chords("<m-x>"), parse_chords("<M-x>"));
    assert_eq!(parse_chords("<d-x>"), parse_chords("<D-x>"));

    let c = &parse_chords("<C-x>")[0];
    assert!(c.modifiers.has(Modifier::Ctrl));
    let a = &parse_chords("<A-x>")[0];
    assert!(a.modifiers.has(Modifier::Alt));
    let s = &parse_chords("<S-Up>")[0];
    assert!(s.modifiers.has(Modifier::Shift));
    let m = &parse_chords("<M-x>")[0];
    assert!(m.modifiers.has(Modifier::Meta));
    let d = &parse_chords("<D-x>")[0];
    assert!(d.modifiers.has(Modifier::Super));
}

#[test]
fn parser_shift_up() {
    let chords = parse_chords("<S-Up>");
    assert_eq!(
        chords,
        vec![key_mods(Key::Arrow(Direction2D::Up), Modifiers::new().with(Modifier::Shift))],
    );
}

#[test]
fn parser_combined_modifiers() {
    let cs_a = &parse_chords("<C-S-a>")[0];
    assert!(cs_a.modifiers.has(Modifier::Ctrl));
    assert!(cs_a.modifiers.has(Modifier::Shift));
    assert_eq!(cs_a.trigger, Trigger::Key(Key::Char('a')));

    let ca_del = &parse_chords("<C-A-Del>")[0];
    assert!(ca_del.modifiers.has(Modifier::Ctrl));
    assert!(ca_del.modifiers.has(Modifier::Alt));
    assert_eq!(ca_del.trigger, Trigger::Key(Key::Delete));
}

#[test]
fn parser_named_keys() {
    assert_eq!(parse_chords("<Enter>"), vec![key(Key::Enter)]);
    assert_eq!(parse_chords("<Esc>"), vec![key(Key::Esc)]);
    assert_eq!(parse_chords("<Tab>"), vec![key(Key::Tab)]);
    assert_eq!(parse_chords("<Space>"), vec![key(Key::Char(' '))]);
    assert_eq!(parse_chords("<Backspace>"), vec![key(Key::Backspace)]);
    assert_eq!(parse_chords("<Delete>"), vec![key(Key::Delete)]);
    assert_eq!(parse_chords("<Insert>"), vec![key(Key::Insert)]);
    assert_eq!(parse_chords("<Home>"), vec![key(Key::Home)]);
    assert_eq!(parse_chords("<End>"), vec![key(Key::End)]);
    assert_eq!(parse_chords("<PageUp>"), vec![key(Key::PageUp)]);
    assert_eq!(parse_chords("<PageDown>"), vec![key(Key::PageDown)]);
}

#[test]
fn parser_arrow_keys() {
    assert_eq!(parse_chords("<Up>"), vec![key(Key::Arrow(Direction2D::Up))]);
    assert_eq!(parse_chords("<Down>"), vec![key(Key::Arrow(Direction2D::Down))]);
    assert_eq!(parse_chords("<Left>"), vec![key(Key::Arrow(Direction2D::Left))]);
    assert_eq!(parse_chords("<Right>"), vec![key(Key::Arrow(Direction2D::Right))]);
}

#[test]
fn parser_named_keys_case_insensitive() {
    assert_eq!(parse_chords("<enter>"), parse_chords("<Enter>"));
    assert_eq!(parse_chords("<ESC>"), parse_chords("<Esc>"));
    assert_eq!(parse_chords("<pageup>"), parse_chords("<PageUp>"));
}

#[test]
fn parser_aliased_named_keys() {
    assert_eq!(parse_chords("<CR>"), vec![key(Key::Enter)]);
    assert_eq!(parse_chords("<BS>"), vec![key(Key::Backspace)]);
    assert_eq!(parse_chords("<Del>"), vec![key(Key::Delete)]);
    assert_eq!(parse_chords("<lt>"), vec![key(Key::Char('<'))]);
    assert_eq!(parse_chords("<gt>"), vec![key(Key::Char('>'))]);
}

#[test]
fn parser_function_keys_f1_through_f12() {
    for n in 1..=12u8 {
        let chords = parse_chords(&format!("<F{}>", n));
        assert_eq!(chords, vec![key(Key::F(n))], "F{} should parse", n);
    }
}

#[test]
fn parser_function_keys_cap_at_f12() {
    let chords = parse_chords("<F13>");
    assert_eq!(
        chords,
        vec![
            key(Key::Char('<')),
            key(Key::Char('F')),
            key(Key::Char('1')),
            key(Key::Char('3')),
            key(Key::Char('>')),
        ],
    );
}

#[test]
fn parser_sequence_two_chords() {
    let chords = parse_chords("<C-x><C-c>");
    assert_eq!(chords.len(), 2);
    assert_eq!(chords[0], key_mods(Key::Char('x'), Modifiers::new().with(Modifier::Ctrl)));
    assert_eq!(chords[1], key_mods(Key::Char('c'), Modifiers::new().with(Modifier::Ctrl)));
}

#[test]
fn parser_sequence_mixed_literal_and_spec() {
    let chords = parse_chords("a<Enter>b");
    assert_eq!(
        chords,
        vec![key(Key::Char('a')), key(Key::Enter), key(Key::Char('b'))],
    );
}

#[test]
fn parser_unclosed_bracket_is_literal() {
    let chords = parse_chords("<unclosed");
    assert_eq!(chords.len(), "<unclosed".len());
    assert_eq!(chords[0], key(Key::Char('<')));
    assert_eq!(chords[1], key(Key::Char('u')));
}

#[test]
fn parser_unknown_spec_is_literal() {
    let chords = parse_chords("<bogus>");
    assert_eq!(
        chords,
        vec![
            key(Key::Char('<')),
            key(Key::Char('b')),
            key(Key::Char('o')),
            key(Key::Char('g')),
            key(Key::Char('u')),
            key(Key::Char('s')),
            key(Key::Char('>')),
        ],
    );
}

#[test]
fn parser_empty_spec_is_literal() {
    let chords = parse_chords("<>");
    assert_eq!(chords, vec![key(Key::Char('<')), key(Key::Char('>'))]);
}

#[test]
fn macro_matches_parser_for_ctrl_a() {
    let from_macro = chord!(Ctrl + a);
    let from_parser = parse_chords("<C-a>").pop().unwrap();
    assert_eq!(from_macro, from_parser);
}

#[test]
fn macro_matches_parser_for_named_keys() {
    assert_eq!(chord!(Enter), parse_chords("<Enter>").pop().unwrap());
    assert_eq!(chord!(Esc), parse_chords("<Esc>").pop().unwrap());
    assert_eq!(chord!(Tab), parse_chords("<Tab>").pop().unwrap());
    assert_eq!(chord!(Space), parse_chords("<Space>").pop().unwrap());
    assert_eq!(chord!(Up), parse_chords("<Up>").pop().unwrap());
    assert_eq!(chord!(PageDown), parse_chords("<PageDown>").pop().unwrap());
}

#[test]
fn macro_function_keys() {
    assert_eq!(chord!(F1), parse_chords("<F1>").pop().unwrap());
    assert_eq!(chord!(F12), parse_chords("<F12>").pop().unwrap());
    assert_eq!(chord!(F1).trigger, Trigger::Key(Key::F(1)));
    assert_eq!(chord!(F12).trigger, Trigger::Key(Key::F(12)));
}

#[test]
fn macro_combined_modifiers() {
    let m = chord!(Ctrl + Alt + Delete);
    let p = parse_chords("<C-A-Del>").pop().unwrap();
    assert_eq!(m, p);
}

#[test]
fn macro_mouse_chords() {
    assert_eq!(
        chord!(LeftClick),
        Chord::new(Trigger::MouseDown(MouseButton::Left), Modifiers::new()),
    );
    assert_eq!(
        chord!(RightClick),
        Chord::new(Trigger::MouseDown(MouseButton::Right), Modifiers::new()),
    );
    assert_eq!(
        chord!(LeftDrag),
        Chord::new(Trigger::MouseDrag(MouseButton::Left), Modifiers::new()),
    );
    assert_eq!(
        chord!(LeftRelease),
        Chord::new(Trigger::MouseUp(MouseButton::Left), Modifiers::new()),
    );
}

#[test]
fn parser_does_not_recognise_mouse() {
    let chords = parse_chords("<LeftClick>");
    assert_eq!(chords[0], key(Key::Char('<')));
    assert!(chords.iter().all(|c| matches!(c.trigger, Trigger::Key(Key::Char(_)))));
}

#[test]
fn divergence_shift_with_char_parser_allows_macro_forbids() {
    let p = parse_chords("<S-a>").pop().unwrap();
    assert!(p.modifiers.has(Modifier::Shift));
    assert_eq!(p.trigger, Trigger::Key(Key::Char('a')));

    let m = chord!(A);
    assert_eq!(m.trigger, Trigger::Key(Key::Char('A')));
    assert!(!m.modifiers.has(Modifier::Shift));

    assert_ne!(p, m);
}

#[test]
fn divergence_macro_has_no_meta_or_hyper() {
    let p = parse_chords("<M-a>").pop().unwrap();
    assert!(p.modifiers.has(Modifier::Meta));
}
