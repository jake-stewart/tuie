//! Terminal palette querying and color resolution.

use crate::prelude::*;
use crate::util::lab;
use crate::util::rgb::Rgb;
use crate::ansi::ColorType;
use crate::ansi::query::{QueryBatch, QueryColor, QueryHandle, QueryResults};
use std::cell::RefCell;

/// Classification of a [`Palette`] relative to the terminal's foreground and background.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteKind {
    /// Palette lightness ordering matches the terminal's foreground and background.
    Semantic,
    /// Palette lightness ordering is inverted relative to the terminal.
    Inverted,
    /// Palette was not generated from the terminal's foreground and background.
    Legacy,
}

impl std::fmt::Display for PaletteKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Semantic => write!(f, "Semantic"),
            Self::Inverted => write!(f, "Inverted"),
            Self::Legacy => write!(f, "Legacy"),
        }
    }
}

/// 256-color terminal palette.
#[derive(Clone, PartialEq, Eq)]
pub struct Palette {
    fg: Rgb,
    bg: Rgb,
    colors: [Rgb; 256],
    kind: PaletteKind,
}

impl Palette {
    /// Builds a palette from `fg`, `bg`, and the 16 ANSI base colors.
    pub fn from_base16(fg: Rgb, bg: Rgb, base16: [Rgb; 16]) -> Self {
        let mut colors = [Rgb::new(0, 0, 0); 256];
        colors[..16].copy_from_slice(&base16);
        extend_to_256(fg, bg, &mut colors);
        let term_light = lab::from_rgb(bg).l > lab::from_rgb(fg).l;
        let bright_white_lighter = lab::from_rgb(colors[15]).l > lab::from_rgb(colors[8]).l;
        if term_light == bright_white_lighter {
            colors.swap(8, 15);
        }
        let palette_light = lab::from_rgb(colors[16]).l > lab::from_rgb(colors[231]).l;
        let kind = if term_light == palette_light {
            PaletteKind::Semantic
        } else {
            PaletteKind::Inverted
        };
        Self { fg, bg, colors, kind }
    }

    /// Builds a palette from a [`Theme`].
    pub fn from_theme(theme: super::Theme) -> Self {
        Self::from_base16(theme.fg, theme.bg, theme.indexed)
    }

    /// Returns the terminal foreground RGB.
    pub fn get_fg(&self) -> Rgb {
        self.fg
    }

    /// Returns the terminal background RGB.
    pub fn get_bg(&self) -> Rgb {
        self.bg
    }

    /// Returns the RGB at the given 256-color index.
    pub fn get_indexed(&self, n: u8) -> Rgb {
        self.colors[n as usize]
    }

    /// Returns the palette kind.
    pub fn get_kind(&self) -> PaletteKind {
        self.kind
    }
}

fn extend_to_256(fg: Rgb, bg: Rgb, colors: &mut [Rgb; 256]) {
    let bg_lab = lab::from_rgb(bg);
    let fg_lab = lab::from_rgb(fg);
    let corner_lab = [
        bg_lab,
        lab::from_rgb(colors[1]),
        lab::from_rgb(colors[2]),
        lab::from_rgb(colors[3]),
        lab::from_rgb(colors[4]),
        lab::from_rgb(colors[5]),
        lab::from_rgb(colors[6]),
        fg_lab,
    ];

    for r in 0..6usize {
        let t_r = r as f64 / 5.0;
        let edge_0 = lab::lerp(t_r, corner_lab[0], corner_lab[1]);
        let edge_1 = lab::lerp(t_r, corner_lab[2], corner_lab[3]);
        let edge_2 = lab::lerp(t_r, corner_lab[4], corner_lab[5]);
        let edge_3 = lab::lerp(t_r, corner_lab[6], corner_lab[7]);
        for g in 0..6usize {
            let t_g = g as f64 / 5.0;
            let face_0 = lab::lerp(t_g, edge_0, edge_1);
            let face_1 = lab::lerp(t_g, edge_2, edge_3);
            for b in 0..6usize {
                let t_b = b as f64 / 5.0;
                let cell = lab::lerp(t_b, face_0, face_1);
                colors[16 + r * 36 + g * 6 + b] = lab::to_rgb(cell);
            }
        }
    }

    for i in 0..24usize {
        let t = (i as f64 + 1.0) / 25.0;
        colors[232 + i] = lab::to_rgb(lab::lerp(t, corner_lab[0], corner_lab[7]));
    }
}

thread_local! {
    static COLOR_TABLE: RefCell<Option<Palette>> = RefCell::new(None);
}

fn color_query_types() -> Vec<ColorType> {
    let mut types: Vec<ColorType> = (0..16u8).map(ColorType::Indexed).collect();
    types.push(ColorType::Foreground);
    types.push(ColorType::Background);
    types.push(ColorType::Indexed(16));
    types.push(ColorType::Indexed(231));
    types
}

pub(crate) fn add_color_queries(
    batch: &mut QueryBatch,
) -> Vec<QueryHandle<Option<(u8, u8, u8)>>> {
    color_query_types().into_iter().map(|ct| batch.add(QueryColor(ct))).collect()
}

pub(crate) fn build_palette_from_batch(
    handles: Vec<QueryHandle<Option<(u8, u8, u8)>>>,
    results: &QueryResults,
) -> std::io::Result<Palette> {
    let mut rgb: Vec<Rgb> = Vec::with_capacity(handles.len());
    for handle in handles.iter() {
        let response = results.get(handle)?;
        let (r, g, b) = response.ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "terminal did not respond to all palette color queries",
            )
        })?;
        rgb.push(Rgb::new(r, g, b));
    }
    palette_from_rgb(&rgb)
}

pub(crate) fn query_palette() -> std::io::Result<Palette> {
    let mut batch = QueryBatch::new().timeout(std::time::Duration::from_millis(100));
    let handles = add_color_queries(&mut batch);
    let results = batch.execute()?;
    build_palette_from_batch(handles, &results)
}

fn palette_from_rgb(results: &[Rgb]) -> std::io::Result<Palette> {
    let fg_rgb = results[16];
    let bg_rgb = results[17];
    let probe_16_rgb = results[18];
    let probe_231_rgb = results[19];

    let mut base16 = [Rgb::new(0, 0, 0); 16];
    base16.copy_from_slice(&results[..16]);
    let mut palette = Palette::from_base16(fg_rgb, bg_rgb, base16);

    let probe_16_l = lab::from_rgb(probe_16_rgb).l;
    let probe_231_l = lab::from_rgb(probe_231_rgb).l;
    let palette_light = probe_16_l > probe_231_l;
    let generated = bg_rgb == probe_16_rgb && fg_rgb == probe_231_rgb;
    let term_light = lab::from_rgb(bg_rgb).l > lab::from_rgb(fg_rgb).l;
    palette.kind = if !generated {
        PaletteKind::Legacy
    } else if term_light == palette_light {
        PaletteKind::Semantic
    } else {
        PaletteKind::Inverted
    };
    Ok(palette)
}

/// Installs `palette` as the active palette.
pub fn apply_palette(palette: Palette) {
    COLOR_TABLE.with(|cell| {
        *cell.borrow_mut() = Some(palette);
    });
}

pub(crate) fn clear_palette() {
    COLOR_TABLE.with(|cell| {
        *cell.borrow_mut() = None;
    });
}

/// Resolves a [`Color`] to RGB using the active palette, returning [`None`] if no palette is set.
pub fn resolve_rgb(color: Color) -> Option<Rgb> {
    COLOR_TABLE.with(|cell| {
        let cell = cell.borrow();
        let palette = cell.as_ref()?;
        match color {
            Color::Rgb(r, g, b) => Some(Rgb::new(r, g, b)),
            Color::Foreground => Some(palette.fg),
            Color::Background => Some(palette.bg),
            Color::Indexed(n) => Some(palette.colors[n as usize]),
        }
    })
}

/// Maps a [`Color`] for terminal output according to the active [`PaletteKind`].
pub fn resolve_color(color: Color) -> Color {
    match color {
        Color::Indexed(n) if n >= 16 => {
            COLOR_TABLE.with(|cell| {
                let cell = cell.borrow();
                let Some(palette) = cell.as_ref() else {
                    return color;
                };
                match palette.kind {
                    PaletteKind::Semantic => color,
                    PaletteKind::Inverted => Color::Indexed(invert_index(n)),
                    PaletteKind::Legacy => {
                        let rgb = palette.colors[n as usize];
                        Color::Rgb(rgb.r, rgb.g, rgb.b)
                    }
                }
            })
        }
        _ => color,
    }
}

fn invert_index(n: u8) -> u8 {
    if n < 16 {
        return n;
    } else if n < 232 {
        let idx = (n - 16) as i16;
        let r = idx / 36;
        let g = (idx / 6) % 6;
        let b = idx % 6;
        let shift = 5 - r.max(g).max(b) - r.min(g).min(b);
        let inv_r = (r + shift) as u8;
        let inv_g = (g + shift) as u8;
        let inv_b = (b + shift) as u8;
        16 + inv_r * 36 + inv_g * 6 + inv_b
    } else {
        232 + (23 - (n - 232))
    }
}
