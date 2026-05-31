//! Integration tests for keyboard modifiers.

use tuie::input::modifiers::{Modifier, Modifiers};

#[test]
fn none_is_empty() {
    let m = Modifiers::new();
    assert!(m.is_empty());
    assert_eq!(m.modifiers, 0);
    assert!(!m.has(Modifier::Shift));
    assert!(!m.has(Modifier::Ctrl));
    assert!(!m.has(Modifier::Alt));
    assert!(!m.has(Modifier::Super));
    assert!(!m.has(Modifier::Meta));
    assert!(!m.has(Modifier::Hyper));
}

#[test]
fn single_flag_constants() {
    assert_eq!(Modifiers::new().with(Modifier::Shift).modifiers, Modifier::Shift as u8);
    assert_eq!(Modifiers::new().with(Modifier::Ctrl).modifiers, Modifier::Ctrl as u8);
    assert_eq!(Modifiers::new().with(Modifier::Alt).modifiers, Modifier::Alt as u8);
    assert_eq!(Modifiers::new().with(Modifier::Super).modifiers, Modifier::Super as u8);
    assert_eq!(Modifiers::new().with(Modifier::Meta).modifiers, Modifier::Meta as u8);
    assert_eq!(Modifiers::new().with(Modifier::Hyper).modifiers, Modifier::Hyper as u8);
}

#[test]
fn modifier_repr_bits() {
    assert_eq!(Modifier::Shift as u8, 0b000001);
    assert_eq!(Modifier::Ctrl as u8, 0b000010);
    assert_eq!(Modifier::Alt as u8, 0b000100);
    assert_eq!(Modifier::Super as u8, 0b001000);
    assert_eq!(Modifier::Meta as u8, 0b010000);
    assert_eq!(Modifier::Hyper as u8, 0b100000);
}

#[test]
fn has_modifier_each_flag() {
    let m = Modifiers::new().with(Modifier::Ctrl);
    assert!(m.has(Modifier::Ctrl));
    assert!(!m.has(Modifier::Shift));
    assert!(!m.has(Modifier::Alt));
}

#[test]
fn set_modifier_toggles() {
    let mut m = Modifiers::new();
    m.set(Modifier::Ctrl, true);
    assert!(m.has(Modifier::Ctrl));
    m.set(Modifier::Ctrl, false);
    assert!(!m.has(Modifier::Ctrl));
    assert!(m.is_empty());
}

#[test]
fn setters_per_modifier() {
    let mut m = Modifiers::new();
    m.set(Modifier::Shift, true);
    m.set(Modifier::Ctrl, true);
    m.set(Modifier::Alt, true);
    m.set(Modifier::Super, true);
    m.set(Modifier::Meta, true);
    m.set(Modifier::Hyper, true);
    assert!(m.has(Modifier::Shift));
    assert!(m.has(Modifier::Ctrl));
    assert!(m.has(Modifier::Alt));
    assert!(m.has(Modifier::Super));
    assert!(m.has(Modifier::Meta));
    assert!(m.has(Modifier::Hyper));
    assert!(!m.is_empty());
    assert_eq!(m.modifiers, 0b111111);

    m.set(Modifier::Shift, false);
    m.set(Modifier::Ctrl, false);
    m.set(Modifier::Alt, false);
    m.set(Modifier::Super, false);
    m.set(Modifier::Meta, false);
    m.set(Modifier::Hyper, false);
    assert!(m.is_empty());
}

#[test]
fn builder_methods_chain() {
    let m = Modifiers::new().with(Modifier::Ctrl).with(Modifier::Shift);
    assert!(m.has(Modifier::Ctrl));
    assert!(m.has(Modifier::Shift));
    assert!(!m.has(Modifier::Alt));
    assert_eq!(m.modifiers, (Modifier::Ctrl as u8) | (Modifier::Shift as u8));
}

#[test]
fn builder_methods_all() {
    let m = Modifiers::new()
        .with(Modifier::Shift)
        .with(Modifier::Ctrl)
        .with(Modifier::Alt)
        .with(Modifier::Super)
        .with(Modifier::Meta)
        .with(Modifier::Hyper);
    assert_eq!(m.modifiers, 0b111111);
}

#[test]
fn builder_if_methods() {
    let m = Modifiers::new()
        .with_if(Modifier::Ctrl, true)
        .with_if(Modifier::Shift, false)
        .with_if(Modifier::Alt, true);
    assert!(m.has(Modifier::Ctrl));
    assert!(!m.has(Modifier::Shift));
    assert!(m.has(Modifier::Alt));
}

#[test]
fn equality_independent_of_construction_order() {
    let a = Modifiers::new().with(Modifier::Ctrl).with(Modifier::Shift).with(Modifier::Alt);
    let b = Modifiers::new().with(Modifier::Alt).with(Modifier::Shift).with(Modifier::Ctrl);
    let c = Modifiers::new().with(Modifier::Shift).with(Modifier::Alt).with(Modifier::Ctrl);
    assert_eq!(a, b);
    assert_eq!(b, c);
}

#[test]
fn equality_distinguishes_different_sets() {
    assert_ne!(Modifiers::new().with(Modifier::Ctrl), Modifiers::new().with(Modifier::Shift));
    assert_ne!(Modifiers::new(), Modifiers::new().with(Modifier::Ctrl));
    assert_ne!(Modifiers::new().with(Modifier::Ctrl).with(Modifier::Shift), Modifiers::new().with(Modifier::Ctrl));
}

#[test]
fn copy_does_not_share_state() {
    let a = Modifiers::new().with(Modifier::Ctrl);
    let mut b = a;
    b.set(Modifier::Alt, true);
    assert!(!a.has(Modifier::Alt));
    assert!(b.has(Modifier::Alt));
    assert!(b.has(Modifier::Ctrl));
}

#[test]
fn display_empty_is_empty_string() {
    assert_eq!(format!("{}", Modifiers::new()), "");
}

#[test]
fn display_single_modifier() {
    assert_eq!(format!("{}", Modifiers::new().with(Modifier::Ctrl)), "Ctrl");
    assert_eq!(format!("{}", Modifiers::new().with(Modifier::Shift)), "Shift");
    assert_eq!(format!("{}", Modifiers::new().with(Modifier::Alt)), "Alt");
    assert_eq!(format!("{}", Modifiers::new().with(Modifier::Super)), "Super");
    assert_eq!(format!("{}", Modifiers::new().with(Modifier::Meta)), "Meta");
    assert_eq!(format!("{}", Modifiers::new().with(Modifier::Hyper)), "Hyper");
}

#[test]
fn display_multiple_uses_fixed_order() {
    let m = Modifiers::new().with(Modifier::Shift).with(Modifier::Ctrl);
    assert_eq!(format!("{}", m), "Ctrl + Shift");

    let m = Modifiers::new().with(Modifier::Alt).with(Modifier::Ctrl);
    assert_eq!(format!("{}", m), "Ctrl + Alt");

    let m = Modifiers::new().with(Modifier::Shift).with(Modifier::Alt).with(Modifier::Ctrl);
    assert_eq!(format!("{}", m), "Ctrl + Alt + Shift");

    let m = Modifiers::new()
        .with(Modifier::Shift)
        .with(Modifier::Ctrl)
        .with(Modifier::Alt)
        .with(Modifier::Super)
        .with(Modifier::Meta)
        .with(Modifier::Hyper);
    assert_eq!(format!("{}", m), "Super + Meta + Hyper + Ctrl + Alt + Shift");
}

#[test]
fn display_order_independent_of_set_order() {
    let a = Modifiers::new().with(Modifier::Ctrl).with(Modifier::Alt).with(Modifier::Shift);
    let b = Modifiers::new().with(Modifier::Shift).with(Modifier::Alt).with(Modifier::Ctrl);
    assert_eq!(format!("{}", a), format!("{}", b));
}

#[test]
fn modifier_display_individual() {
    assert_eq!(format!("{}", Modifier::Ctrl), "Ctrl");
    assert_eq!(format!("{}", Modifier::Alt), "Alt");
    assert_eq!(format!("{}", Modifier::Shift), "Shift");
    assert_eq!(format!("{}", Modifier::Super), "Super");
    assert_eq!(format!("{}", Modifier::Meta), "Meta");
    assert_eq!(format!("{}", Modifier::Hyper), "Hyper");
}

#[test]
fn set_modifier_idempotent() {
    let mut m = Modifiers::new();
    m.set(Modifier::Ctrl, true);
    m.set(Modifier::Ctrl, true);
    assert!(m.has(Modifier::Ctrl));
    assert_eq!(m.modifiers, Modifier::Ctrl as u8);
}

#[test]
fn clearing_unset_modifier_is_noop() {
    let mut m = Modifiers::new().with(Modifier::Ctrl);
    m.set(Modifier::Alt, false);
    assert_eq!(m, Modifiers::new().with(Modifier::Ctrl));
}
