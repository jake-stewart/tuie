//! Integration tests for border rendering.

use tuie::render::border::{Border, junction, merge};

#[test]
fn identity() {
    for c in ['─', '│', '┌', '┐', '└', '┘', '┼', '├', '┤', '┬', '┴', '━', '┃', '═', '║'] {
        assert_eq!(merge(c, c), c, "merge({c:?}, {c:?})");
    }
}

#[test]
fn space_yields_other_for_unicode() {
    assert_eq!(merge(' ', '─'), '─');
    assert_eq!(merge('─', ' '), '─');
}

#[test]
fn perpendicular_lines_form_cross() {
    assert_eq!(merge('─', '│'), '┼');
    assert_eq!(merge('│', '─'), '┼');
}

#[test]
fn opposite_corners_form_cross() {
    assert_eq!(merge('└', '┐'), '┼');
    assert_eq!(merge('┌', '┘'), '┼');
    assert_eq!(merge('╭', '╯'), '┼');
}

#[test]
fn opposite_t_junctions_form_cross() {
    assert_eq!(merge('┬', '┴'), '┼');
    assert_eq!(merge('├', '┤'), '┼');
}

#[test]
fn opposing_stubs_form_lines() {
    assert_eq!(merge('╴', '╶'), '─');
    assert_eq!(merge('╵', '╷'), '│');
    assert_eq!(merge('╸', '╺'), '━');
    assert_eq!(merge('╹', '╻'), '┃');
}

#[test]
fn weight_takes_max_per_arm() {
    assert_eq!(merge('─', '━'), '━');
    assert_eq!(merge('│', '┃'), '┃');
    assert_eq!(merge('─', '═'), '═');
}

#[test]
fn mixed_weights_at_cross() {
    assert_eq!(merge('─', '┃'), '╂');
    assert_eq!(merge('━', '│'), '┿');
}

#[test]
fn corner_plus_perpendicular_arm() {
    assert_eq!(merge('┌', '╵'), '├');
    assert_eq!(merge('┐', '╵'), '┤');
}

#[test]
fn ascii_both_combine() {
    assert_eq!(merge('-', '|'), '+');
    assert_eq!(merge('-', '-'), '-');
    assert_eq!(merge('|', '|'), '|');
    assert_eq!(merge('+', '-'), '+');
    assert_eq!(merge(' ', ' '), ' ');
    assert_eq!(merge(' ', '-'), '-');
    assert_eq!(merge(' ', '+'), '+');
}

#[test]
fn ascii_mixed_with_unicode_yields_unicode() {
    assert_eq!(merge('-', '│'), '│');
    assert_eq!(merge('│', '-'), '│');
    assert_eq!(merge('─', '|'), '─');
    assert_eq!(merge('|', '─'), '─');
    assert_eq!(merge('+', '─'), '─');
}

#[test]
fn unknown_char_falls_back_to_b() {
    assert_eq!(merge('x', '─'), '─');
    assert_eq!(merge('─', 'x'), 'x');
    assert_eq!(merge('x', 'y'), 'y');
}

#[test]
fn junction_all_hidden_is_blank() {
    let h = Border::HIDDEN;
    assert_eq!(junction(h, h, h, h), Some(' '));
}

#[test]
fn junction_same_border_full_cross() {
    let s = Border::SINGLE;
    assert_eq!(junction(s, s, s, s), Some('┼'));
}

#[test]
fn junction_same_border_round_corner() {
    let r = Border::ROUND;
    let h = Border::HIDDEN;
    assert_eq!(junction(h, r, h, r), Some('╭'));
    assert_eq!(junction(r, h, r, h), Some('╯'));
}

#[test]
fn junction_double_horizontal_meets_single_vertical_edge() {
    let s = Border::SINGLE;
    let d = Border::DOUBLE;
    let h = Border::HIDDEN;
    assert_eq!(junction(h, d, s, s), Some('╞'));
    assert_eq!(junction(d, h, s, s), Some('╡'));
    assert_eq!(junction(d, d, s, s), Some('╪'));
}

#[test]
fn junction_mixed_light_heavy_cross() {
    let s = Border::SINGLE;
    let t = Border::THICK;
    assert_eq!(junction(s, s, t, t), Some('╂'));
    assert_eq!(junction(t, t, s, s), Some('┿'));
}

#[test]
fn junction_unrepresentable_mix_returns_none() {
    let t = Border::THICK;
    let d = Border::DOUBLE;
    assert_eq!(junction(t, t, d, d), None);
}

#[test]
fn junction_dashed_treated_as_light() {
    let dash = Border::DASHED;
    let d = Border::DOUBLE;
    let h = Border::HIDDEN;
    assert_eq!(junction(h, d, dash, dash), Some('╞'));
}

#[test]
fn merge_is_commutative_for_lines() {
    let cases = [
        ('─', '│'),
        ('━', '│'),
        ('─', '┃'),
        ('└', '┐'),
        ('┬', '┴'),
        ('├', '┤'),
        ('╴', '╶'),
        ('╵', '╷'),
    ];
    for (a, b) in cases {
        assert_eq!(merge(a, b), merge(b, a), "non-commutative for ({a:?}, {b:?})");
    }
}
