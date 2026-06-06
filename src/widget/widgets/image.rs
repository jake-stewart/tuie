//! Image widget.

use crate::prelude::*;

pub use crate::render::image::config;
pub use crate::render::image::{ImageConfig, ImageProtocol, ImageSource, ImageSourceError};

/// Widget that renders an [`ImageSource`].
pub struct Image {
    layout: Layout,
    source: ImageSource,
    fill: bool,
}

impl Image {
    fn get_aspect_cells(&self) -> f64 {
        let px = self.source.get_pixel_dims();
        if px.x == 0 || px.y == 0 {
            return 1.0;
        }
        let cell_px = crate::runtime::get_terminal_info()
            .cell_size
            .unwrap_or(Vec2::new(1u16, 2u16));
        let numerator = px.x as f64 * cell_px.y as f64;
        let denominator = (px.y as f64 * cell_px.x as f64).max(1.0);
        numerator / denominator
    }

    fn get_intrinsic_size(&self, bound: Vec2<u16>) -> Vec2<u16> {
        let min = Axis2D::map(|a| self.layout.get_explicit_min(a).unwrap_or(0));
        let max = Axis2D::map(|a| self.layout.get_explicit_max(a).unwrap_or(u16::MAX));
        let upper = Axis2D::map(|a| max[a].min(bound[a]));
        let exact = Axis2D::map(|a| {
            let mn = self.layout.explicit_min[a];
            mn.is_some() && mn == self.layout.explicit_max[a]
        });
        let aspect = self.get_aspect_cells();
        match (exact.x, exact.y) {
            (true, _) => {
                let width = min.x.min(bound.x);
                let height = ((width as f64 / aspect).round() as u16).max(min.y).min(upper.y);
                Vec2::new(width, height)
            }
            (false, true) => {
                let height = min.y.min(bound.y);
                let width = ((height as f64 * aspect).round() as u16).max(min.x).min(upper.x);
                Vec2::new(width, height)
            }
            (false, false) if upper.x == u16::MAX && upper.y == u16::MAX => {
                let px = self.source.get_pixel_dims();
                let cell_px = crate::runtime::get_terminal_info()
                    .cell_size
                    .unwrap_or(Vec2::new(1u16, 2u16));
                let nat_x = (px.x / cell_px.x.max(1) as u32).clamp(1, u16::MAX as u32) as u16;
                let nat_y = (px.y / cell_px.y.max(1) as u32).clamp(1, u16::MAX as u32) as u16;
                Vec2::new(nat_x.max(min.x), nat_y.max(min.y))
            }
            (false, false) => {
                let fit = Self::aspect_fit(upper, aspect);
                Vec2::new(fit.x.max(min.x).min(upper.x), fit.y.max(min.y).min(upper.y))
            }
        }
    }

    fn aspect_fit(bound: Vec2<u16>, aspect: f64) -> Vec2<u16> {
        if bound.x == 0 || bound.y == 0 {
            return Vec2::of(0);
        }
        let (bx, by) = (bound.x as f64, bound.y as f64);
        let h_for_full_w = ((bx / aspect).round() as u16).max(1);
        let w_for_full_h = ((by * aspect).round() as u16).max(1);
        let cover_w_fits = h_for_full_w <= bound.y;
        let cover_h_fits = w_for_full_h <= bound.x;
        match (cover_w_fits, cover_h_fits) {
            (true, true) => {
                let area_w = bound.x as u32 * h_for_full_w as u32;
                let area_h = w_for_full_h as u32 * bound.y as u32;
                if area_w >= area_h {
                    Vec2::new(bound.x, h_for_full_w)
                } else {
                    Vec2::new(w_for_full_h, bound.y)
                }
            }
            (true, false) => Vec2::new(bound.x, h_for_full_w),
            (false, true) => Vec2::new(w_for_full_h, bound.y),
            (false, false) => {
                if bx / by > aspect {
                    Vec2::new(w_for_full_h.min(bound.x), bound.y)
                } else {
                    Vec2::new(bound.x, h_for_full_w.min(bound.y))
                }
            }
        }
    }
}

impl Widget for Image {
    fn get_layout(&self) -> &Layout {
        &self.layout
    }

    fn get_layout_mut(&mut self) -> &mut Layout {
        &mut self.layout
    }

    fn get_name(&self) -> &'static str {
        "Image"
    }

    fn measure_constraints(&mut self) -> Constraints {
        let intrinsic = self.get_intrinsic_size(Vec2::of(u16::MAX));
        Constraints {
            min_size: intrinsic,
            max_size: Vec2::of(u16::MAX),
            preferred_size: intrinsic,
        }
    }

    fn layout_flow(&mut self, allocated: Vec2<u16>) -> Vec2<u16> {
        let placement = self.get_intrinsic_size(allocated);
        crate::render::image::prepare(&self.source, placement, self.fill);
        placement
    }

    fn layout_measure(&self, allocated: Vec2<u16>) -> Vec2<u16> {
        self.get_intrinsic_size(allocated)
    }

    fn render(&self, mut ctx: RenderContext) {
        ctx.draw_image(&self.source, self.fill);
    }
}

impl Image {
    /// Creates an image widget that draws `source`.
    pub fn new(source: ImageSource) -> Box<Self> {
        Box::new(Self {
            layout: Layout::new(),
            source,
            fill: false,
        })
    }

    crate::style_field! {
        /// Whether the image stretches to fill the area instead of letterboxing.
        fill: bool
    }
}
