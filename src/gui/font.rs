//! Font loading and glyph rasterization for the GUI backend.

use crate::prelude::*;
use freetype::bitmap::PixelMode;
use freetype::face::LoadFlag;
use freetype::{Face, Library};
use std::collections::HashMap;
use std::rc::Rc;

use super::GuiConfig;
use super::box_drawing;

type LoadedFonts = (
    Library,
    Vec<Face>,
    Option<Face>,
    Option<Face>,
    Option<Face>,
);

pub(crate) struct FontCache {
    _library: Library,
    fonts: Vec<Face>,
    bold_primary: Option<Face>,
    italic_primary: Option<Face>,
    bold_italic_primary: Option<Face>,
    px_size: f32,
    cell_w: u32,
    cell_h: u32,
    baseline: i32,
    glyphs: HashMap<(char, bool, bool), Glyph>,
    box_masks: HashMap<char, Vec<u8>>,
}

pub(crate) struct Glyph {
    bitmap: Vec<u8>,
    width: u32,
    height: u32,
    x_off: i32,
    y_off: i32,
    synth_bold: bool,
    synth_italic: bool,
}

impl Glyph {
    /// Returns the rasterized alpha bitmap, row-major.
    pub fn get_bitmap(&self) -> &[u8] {
        &self.bitmap
    }

    /// Returns the bitmap width in pixels.
    pub fn get_width(&self) -> u32 {
        self.width
    }

    /// Returns the bitmap height in pixels.
    pub fn get_height(&self) -> u32 {
        self.height
    }

    /// Returns the horizontal pixel offset from the cell origin to the bitmap's left edge.
    pub fn get_x_off(&self) -> i32 {
        self.x_off
    }

    /// Returns the vertical pixel offset from the cell top to the bitmap's top edge.
    pub fn get_y_off(&self) -> i32 {
        self.y_off
    }

    /// Returns whether bold was synthesized rather than provided by the font.
    pub fn is_synth_bold(&self) -> bool {
        self.synth_bold
    }

    /// Returns whether italic was synthesized rather than provided by the font.
    pub fn is_synth_italic(&self) -> bool {
        self.synth_italic
    }
}

impl FontCache {
    fn apply_pixel_size(
        fonts: &[Face],
        bold_primary: Option<&Face>,
        italic_primary: Option<&Face>,
        bold_italic_primary: Option<&Face>,
        px_int: u32,
    ) -> std::io::Result<(u32, u32, i32)> {
        fonts[0].set_pixel_sizes(0, px_int).map_err(Self::io_err)?;
        for face in fonts.iter().skip(1) {
            let _ = face.set_pixel_sizes(0, px_int);
        }
        for f in [bold_primary, italic_primary, bold_italic_primary]
            .into_iter()
            .flatten()
        {
            let _ = f.set_pixel_sizes(0, px_int);
        }
        let primary = &fonts[0];
        primary.load_char('M' as usize, LoadFlag::DEFAULT).map_err(Self::io_err)?;
        let m_advance = (primary.glyph().metrics().horiAdvance >> 6) as u32;
        let metrics = primary
            .size_metrics()
            .ok_or_else(|| Self::io_err("font lacks size metrics"))?;
        let cell_w = m_advance.max(1);
        let cell_h = ((metrics.height >> 6) as u32).max(1);
        let baseline = (metrics.ascender >> 6) as i32;
        Ok((cell_w, cell_h, baseline))
    }

    fn rasterize(face: &Face, c: char, baseline: i32) -> (Vec<u8>, u32, u32, i32, i32) {
        if face.load_char(c as usize, LoadFlag::RENDER | LoadFlag::TARGET_NORMAL).is_err() {
            return (Vec::new(), 0, 0, 0, 0);
        }
        let slot = face.glyph();
        let bitmap = slot.bitmap();
        let pixel_mode = match bitmap.pixel_mode() {
            Ok(p) => p,
            Err(_) => return (Vec::new(), 0, 0, 0, 0),
        };
        if !matches!(pixel_mode, PixelMode::Gray) {
            return (Vec::new(), 0, 0, 0, 0);
        }
        let w = bitmap.width().max(0) as u32;
        let h = bitmap.rows().max(0) as u32;
        let x_off = slot.bitmap_left();
        let y_off = baseline - slot.bitmap_top();
        if w == 0 || h == 0 {
            return (Vec::new(), w, h, x_off, y_off);
        }
        let pitch = bitmap.pitch();
        let buf = bitmap.buffer();
        let row_len = w as usize;
        let mut tight = vec![0u8; row_len * h as usize];
        if pitch >= 0 {
            let stride = pitch as usize;
            for row in 0..h as usize {
                let src = &buf[row * stride..row * stride + row_len];
                tight[row * row_len..(row + 1) * row_len].copy_from_slice(src);
            }
        } else {
            let stride = (-pitch) as usize;
            let last = h as usize - 1;
            for row in 0..h as usize {
                let src_row = last - row;
                let src = &buf[src_row * stride..src_row * stride + row_len];
                tight[row * row_len..(row + 1) * row_len].copy_from_slice(src);
            }
        }
        (tight, w, h, x_off, y_off)
    }

    fn load_fonts(cfg: &GuiConfig, db: Option<fontdb::Database>) -> std::io::Result<LoadedFonts> {
        let library = Library::init().map_err(Self::io_err)?;
        let db = db.unwrap_or_else(|| {
            let mut db = fontdb::Database::new();
            db.load_system_fonts();
            db
        });
        let families: Vec<fontdb::Family> = match &cfg.font_family {
            Some(name) => vec![fontdb::Family::Name(name), fontdb::Family::Monospace],
            None => vec![fontdb::Family::Monospace],
        };
        let primary_data: Vec<u8> = if let Some(data) = &cfg.font_data {
            data.clone()
        } else {
            let query = fontdb::Query {
                families: &families,
                ..Default::default()
            };
            let id = db
                .query(&query)
                .ok_or_else(|| Self::io_err("no monospace font found on system"))?;
            db.with_face_data(id, |data, _index| data.to_vec())
                .ok_or_else(|| Self::io_err("could not read primary font data"))?
        };
        let primary = library
            .new_memory_face(Rc::new(primary_data), 0)
            .map_err(Self::io_err)?;
        let mut fonts = vec![primary];
        for name in cfg.font_fallbacks.iter().map(String::as_str).chain(Self::platform_fallbacks().iter().copied()) {
            if let Some(font) = Self::load_named(&library, &db, name, fontdb::Weight::NORMAL, fontdb::Style::Normal) {
                fonts.push(font);
            }
        }
        let (bold_primary, italic_primary, bold_italic_primary) = if cfg.font_data.is_some() {
            (None, None, None)
        } else {
            let pick = |weight: fontdb::Weight, style: fontdb::Style| -> Option<Face> {
                let query = fontdb::Query {
                    families: &families,
                    weight,
                    style,
                    ..Default::default()
                };
                db.query(&query).and_then(|id| {
                    let bytes = db.with_face_data(id, |data, _index| data.to_vec())?;
                    library.new_memory_face(Rc::new(bytes), 0).ok()
                })
            };
            (
                pick(fontdb::Weight::BOLD, fontdb::Style::Normal),
                pick(fontdb::Weight::NORMAL, fontdb::Style::Italic),
                pick(fontdb::Weight::BOLD, fontdb::Style::Italic),
            )
        };
        Ok((library, fonts, bold_primary, italic_primary, bold_italic_primary))
    }

    fn load_named(
        library: &Library,
        db: &fontdb::Database,
        name: &str,
        weight: fontdb::Weight,
        style: fontdb::Style,
    ) -> Option<Face> {
        let families = [fontdb::Family::Name(name)];
        let query = fontdb::Query {
            families: &families,
            weight,
            style,
            ..Default::default()
        };
        let id = db.query(&query)?;
        let bytes = db.with_face_data(id, |data, _index| data.to_vec())?;
        library.new_memory_face(Rc::new(bytes), 0).ok()
    }

    fn platform_fallbacks() -> &'static [&'static str] {
        #[cfg(target_os = "macos")]
        {
            &[
                "Apple Symbols",
                "Symbol",
                "PingFang SC",
                "Hiragino Sans",
            ]
        }
        #[cfg(target_os = "linux")]
        {
            &[
                "DejaVu Sans Mono",
                "DejaVu Sans",
                "Noto Sans Mono CJK SC",
                "Noto Sans CJK SC",
            ]
        }
        #[cfg(target_os = "windows")]
        {
            &[
                "Segoe UI Symbol",
                "Microsoft YaHei",
                "Cambria Math",
            ]
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            &[]
        }
    }

    fn io_err<E: std::fmt::Display>(e: E) -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
    }
}

impl FontCache {
    /// Creates a [`FontCache`] from a [`GuiConfig`], optionally reusing a pre-loaded `fontdb::Database`.
    pub fn new_with_db(
        cfg: &GuiConfig,
        db: Option<fontdb::Database>,
        scale: u32,
    ) -> std::io::Result<Self> {
        let px_size = cfg.font_size * scale as f32;
        let px_int = (px_size.round() as u32).max(1);
        let (library, fonts, bold_primary, italic_primary, bold_italic_primary) = Self::load_fonts(cfg, db)?;
        let (cell_w, cell_h, baseline) = Self::apply_pixel_size(
            &fonts,
            bold_primary.as_ref(),
            italic_primary.as_ref(),
            bold_italic_primary.as_ref(),
            px_int,
        )?;
        Ok(Self {
            _library: library,
            fonts,
            bold_primary,
            italic_primary,
            bold_italic_primary,
            px_size,
            cell_w,
            cell_h,
            baseline,
            glyphs: HashMap::new(),
            box_masks: HashMap::new(),
        })
    }

    /// Returns the cell width in pixels.
    pub fn get_cell_w(&self) -> u32 {
        self.cell_w
    }

    /// Returns the cell height in pixels.
    pub fn get_cell_h(&self) -> u32 {
        self.cell_h
    }

    /// Returns the rasterized mask for box-drawing codepoint `ch`, or `None` if unsupported.
    pub fn get_box_mask(&mut self, ch: char) -> Option<&[u8]> {
        if !box_drawing::is_box_codepoint(ch) {
            return None;
        }
        if !self.box_masks.contains_key(&ch) {
            let mask = box_drawing::rasterize(ch, Vec2::new(self.cell_w, self.cell_h))?;
            self.box_masks.insert(ch, mask);
        }
        self.box_masks.get(&ch).map(|v| v.as_slice())
    }

    /// Returns the current font pixel size.
    pub fn get_px_size(&self) -> f32 {
        self.px_size
    }

    /// Sets the font pixel size and clears cached glyphs.
    pub fn set_pixel_size(&mut self, px_size: f32) -> std::io::Result<()> {
        let px_int = (px_size.round() as u32).max(1);
        let (cell_w, cell_h, baseline) = Self::apply_pixel_size(
            &self.fonts,
            self.bold_primary.as_ref(),
            self.italic_primary.as_ref(),
            self.bold_italic_primary.as_ref(),
            px_int,
        )?;
        self.cell_w = cell_w;
        self.cell_h = cell_h;
        self.baseline = baseline;
        self.px_size = px_size;
        self.glyphs.clear();
        self.box_masks.clear();
        Ok(())
    }

    /// Returns `(underline_pos, underline_thickness, strikethrough_pos, strikethrough_thickness)` in pixel rows from the cell top.
    pub fn get_deco_metrics(&self) -> (u32, u32, u32, u32) {
        let cell_h = self.cell_h.max(1);
        let baseline = (self.baseline.max(0) as u32).min(cell_h.saturating_sub(1));
        let thickness = (self.px_size as u32 / 14).max(1);
        let descender_room = cell_h.saturating_sub(baseline);
        let underline_offset = (descender_room / 2).max(1);
        let underline_pos = (baseline + underline_offset)
            .min(cell_h.saturating_sub(thickness.div_ceil(2)).max(1));
        let strike_pos = (baseline as f32 * 0.65) as u32;
        let strike_pos = strike_pos.max(thickness).min(baseline.saturating_sub(1).max(1));
        (underline_pos, thickness, strike_pos, thickness)
    }

    /// Returns the [`Glyph`] for `c` with the given `bold` and `italic` flags.
    pub fn get_glyph(&mut self, c: char, bold: bool, italic: bool) -> &Glyph {
        let key = (c, bold, italic);
        if !self.glyphs.contains_key(&key) {
            let bold_italic_match = self
                .bold_italic_primary
                .as_ref()
                .filter(|f| f.get_char_index(c as usize).is_some());
            let italic_match = self
                .italic_primary
                .as_ref()
                .filter(|f| f.get_char_index(c as usize).is_some());
            let bold_match = self
                .bold_primary
                .as_ref()
                .filter(|f| f.get_char_index(c as usize).is_some());
            let regular_match = self
                .fonts
                .iter()
                .find(|f| f.get_char_index(c as usize).is_some())
                .unwrap_or(&self.fonts[0]);
            let (font, used_bold, used_italic): (&Face, bool, bool) = match (bold, italic) {
                (true, true) => {
                    if let Some(f) = bold_italic_match {
                        (f, true, true)
                    } else if let Some(f) = italic_match {
                        (f, false, true)
                    } else if let Some(f) = bold_match {
                        (f, true, false)
                    } else {
                        (regular_match, false, false)
                    }
                }
                (true, false) => {
                    if let Some(f) = bold_match {
                        (f, true, false)
                    } else {
                        (regular_match, false, false)
                    }
                }
                (false, true) => {
                    if let Some(f) = italic_match {
                        (f, false, true)
                    } else {
                        (regular_match, false, false)
                    }
                }
                (false, false) => (regular_match, false, false),
            };
            let (bitmap, width, height, x_off, y_off) = Self::rasterize(font, c, self.baseline);
            self.glyphs.insert(
                key,
                Glyph {
                    bitmap,
                    width,
                    height,
                    x_off,
                    y_off,
                    synth_bold: bold && !used_bold,
                    synth_italic: italic && !used_italic,
                },
            );
        }
        &self.glyphs[&key]
    }
}
