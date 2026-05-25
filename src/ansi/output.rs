//! Raw ANSI escape emission.

use crate::prelude::*;
use crate::render::style::StyleAttribute;
use crate::render::GridCellStyle;

#[inline(always)]
fn push_byte(out: &mut String, b: u8) {
    unsafe { out.as_mut_vec().push(b) };
}

#[inline(always)]
fn push_bytes(out: &mut String, bytes: &[u8]) {
    unsafe { out.as_mut_vec().extend_from_slice(bytes) };
}

#[inline]
fn push_u8(out: &mut String, n: u8) {
    let v = unsafe { out.as_mut_vec() };
    if n >= 100 {
        v.push(b'0' + n / 100);
        v.push(b'0' + (n / 10) % 10);
        v.push(b'0' + n % 10);
    } else if n >= 10 {
        v.push(b'0' + n / 10);
        v.push(b'0' + n % 10);
    } else {
        v.push(b'0' + n);
    }
}

#[inline]
fn push_u16(out: &mut String, n: u16) {
    if n < 256 {
        push_u8(out, n as u8);
        return;
    }
    let v = unsafe { out.as_mut_vec() };
    if n >= 10000 {
        v.push(b'0' + (n / 10000) as u8);
        v.push(b'0' + ((n / 1000) % 10) as u8);
        v.push(b'0' + ((n / 100) % 10) as u8);
        v.push(b'0' + ((n / 10) % 10) as u8);
        v.push(b'0' + (n % 10) as u8);
    } else if n >= 1000 {
        v.push(b'0' + (n / 1000) as u8);
        v.push(b'0' + ((n / 100) % 10) as u8);
        v.push(b'0' + ((n / 10) % 10) as u8);
        v.push(b'0' + (n % 10) as u8);
    } else {
        v.push(b'0' + (n / 100) as u8);
        v.push(b'0' + ((n / 10) % 10) as u8);
        v.push(b'0' + (n % 10) as u8);
    }
}

/// Emits an absolute cursor-position escape for 0-indexed `col` and `row`.
#[inline]
pub fn move_to(out: &mut String, col: u16, row: u16) {
    push_bytes(out, b"\x1b[");
    push_u16(out, row + 1);
    push_byte(out, b';');
    push_u16(out, col + 1);
    push_byte(out, b'H');
}

/// Emits a move-to-column escape for 0-indexed `col` on the current row.
#[inline]
pub fn move_to_column(out: &mut String, col: u16) {
    push_bytes(out, b"\x1b[");
    push_u16(out, col + 1);
    push_byte(out, b'G');
}

/// Emits a move-cursor-left escape for `n` columns.
#[inline]
pub fn move_left(out: &mut String, n: u16) {
    if n == 0 {
        return;
    }
    push_bytes(out, b"\x1b[");
    push_u16(out, n);
    push_byte(out, b'D');
}

/// Emits a move-cursor-right escape for `n` columns.
#[inline]
pub fn move_right(out: &mut String, n: u16) {
    if n == 0 {
        return;
    }
    push_bytes(out, b"\x1b[");
    push_u16(out, n);
    push_byte(out, b'C');
}

/// Emits the clear-screen escape.
#[inline]
pub fn clear_screen(out: &mut String) {
    out.push_str("\x1b[2J");
}

/// Enables auto-wrap at the right margin.
#[inline]
pub fn enable_line_wrap(out: &mut String) {
    out.push_str("\x1b[?7h");
}

/// Disables auto-wrap at the right margin.
#[inline]
pub fn disable_line_wrap(out: &mut String) {
    out.push_str("\x1b[?7l");
}

/// Resets all SGR attributes.
#[inline]
pub fn reset_attrs(out: &mut String) {
    out.push_str("\x1b[0m");
}

#[inline]
fn sep(out: &mut String, started: &mut bool) {
    if *started {
        push_byte(out, b';');
    } else {
        push_bytes(out, b"\x1b[");
        *started = true;
    }
}

#[inline]
fn push_rgb(out: &mut String, prefix: &[u8], r: u8, g: u8, b: u8) {
    push_bytes(out, prefix);
    push_u8(out, r);
    push_byte(out, b';');
    push_u8(out, g);
    push_byte(out, b';');
    push_u8(out, b);
}

#[cfg(feature = "harmonious")]
#[inline]
fn resolve_sentinel_rgb(sentinel: Color) -> Option<(u8, u8, u8)> {
    crate::theme::harmonious::resolve_rgb(sentinel).map(|rgb| (rgb.r, rgb.g, rgb.b))
}

#[inline]
fn write_fg(out: &mut String, color: Color) {
    match color {
        Color::Foreground => push_bytes(out, b"39"),
        Color::Background => {
            #[cfg(feature = "harmonious")]
            if let Some((r, g, b)) = resolve_sentinel_rgb(Color::Background) {
                return push_rgb(out, b"38;2;", r, g, b);
            }
            push_bytes(out, b"39");
        }
        Color::Base256(n) => match n {
            0..=7 => push_u8(out, 30 + n),
            8..=15 => push_u8(out, 90 + (n - 8)),
            n => {
                push_bytes(out, b"38;5;");
                push_u8(out, n);
            }
        },
        Color::Rgb(r, g, b) => push_rgb(out, b"38;2;", r, g, b),
    }
}

#[inline]
fn write_bg(out: &mut String, color: Color) {
    match color {
        Color::Background => push_bytes(out, b"49"),
        Color::Foreground => {
            #[cfg(feature = "harmonious")]
            if let Some((r, g, b)) = resolve_sentinel_rgb(Color::Foreground) {
                return push_rgb(out, b"48;2;", r, g, b);
            }
            push_bytes(out, b"49");
        }
        Color::Base256(n) => match n {
            0..=7 => push_u8(out, 40 + n),
            8..=15 => push_u8(out, 100 + (n - 8)),
            n => {
                push_bytes(out, b"48;5;");
                push_u8(out, n);
            }
        },
        Color::Rgb(r, g, b) => push_rgb(out, b"48;2;", r, g, b),
    }
}

#[inline]
fn write_underline_type(out: &mut String, ty: UnderlineType) {
    match ty {
        UnderlineType::None => push_bytes(out, b"24"),
        UnderlineType::Single => push_byte(out, b'4'),
        UnderlineType::Double => push_bytes(out, b"4:2"),
        UnderlineType::Curly => push_bytes(out, b"4:3"),
        UnderlineType::Dotted => push_bytes(out, b"4:4"),
        UnderlineType::Dashed => push_bytes(out, b"4:5"),
    }
}

#[inline]
fn write_underline_color(out: &mut String, color: Color) {
    match color {
        Color::Foreground | Color::Background => push_bytes(out, b"59"),
        Color::Base256(n) => {
            push_bytes(out, b"58:5:");
            push_u8(out, n);
        }
        Color::Rgb(r, g, b) => {
            push_bytes(out, b"58:2::");
            push_u8(out, r);
            push_byte(out, b':');
            push_u8(out, g);
            push_byte(out, b':');
            push_u8(out, b);
        }
    }
}

#[inline]
fn resolve(style: &GridCellStyle) -> (Color, Color, bool) {
    let mut fg = style.fg;
    let mut bg = style.bg;
    let mut reverse = style.attrs & StyleAttribute::Reverse as u8 != 0;
    if (fg == Color::Background || bg == Color::Foreground) && fg != bg {
        std::mem::swap(&mut fg, &mut bg);
        reverse = !reverse;
    }
    (fg, bg, reverse)
}

/// Emits an SGR escape for the difference between `prev` and `new` styles.
#[inline]
pub(crate) fn write_style_diff(out: &mut String, prev: &GridCellStyle, new: &GridCellStyle) {
    let (prev_fg, prev_bg, prev_rev) = resolve(prev);
    let (new_fg, new_bg, new_rev) = resolve(new);

    let prev_attrs_no_rev = prev.attrs & !(StyleAttribute::Reverse as u8);
    let new_attrs_no_rev = new.attrs & !(StyleAttribute::Reverse as u8);
    let added = new_attrs_no_rev & !prev_attrs_no_rev;
    let removed = prev_attrs_no_rev & !new_attrs_no_rev;

    let bold = StyleAttribute::Bold as u8;
    let dim = StyleAttribute::Dim as u8;
    let italic = StyleAttribute::Italic as u8;
    let strike = StyleAttribute::Strikethrough as u8;
    let bold_dim = bold | dim;

    let mut started = false;

    if prev_fg != new_fg {
        sep(out, &mut started);
        write_fg(out, new_fg);
    }
    if prev_bg != new_bg {
        sep(out, &mut started);
        write_bg(out, new_bg);
    }

    if removed & bold_dim != 0 {
        sep(out, &mut started);
        push_bytes(out, b"22");
        let kept = new_attrs_no_rev & bold_dim;
        if kept & bold != 0 {
            sep(out, &mut started);
            push_byte(out, b'1');
        }
        if kept & dim != 0 {
            sep(out, &mut started);
            push_byte(out, b'2');
        }
    } else {
        if added & bold != 0 {
            sep(out, &mut started);
            push_byte(out, b'1');
        }
        if added & dim != 0 {
            sep(out, &mut started);
            push_byte(out, b'2');
        }
    }
    if removed & italic != 0 {
        sep(out, &mut started);
        push_bytes(out, b"23");
    }
    if added & italic != 0 {
        sep(out, &mut started);
        push_byte(out, b'3');
    }
    if removed & strike != 0 {
        sep(out, &mut started);
        push_bytes(out, b"29");
    }
    if added & strike != 0 {
        sep(out, &mut started);
        push_byte(out, b'9');
    }

    if prev_rev != new_rev {
        sep(out, &mut started);
        if new_rev {
            push_byte(out, b'7');
        } else {
            push_bytes(out, b"27");
        }
    }

    if prev.underline != new.underline {
        sep(out, &mut started);
        write_underline_type(out, new.underline);
    }
    if prev.underline_color != new.underline_color {
        sep(out, &mut started);
        write_underline_color(out, new.underline_color);
    }

    if started {
        push_byte(out, b'm');
    }
}


/// Enters the alternate screen buffer (`CSI ? 1049 h`).
#[inline]
pub fn enter_alternate_screen(out: &mut String) {
    out.push_str("\x1b[?1049h");
}

/// Leaves the alternate screen buffer (`CSI ? 1049 l`).
#[inline]
pub fn leave_alternate_screen(out: &mut String) {
    out.push_str("\x1b[?1049l");
}

/// Begins a synchronized update (`CSI ? 2026 h`).
#[inline]
pub fn begin_synchronized_update(out: &mut String) {
    out.push_str("\x1b[?2026h");
}

/// Ends a synchronized update (`CSI ? 2026 l`).
#[inline]
pub fn end_synchronized_update(out: &mut String) {
    out.push_str("\x1b[?2026l");
}

/// Enables press/release mouse tracking (DEC private mode 1000).
#[inline]
pub fn enable_mouse_click_events(out: &mut String) {
    out.push_str("\x1b[?1000h");
}

/// Enables button-motion (drag) mouse tracking (DEC private mode 1002).
#[inline]
pub fn enable_mouse_drag_events(out: &mut String) {
    out.push_str("\x1b[?1002h");
}

/// Enables any-motion (hover) mouse tracking (DEC private mode 1003).
#[inline]
pub fn enable_mouse_hover_events(out: &mut String) {
    out.push_str("\x1b[?1003h");
}

/// Enables SGR mouse reporting (DEC private mode 1006).
#[inline]
pub fn enable_sgr_mouse(out: &mut String) {
    out.push_str("\x1b[?1006h");
}

/// Enables pixel-precision mouse reporting (DEC private mode 1016). Requires SGR (1006) to be enabled first.
#[inline]
pub fn enable_mouse_pixel_capture(out: &mut String) {
    out.push_str("\x1b[?1016h");
}

/// Disables pixel-precision mouse reporting (DEC private mode 1016).
#[inline]
pub fn disable_mouse_pixel_capture(out: &mut String) {
    out.push_str("\x1b[?1016l");
}

/// Disables all mouse tracking modes.
#[inline]
pub fn disable_mouse_capture(out: &mut String) {
    out.push_str("\x1b[?1006l\x1b[?1015l\x1b[?1003l\x1b[?1002l\x1b[?1000l");
}

/// Enables focus-change reporting (DEC private mode 1004).
#[inline]
pub fn enable_focus_change(out: &mut String) {
    out.push_str("\x1b[?1004h");
}

/// Disables focus-change reporting (DEC private mode 1004).
#[inline]
pub fn disable_focus_change(out: &mut String) {
    out.push_str("\x1b[?1004l");
}

/// Enables bracketed paste (DEC private mode 2004).
#[inline]
pub fn enable_bracketed_paste(out: &mut String) {
    out.push_str("\x1b[?2004h");
}

/// Disables bracketed paste (DEC private mode 2004).
#[inline]
pub fn disable_bracketed_paste(out: &mut String) {
    out.push_str("\x1b[?2004l");
}

/// Enables color-scheme (light/dark) change reporting (DEC private mode 2031).
#[inline]
pub fn enable_color_scheme_detection(out: &mut String) {
    out.push_str("\x1b[?2031h");
}

/// Disables color-scheme change reporting (DEC private mode 2031).
#[inline]
pub fn disable_color_scheme_detection(out: &mut String) {
    out.push_str("\x1b[?2031l");
}

/// Pushes kitty keyboard enhancement flags (`CSI > {flags} u`).
#[inline]
pub fn push_keyboard_enhancement_flags(out: &mut String, flags: u8) {
    push_bytes(out, b"\x1b[>");
    push_u8(out, flags);
    push_byte(out, b'u');
}

/// Pops the kitty keyboard enhancement flags stack (`CSI < 1 u`).
#[inline]
pub fn pop_keyboard_enhancement_flags(out: &mut String) {
    out.push_str("\x1b[<1u");
}

/// Shows the cursor (`CSI ? 25 h`).
#[inline]
pub fn show_cursor(out: &mut String) {
    out.push_str("\x1b[?25h");
}

/// Hides the cursor (`CSI ? 25 l`).
#[inline]
pub fn hide_cursor(out: &mut String) {
    out.push_str("\x1b[?25l");
}

/// Resets the cursor style to the terminal default (DECSCUSR, `CSI 0 SP q`).
#[inline]
pub fn reset_cursor_style(out: &mut String) {
    out.push_str("\x1b[0 q");
}

/// Sets the cursor style (DECSCUSR, `CSI n SP q`) from a [`CursorShape`] and blink flag.
#[inline]
pub fn set_cursor_style(out: &mut String, shape: CursorShape, blink: bool) {
    let code: u8 = match (shape, blink) {
        (CursorShape::Block, true) => 1,
        (CursorShape::Block, false) => 2,
        (CursorShape::Underline, true) => 3,
        (CursorShape::Underline, false) => 4,
        (CursorShape::Beam, true) => 5,
        (CursorShape::Beam, false) => 6,
    };
    push_bytes(out, b"\x1b[");
    push_u8(out, code);
    push_bytes(out, b" q");
}
