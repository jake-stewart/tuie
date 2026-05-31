//! Integration tests for RGB color utilities.

use tuie::util::rgb::Rgb;
#[cfg(feature = "images")]
use tuie::util::rgb::blend_over;

#[test]
fn new_stores_channels() {
    let c = Rgb::new(10, 20, 30);
    assert_eq!(c.r, 10);
    assert_eq!(c.g, 20);
    assert_eq!(c.b, 30);
}

#[test]
fn new_is_const() {
    const C: Rgb = Rgb::new(1, 2, 3);
    assert_eq!(C, Rgb { r: 1, g: 2, b: 3 });
}

#[test]
fn from_hex_extracts_channels() {
    let c = Rgb::from_hex(0x123456);
    assert_eq!(c.r, 0x12);
    assert_eq!(c.g, 0x34);
    assert_eq!(c.b, 0x56);
}

#[test]
fn from_hex_white() {
    let c = Rgb::from_hex(0xFFFFFF);
    assert_eq!(c, Rgb::new(255, 255, 255));
}

#[test]
fn from_hex_black() {
    let c = Rgb::from_hex(0x000000);
    assert_eq!(c, Rgb::new(0, 0, 0));
}

#[test]
fn from_hex_red() {
    assert_eq!(Rgb::from_hex(0xFF0000), Rgb::new(255, 0, 0));
}

#[test]
fn from_hex_green() {
    assert_eq!(Rgb::from_hex(0x00FF00), Rgb::new(0, 255, 0));
}

#[test]
fn from_hex_blue() {
    assert_eq!(Rgb::from_hex(0x0000FF), Rgb::new(0, 0, 255));
}

#[test]
fn from_hex_ignores_top_byte() {
    let a = Rgb::from_hex(0xAB_112233);
    let b = Rgb::from_hex(0x00_112233);
    assert_eq!(a, b);
}

#[test]
fn equality_and_clone() {
    let a = Rgb::new(7, 8, 9);
    let b = a;
    let c = a.clone();
    assert_eq!(a, b);
    assert_eq!(a, c);
    assert_ne!(a, Rgb::new(7, 8, 10));
}

#[cfg(feature = "images")]
#[test]
fn blend_over_fully_opaque_takes_foreground() {
    let bg = Rgb::new(0, 0, 0);
    let px = [200, 100, 50, 255];
    let out = blend_over(&px, bg);
    assert_eq!(out, Rgb::new(200, 100, 50));
}

#[cfg(feature = "images")]
#[test]
fn blend_over_fully_transparent_keeps_background() {
    let bg = Rgb::new(10, 20, 30);
    let px = [200, 100, 50, 0];
    let out = blend_over(&px, bg);
    assert_eq!(out, bg);
}

#[cfg(feature = "images")]
#[test]
fn blend_over_half_alpha_midpoint() {
    let bg = Rgb::new(0, 0, 0);
    let px = [200, 100, 50, 128];
    let out = blend_over(&px, bg);
    assert_eq!(out, Rgb::new(100, 50, 25));
}

#[cfg(feature = "images")]
#[test]
fn blend_over_matches_bg_when_fg_equals_bg() {
    let bg = Rgb::new(123, 45, 67);
    let px = [123, 45, 67, 99];
    let out = blend_over(&px, bg);
    assert_eq!(out, bg);
}

#[cfg(feature = "images")]
#[test]
fn blend_over_does_not_overflow_on_extremes() {
    let bg = Rgb::new(255, 255, 255);
    let px = [255, 255, 255, 255];
    let out = blend_over(&px, bg);
    assert_eq!(out, Rgb::new(255, 255, 255));
}
