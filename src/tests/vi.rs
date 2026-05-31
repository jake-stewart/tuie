//! Integration tests for the vi mode bindings.

use chord_macro::chord;
use tuie::prelude::*;
use tuie::emulator::Emulator;

type Vi = ViBindings<Text>;

fn make(content: &str) -> Box<Input> {
    tuie::clipboard::clear();
    Input::new().multiline().bindings(ViBindings::new).content(content)
}

fn mode(input: &Input) -> ViMode {
    let (vi, _) = input.get_bindings_as::<Vi>().expect("vi bindings");
    vi.get_mode()
}

fn cursor(input: &Input) -> usize {
    input.get_editor().get_cursor().get_index()
}

fn send(term: &mut Emulator, input: &mut Input, chords: &[Chord]) {
    let events: Vec<RuntimeEvent> = chords.iter().cloned().map(RuntimeEvent::from).collect();
    term.update(&mut *input, &events);
}

fn esc(term: &mut Emulator, input: &mut Input) {
    send(term, input, &[chord!(Esc)]);
}

#[test]
fn starts_in_insert_mode() {
    let mut input = make("");
    let _term = Emulator::new(&mut *input, Vec2::new(20, 3));
    assert_eq!(mode(&input), ViMode::Insert);
}

#[test]
fn esc_enters_normal_mode() {
    let mut input = make("");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    assert_eq!(mode(&input), ViMode::Normal);
}

#[test]
fn i_enters_insert_from_normal() {
    let mut input = make("abc");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    assert_eq!(mode(&input), ViMode::Normal);
    send(&mut term, &mut input, &[chord!(i)]);
    assert_eq!(mode(&input), ViMode::Insert);
}

#[test]
fn a_appends_after_cursor() {
    let mut input = make("ab");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(a), chord!(X)]);
    assert_eq!(input.get_string(), "aXb");
    assert_eq!(mode(&input), ViMode::Insert);
}

#[test]
fn capital_i_inserts_at_line_start() {
    let mut input = make("  abc");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(l), chord!(l), chord!(l)]);
    send(&mut term, &mut input, &[chord!(I), chord!(X)]);
    assert_eq!(input.get_string(), "  Xabc");
}

#[test]
fn capital_a_appends_at_line_end() {
    let mut input = make("abc");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(A), chord!(X)]);
    assert_eq!(input.get_string(), "abcX");
}

#[test]
fn o_opens_line_below() {
    let mut input = make("abc\ndef");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(o), chord!(X)]);
    assert_eq!(input.get_string(), "abc\nX\ndef");
}

#[test]
fn capital_o_opens_line_above() {
    let mut input = make("abc\ndef");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(O), chord!(X)]);
    assert_eq!(input.get_string(), "X\nabc\ndef");
}

#[test]
fn h_l_move_cursor_horizontally() {
    let mut input = make("abc");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    assert_eq!(cursor(&input), 0);
    send(&mut term, &mut input, &[chord!(l), chord!(l)]);
    assert_eq!(cursor(&input), 2);
    send(&mut term, &mut input, &[chord!(h)]);
    assert_eq!(cursor(&input), 1);
}

#[test]
fn j_k_move_cursor_vertically() {
    let mut input = make("aaa\nbbb\nccc");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 5));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(j)]);
    assert_eq!(cursor(&input), 4);
    send(&mut term, &mut input, &[chord!(j)]);
    assert_eq!(cursor(&input), 8);
    send(&mut term, &mut input, &[chord!(k)]);
    assert_eq!(cursor(&input), 4);
}

#[test]
fn w_b_e_word_motions() {
    let mut input = make("foo bar baz");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(w)]);
    assert_eq!(cursor(&input), 4);
    send(&mut term, &mut input, &[chord!(w)]);
    assert_eq!(cursor(&input), 8);
    send(&mut term, &mut input, &[chord!(b)]);
    assert_eq!(cursor(&input), 4);
    send(&mut term, &mut input, &[chord!(e)]);
    assert_eq!(cursor(&input), 6);
}

#[test]
fn zero_and_dollar_jump_line_ends() {
    let mut input = make("hello world");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(l), chord!(l), chord!(l)]);
    assert_eq!(cursor(&input), 3);
    send(&mut term, &mut input, &[chord!(0)]);
    assert_eq!(cursor(&input), 0);
    send(&mut term, &mut input, &[chord!('$')]);
    assert_eq!(cursor(&input), 10);
}

#[test]
fn gg_and_capital_g_jump_document() {
    let mut input = make("line1\nline2\nline3");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 5));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(G)]);
    let after_g = cursor(&input);
    assert!(after_g >= 12, "G should land on line3, got {}", after_g);
    send(&mut term, &mut input, &[chord!(g), chord!(g)]);
    assert_eq!(cursor(&input), 0);
}

#[test]
fn count_prefix_repeats_motion() {
    let mut input = make("aaaaaaaa");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(3), chord!(l)]);
    assert_eq!(cursor(&input), 3);
}

#[test]
fn count_prefix_repeats_word_motion() {
    let mut input = make("a b c d e");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(3), chord!(w)]);
    assert_eq!(cursor(&input), 6);
}

#[test]
fn count_prefix_repeats_line_motion() {
    let mut input = make("1\n2\n3\n4\n5\n6");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 10));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(3), chord!(j)]);
    assert_eq!(cursor(&input), 6);
}

#[test]
fn dw_deletes_word() {
    let mut input = make("foo bar baz");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(d), chord!(w)]);
    assert_eq!(input.get_string(), "bar baz");
}

#[test]
fn d_dollar_deletes_to_eol() {
    let mut input = make("hello world");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(l), chord!(l), chord!(l), chord!(l), chord!(l)]);
    send(&mut term, &mut input, &[chord!(d), chord!('$')]);
    assert_eq!(input.get_string(), "hello");
}

#[test]
fn dd_deletes_entire_line() {
    let mut input = make("aaa\nbbb\nccc");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 5));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(j)]);
    send(&mut term, &mut input, &[chord!(d), chord!(d)]);
    assert_eq!(input.get_string(), "aaa\nccc");
}

#[test]
fn cw_changes_word_and_enters_insert() {
    let mut input = make("foo bar");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(c), chord!(w)]);
    assert_eq!(mode(&input), ViMode::Insert);
    send(&mut term, &mut input, &[chord!(X), chord!(Y)]);
    assert_eq!(input.get_string(), "XY bar");
}

#[test]
fn ci_paren_changes_inside_parens() {
    let mut input = make("(hello)");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(c), chord!(i), chord!('(')]);
    assert_eq!(mode(&input), ViMode::Insert);
    send(&mut term, &mut input, &[chord!(Z)]);
    assert_eq!(input.get_string(), "(Z)");
}

#[test]
fn ci_quote_changes_inside_quotes() {
    let mut input = make("\"hi\"");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(c), chord!(i), chord!('"')]);
    assert_eq!(mode(&input), ViMode::Insert);
    send(&mut term, &mut input, &[chord!(Q)]);
    assert_eq!(input.get_string(), "\"Q\"");
}

#[test]
fn yi_paren_yanks_inside_parens() {
    let mut input = make("(hello)");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(y), chord!(i), chord!('(')]);
    assert_eq!(input.get_string(), "(hello)");
    assert_eq!(tuie::clipboard::read_string().as_deref(), Some("hello"));
}

#[test]
fn yy_yanks_entire_line() {
    let mut input = make("aaa\nbbb");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(y), chord!(y)]);
    let yanked = tuie::clipboard::read_string().expect("yy yanks");
    assert!(yanked.contains("aaa"), "yy clipboard: {:?}", yanked);
}

#[test]
fn p_pastes_after_cursor() {
    let mut input = make("abc");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    tuie::clipboard::write(ClipboardItem::Text("XY".to_string()));
    send(&mut term, &mut input, &[chord!(p)]);
    assert_eq!(input.get_string(), "aXYbc");
}

#[test]
fn capital_p_pastes_before_cursor() {
    let mut input = make("abc");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    tuie::clipboard::write(ClipboardItem::Text("XY".to_string()));
    send(&mut term, &mut input, &[chord!(P)]);
    assert_eq!(input.get_string(), "XYabc");
}

#[test]
fn yank_then_paste_roundtrip() {
    let mut input = make("hello world");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(y), chord!(w)]);
    send(&mut term, &mut input, &[chord!('$')]);
    send(&mut term, &mut input, &[chord!(p)]);
    assert_eq!(input.get_string(), "hello worldhello ");
}

#[test]
fn v_enters_visual() {
    let mut input = make("abc");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(v)]);
    assert_eq!(mode(&input), ViMode::Visual);
}

#[test]
fn capital_v_enters_visual_line() {
    let mut input = make("abc\ndef");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(V)]);
    assert_eq!(mode(&input), ViMode::VisualLine);
}

#[test]
fn esc_exits_visual_to_normal() {
    let mut input = make("abc");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(v)]);
    assert_eq!(mode(&input), ViMode::Visual);
    esc(&mut term, &mut input);
    assert_eq!(mode(&input), ViMode::Normal);
}

#[test]
fn visual_d_deletes_selection() {
    let mut input = make("abcdef");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(v), chord!(l), chord!(l)]);
    send(&mut term, &mut input, &[chord!(d)]);
    assert_eq!(mode(&input), ViMode::Normal);
    assert_eq!(input.get_string(), "def");
}

#[test]
fn u_undoes_last_edit() {
    let mut input = make("abc");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(d), chord!(w)]);
    assert_eq!(input.get_string(), "");
    send(&mut term, &mut input, &[chord!(u)]);
    assert_eq!(input.get_string(), "abc");
}

#[test]
fn ctrl_r_redoes_after_undo() {
    let mut input = make("abc");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(d), chord!(w)]);
    send(&mut term, &mut input, &[chord!(u)]);
    assert_eq!(input.get_string(), "abc");
    send(&mut term, &mut input, &[chord!(Ctrl + r)]);
    assert_eq!(input.get_string(), "");
}

#[test]
fn dot_repeats_last_edit() {
    let mut input = make("foo bar baz qux");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(d), chord!(w)]);
    assert_eq!(input.get_string(), "bar baz qux");
    send(&mut term, &mut input, &[chord!('.')]);
    assert_eq!(input.get_string(), "baz qux");
    send(&mut term, &mut input, &[chord!('.')]);
    assert_eq!(input.get_string(), "qux");
}

#[test]
fn dot_repeats_insert() {
    let mut input = make("a");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(a), chord!(X)]);
    esc(&mut term, &mut input);
    let before_dot = input.get_string();
    assert_eq!(before_dot, "aX");
    send(&mut term, &mut input, &[chord!('.')]);
    let after_dot = input.get_string();
    assert_ne!(after_dot, before_dot, "dot should repeat the insert");
    assert!(after_dot.contains('X'));
}

#[test]
fn x_deletes_character_under_cursor() {
    let mut input = make("abc");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(x)]);
    assert_eq!(input.get_string(), "bc");
}

#[test]
fn r_replaces_single_character() {
    let mut input = make("abc");
    let mut term = Emulator::new(&mut *input, Vec2::new(20, 3));
    esc(&mut term, &mut input);
    send(&mut term, &mut input, &[chord!(r), chord!(Z)]);
    assert_eq!(input.get_string(), "Zbc");
    assert_eq!(mode(&input), ViMode::Normal);
}

#[test]
fn g0_g_dollar_stay_on_screen_line() {
    let row = |input: &Input| -> i32 {
        let (_, text) = input.get_bindings_as::<Vi>().expect("vi");
        let editor = input.get_editor();
        editor.get_cursor().get_virtual_pos(text, editor.get_wrap_bias()).y as i32
    };
    let mut input = make("helloworld");
    let mut term = Emulator::new(&mut *input, Vec2::new(5, 5));
    esc(&mut term, &mut input);

    for _ in 0..6 {
        send(&mut term, &mut input, &[chord!(l)]);
    }
    assert_eq!(row(&input), 1);
    send(&mut term, &mut input, &[chord!(g), chord!('$')]);
    assert_eq!((cursor(&input), row(&input)), (8, 1));
    send(&mut term, &mut input, &[chord!(g), chord!(0)]);
    assert_eq!((cursor(&input), row(&input)), (4, 1));

    send(&mut term, &mut input, &[chord!(g), chord!(g)]);
    for _ in 0..3 {
        send(&mut term, &mut input, &[chord!(l)]);
    }
    assert_eq!(row(&input), 0);
    send(&mut term, &mut input, &[chord!(g), chord!('$')]);
    assert_eq!((cursor(&input), row(&input)), (4, 0));
    send(&mut term, &mut input, &[chord!(g), chord!(0)]);
    assert_eq!((cursor(&input), row(&input)), (0, 0));
}


