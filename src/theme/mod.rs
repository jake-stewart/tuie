//! Color palettes and theming.

use crate::util::rgb::Rgb;

#[cfg(feature = "harmonious")]
pub mod harmonious;

/// Terminal color theme: foreground, background, and the 16 ANSI base colors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Theme {
    fg: Rgb,
    bg: Rgb,
    indexed: [Rgb; 16],
}

impl Theme {
    /// Creates a theme from `fg`, `bg`, and the 16 ANSI base colors.
    pub const fn new(fg: Rgb, bg: Rgb, indexed: [Rgb; 16]) -> Self {
        Self { fg, bg, indexed }
    }

    /// Creates a theme from the 8 ANSI base colors duplicated as their brights, with `fg` and `bg` taken from indices 7 and 0.
    pub const fn from_base8(base8: [Rgb; 8]) -> Self {
        let mut indexed = [base8[0]; 16];
        let mut i = 0;
        while i < 8 {
            indexed[i] = base8[i];
            indexed[i + 8] = base8[i];
            i += 1;
        }
        Self { fg: base8[7], bg: base8[0], indexed }
    }

    /// Creates a theme from the 16 ANSI base colors, with `fg` and `bg` taken from indices 7 and 0.
    pub const fn from_base16(indexed: [Rgb; 16]) -> Self {
        Self { fg: indexed[7], bg: indexed[0], indexed }
    }

    /// Returns the foreground RGB.
    pub const fn get_fg(&self) -> Rgb {
        self.fg
    }

    /// Returns the background RGB.
    pub const fn get_bg(&self) -> Rgb {
        self.bg
    }

    /// Returns the RGB at the given ANSI index.
    ///
    /// # Panics
    ///
    /// Panics if `n >= 16`.
    pub const fn get_indexed(&self, n: u8) -> Rgb {
        self.indexed[n as usize]
    }

    /// One Dark color scheme.
    pub const ONE_DARK: Self = Self::from_base8([
        Rgb::from_hex(0x282c34),
        Rgb::from_hex(0xe06c75),
        Rgb::from_hex(0x98c379),
        Rgb::from_hex(0xe5c07b),
        Rgb::from_hex(0x61afef),
        Rgb::from_hex(0xc678dd),
        Rgb::from_hex(0x56b6c2),
        Rgb::from_hex(0xabb2bf),
    ]);

    /// One Light color scheme.
    pub const ONE_LIGHT: Self = Self::from_base8([
        Rgb::from_hex(0xfafafa),
        Rgb::from_hex(0xe06c75),
        Rgb::from_hex(0x98c379),
        Rgb::from_hex(0xe5c07b),
        Rgb::from_hex(0x61afef),
        Rgb::from_hex(0xc678dd),
        Rgb::from_hex(0x56b6c2),
        Rgb::from_hex(0x383a42),
    ]);

    /// Gruvbox Dark color scheme.
    pub const GRUVBOX_DARK: Self = Self::from_base8([
        Rgb::from_hex(0x282828),
        Rgb::from_hex(0xcc241d),
        Rgb::from_hex(0x98971a),
        Rgb::from_hex(0xd79921),
        Rgb::from_hex(0x458588),
        Rgb::from_hex(0xb16286),
        Rgb::from_hex(0x689d6a),
        Rgb::from_hex(0xebdbb2),
    ]);

    /// Gruvbox Light color scheme.
    pub const GRUVBOX_LIGHT: Self = Self::from_base8([
        Rgb::from_hex(0xfcf1c7),
        Rgb::from_hex(0xcc241d),
        Rgb::from_hex(0x98971a),
        Rgb::from_hex(0xd79921),
        Rgb::from_hex(0x458588),
        Rgb::from_hex(0xb16286),
        Rgb::from_hex(0x689d6a),
        Rgb::from_hex(0x3d3836),
    ]);

    /// Solarized Dark color scheme.
    pub const SOLARIZED_DARK: Self = Self::from_base8([
        Rgb::from_hex(0x002b36),
        Rgb::from_hex(0xdc322f),
        Rgb::from_hex(0x859900),
        Rgb::from_hex(0xb58900),
        Rgb::from_hex(0x268bd2),
        Rgb::from_hex(0x6c71c4),
        Rgb::from_hex(0x2aa198),
        Rgb::from_hex(0x93a1a1),
    ]);

    /// Solarized Light color scheme.
    pub const SOLARIZED_LIGHT: Self = Self::from_base8([
        Rgb::from_hex(0xfdf6e3),
        Rgb::from_hex(0xdc322f),
        Rgb::from_hex(0x859900),
        Rgb::from_hex(0xb58900),
        Rgb::from_hex(0x268bd2),
        Rgb::from_hex(0x6c71c4),
        Rgb::from_hex(0x2aa198),
        Rgb::from_hex(0x586e75),
    ]);

    /// Everforest Dark color scheme.
    pub const EVERFOREST_DARK: Self = Self::from_base8([
        Rgb::from_hex(0x2d353b),
        Rgb::from_hex(0xe67e80),
        Rgb::from_hex(0xa7c080),
        Rgb::from_hex(0xdbbc7f),
        Rgb::from_hex(0x7fbbb3),
        Rgb::from_hex(0xd699b6),
        Rgb::from_hex(0x83c092),
        Rgb::from_hex(0xd3c6aa),
    ]);

    /// Everforest Light color scheme.
    pub const EVERFOREST_LIGHT: Self = Self::from_base8([
        Rgb::from_hex(0xfdf6e3),
        Rgb::from_hex(0xe67e80),
        Rgb::from_hex(0xa7c080),
        Rgb::from_hex(0xdbbc7f),
        Rgb::from_hex(0x7fbbb3),
        Rgb::from_hex(0xd699b6),
        Rgb::from_hex(0x83c092),
        Rgb::from_hex(0x5c6a72),
    ]);

    /// Century Dark color scheme.
    pub const CENTURY_DARK: Self = Self::from_base8([
        Rgb::from_hex(0x2d323b),
        Rgb::from_hex(0xc18181),
        Rgb::from_hex(0x91b191),
        Rgb::from_hex(0xc9a989),
        Rgb::from_hex(0x81a1c1),
        Rgb::from_hex(0xb191b1),
        Rgb::from_hex(0x91b1b1),
        Rgb::from_hex(0xa1a1a1),
    ]);

    /// Century Light color scheme.
    pub const CENTURY_LIGHT: Self = Self::from_base8([
        Rgb::from_hex(0xd4d8dc),
        Rgb::from_hex(0xe05661),
        Rgb::from_hex(0x599a54),
        Rgb::from_hex(0xbc8f2f),
        Rgb::from_hex(0x3d92cc),
        Rgb::from_hex(0x8a69b8),
        Rgb::from_hex(0x50a5a2),
        Rgb::from_hex(0x6a6a6a),
    ]);
}
