//! Integration tests for CIELAB color operations.

use tuie::util::lab::{Lab, from_rgb, lerp, to_rgb};
use tuie::util::rgb::Rgb;

fn approx(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() < eps
}

fn rgb_close(a: Rgb, b: Rgb, tol: i32) -> bool {
    (a.r as i32 - b.r as i32).abs() <= tol
        && (a.g as i32 - b.g as i32).abs() <= tol
        && (a.b as i32 - b.b as i32).abs() <= tol
}

#[test]
fn lab_struct_fields_accessible() {
    let c = Lab { l: 50.0, a: 10.0, b: -5.0 };
    assert_eq!(c.l, 50.0);
    assert_eq!(c.a, 10.0);
    assert_eq!(c.b, -5.0);
}

#[test]
fn lab_is_copy() {
    let c = Lab { l: 1.0, a: 2.0, b: 3.0 };
    let d = c;
    assert_eq!(c.l, d.l);
}

#[test]
fn white_maps_to_l_100() {
    let lab = from_rgb(Rgb::new(255, 255, 255));
    assert!(approx(lab.l, 100.0, 0.01), "L was {}", lab.l);
    assert!(approx(lab.a, 0.0, 0.01), "a was {}", lab.a);
    assert!(approx(lab.b, 0.0, 0.01), "b was {}", lab.b);
}

#[test]
fn black_maps_to_l_0() {
    let lab = from_rgb(Rgb::new(0, 0, 0));
    assert!(approx(lab.l, 0.0, 0.01), "L was {}", lab.l);
    assert!(approx(lab.a, 0.0, 0.01), "a was {}", lab.a);
    assert!(approx(lab.b, 0.0, 0.01), "b was {}", lab.b);
}

#[test]
fn red_known_lab_value() {
    let lab = from_rgb(Rgb::new(255, 0, 0));
    assert!(approx(lab.l, 53.24, 0.5), "L was {}", lab.l);
    assert!(approx(lab.a, 80.09, 0.5), "a was {}", lab.a);
    assert!(approx(lab.b, 67.20, 0.5), "b was {}", lab.b);
}

#[test]
fn green_known_lab_value() {
    let lab = from_rgb(Rgb::new(0, 255, 0));
    assert!(approx(lab.l, 87.73, 0.5), "L was {}", lab.l);
    assert!(approx(lab.a, -86.18, 0.5), "a was {}", lab.a);
    assert!(approx(lab.b, 83.18, 0.5), "b was {}", lab.b);
}

#[test]
fn blue_known_lab_value() {
    let lab = from_rgb(Rgb::new(0, 0, 255));
    assert!(approx(lab.l, 32.30, 0.5), "L was {}", lab.l);
    assert!(approx(lab.a, 79.20, 0.5), "a was {}", lab.a);
    assert!(approx(lab.b, -107.86, 0.5), "b was {}", lab.b);
}

#[test]
fn mid_gray_has_zero_chroma() {
    let lab = from_rgb(Rgb::new(128, 128, 128));
    assert!(approx(lab.a, 0.0, 0.01), "a was {}", lab.a);
    assert!(approx(lab.b, 0.0, 0.01), "b was {}", lab.b);
    assert!(lab.l > 40.0 && lab.l < 60.0, "L was {}", lab.l);
}

#[test]
fn round_trip_white() {
    let start = Rgb::new(255, 255, 255);
    let out = to_rgb(from_rgb(start));
    assert!(rgb_close(start, out, 1), "got {:?}", out);
}

#[test]
fn round_trip_black() {
    let start = Rgb::new(0, 0, 0);
    let out = to_rgb(from_rgb(start));
    assert!(rgb_close(start, out, 1), "got {:?}", out);
}

#[test]
fn round_trip_primary_colors() {
    let cases = [
        Rgb::new(255, 0, 0),
        Rgb::new(0, 255, 0),
        Rgb::new(0, 0, 255),
        Rgb::new(255, 255, 0),
        Rgb::new(0, 255, 255),
        Rgb::new(255, 0, 255),
    ];
    for start in cases {
        let out = to_rgb(from_rgb(start));
        assert!(rgb_close(start, out, 1), "{:?} -> {:?}", start, out);
    }
}

#[test]
fn round_trip_assorted_midtones() {
    let cases = [
        Rgb::new(64, 128, 192),
        Rgb::new(200, 100, 50),
        Rgb::new(33, 77, 121),
        Rgb::new(250, 230, 210),
        Rgb::new(15, 15, 15),
        Rgb::new(240, 240, 240),
    ];
    for start in cases {
        let out = to_rgb(from_rgb(start));
        assert!(rgb_close(start, out, 1), "{:?} -> {:?}", start, out);
    }
}

#[test]
fn to_rgb_clamps_out_of_gamut_high() {
    let lab = Lab { l: 500.0, a: 200.0, b: 200.0 };
    let out = to_rgb(lab);
    assert!(out.r == 255 || out.g == 255 || out.b == 255);
}

#[test]
fn to_rgb_clamps_out_of_gamut_low() {
    let lab = Lab { l: -50.0, a: -200.0, b: -200.0 };
    let out = to_rgb(lab);
    assert!(out.r == 0 || out.g == 0 || out.b == 0);
}

#[test]
fn lerp_at_zero_returns_a() {
    let a = Lab { l: 10.0, a: 20.0, b: 30.0 };
    let b = Lab { l: 90.0, a: -40.0, b: -50.0 };
    let m = lerp(0.0, a, b);
    assert!(approx(m.l, a.l, 1e-12));
    assert!(approx(m.a, a.a, 1e-12));
    assert!(approx(m.b, a.b, 1e-12));
}

#[test]
fn lerp_at_one_returns_b() {
    let a = Lab { l: 10.0, a: 20.0, b: 30.0 };
    let b = Lab { l: 90.0, a: -40.0, b: -50.0 };
    let m = lerp(1.0, a, b);
    assert!(approx(m.l, b.l, 1e-12));
    assert!(approx(m.a, b.a, 1e-12));
    assert!(approx(m.b, b.b, 1e-12));
}

#[test]
fn lerp_at_half_is_midpoint() {
    let a = Lab { l: 0.0, a: 0.0, b: 0.0 };
    let b = Lab { l: 100.0, a: 80.0, b: -60.0 };
    let m = lerp(0.5, a, b);
    assert!(approx(m.l, 50.0, 1e-12));
    assert!(approx(m.a, 40.0, 1e-12));
    assert!(approx(m.b, -30.0, 1e-12));
}

#[test]
fn lerp_extrapolates_beyond_one() {
    let a = Lab { l: 0.0, a: 0.0, b: 0.0 };
    let b = Lab { l: 10.0, a: 10.0, b: 10.0 };
    let m = lerp(2.0, a, b);
    assert!(approx(m.l, 20.0, 1e-12));
    assert!(approx(m.a, 20.0, 1e-12));
    assert!(approx(m.b, 20.0, 1e-12));
}
