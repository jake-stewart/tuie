//! GUI backend.

mod box_drawing;
mod font;
mod gpu;
#[cfg(feature = "images")]
mod image_scan;

use crate::prelude::*;
use crate::render::{GridCellStyle, GridRenderer, grid_cell_style_to_style};
use crate::theme::Theme;
use font::FontCache;
use gpu::Backend;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, KeyEvent, MouseButton as WMouseButton, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key as WKey, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};
use std::sync::Arc;

/// How the window's title bar is presented.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TitleBar {
    /// Transparent title bar with a top pad reserved above the grid.
    Padding,
    /// Transparent title bar with no top pad.
    Overlap,
    /// OS-drawn title bar above the content.
    System,
}

impl Default for TitleBar {
    fn default() -> Self {
        TitleBar::Padding
    }
}

impl std::fmt::Display for TitleBar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Padding => write!(f, "Padding"),
            Self::Overlap => write!(f, "Overlap"),
            Self::System => write!(f, "System"),
        }
    }
}

/// GUI backend configuration.
#[derive(Clone, Copy)]
pub struct GuiConfig {
    /// The system font family name.
    pub font_family: Option<&'static str>,
    /// The font size in logical pixels.
    pub font_size: f32,
    /// Bundled font bytes, taking precedence over `font_family`.
    pub font_data: Option<&'static [u8]>,
    /// Family names tried in order for glyphs the primary font lacks.
    pub font_fallbacks: &'static [&'static str],
    /// The title-bar style.
    pub title_bar: TitleBar,
    /// Whether to extend edge-row backgrounds into the side padding.
    pub extend_sides: bool,
    /// Whether to mirror row 0's background into the top pad strip.
    pub extend_header: bool,
    /// Whether to mirror the last row's background into the bottom pad strip.
    pub extend_footer: bool,
    /// The horizontal padding in logical pixels on each side of the cell grid.
    pub horizontal_padding: u32,
    /// The vertical padding in logical pixels above and below the cell grid.
    pub vertical_padding: u32,
    /// Whether to center the grid in the padded window area.
    pub center_grid: bool,
    /// The style applied to the [`CursorShape::Block`] cell.
    pub cursor_style: Style,
    /// The theme applied when the active appearance is light.
    pub light_theme: Theme,
    /// The theme applied when the active appearance is dark.
    pub dark_theme: Theme,
    /// The forced color scheme, or `None` to follow the OS appearance.
    pub appearance: Option<ColorScheme>,
}

crate::config_module!(GuiConfig {
    font_family: None,
    font_size: 14.0,
    font_data: None,
    font_fallbacks: &[],
    title_bar: TitleBar::Padding,
    extend_sides: false,
    extend_header: false,
    extend_footer: false,
    horizontal_padding: 0,
    vertical_padding: 0,
    center_grid: false,
    cursor_style: Style::new().fg(Color::Background).bg(Color::Foreground),
    light_theme: Theme::CENTURY_LIGHT,
    dark_theme: Theme::CENTURY_DARK,
    appearance: None,
});

#[cfg(feature = "harmonious")]
fn apply_window_theme(win_theme: Option<winit::window::Theme>, cfg: &GuiConfig) {
    let scheme = cfg.appearance.unwrap_or_else(|| match win_theme {
        Some(winit::window::Theme::Light) => ColorScheme::Light,
        _ => ColorScheme::Dark,
    });
    let theme = match scheme {
        ColorScheme::Light => cfg.light_theme,
        ColorScheme::Dark => cfg.dark_theme,
    };
    crate::theme::harmonious::apply_palette(crate::theme::harmonious::Palette::from_theme(theme));
}

/// Re-applies the active GUI theme from [`GuiConfig`].
#[cfg(feature = "harmonious")]
pub fn reapply_theme() {
    crate::runtime::try_with_gui_state(|s| {
        let win_theme = s.window.as_ref().and_then(|w| w.theme());
        apply_window_theme(win_theme, &config::get());
    });
    crate::runtime::dirty_paint();
}

/// Sets the GUI font size in logical pixels.
pub fn set_font_size(px: f32) {
    config::update(|cfg| cfg.font_size = px);
    crate::runtime::try_with_gui_state(|s| {
        s.reload_font_if_needed();
        if let Some(backend) = s.backend.as_mut() {
            backend.clear_glyph_atlas();
        }
        s.relayout();
    });
}

/// Returns `(left, right)` cell-column counts to keep clear of OS title-bar chrome.
pub fn title_bar_insets() -> (u16, u16) {
    let cfg = config::get();
    if !matches!(cfg.title_bar, TitleBar::Overlap) {
        return (0, 0);
    }
    #[cfg(target_os = "macos")]
    {
        crate::runtime::try_with_gui_state(|s| {
            let cell_w = (s.font.get_cell_w() as u32).max(1);
            let traffic_light_px = 66u32 * s.scale.max(1);
            let cells = traffic_light_px.div_ceil(cell_w).min(u16::MAX as u32) as u16;
            (cells, 0u16)
        })
        .unwrap_or((0, 0))
    }
    #[cfg(not(target_os = "macos"))]
    {
        (0, 0)
    }
}

pub(crate) struct Gui {
    pub state: GuiState,
    pub event_loop: Option<EventLoop<()>>,
}

pub(crate) struct GuiState {
    backend: Option<Backend>,
    window: Option<Arc<Window>>,
    pub(crate) pixel_size: Vec2<u32>,
    pub(crate) cell_size: Vec2<u16>,
    scale: u32,
    font: FontCache,
    pub(crate) pending_events: Vec<RuntimeEvent>,
    modifiers: Modifiers,
    mouse_cell: Vec2<i32>,
    mouse_subpx: Vec2<i32>,
    held_buttons: u8,
    dragging: bool,
    last_click: std::time::Instant,
    click_pos: Vec2<i32>,
    click_count: u8,
    gpu_prebuilt: Option<(wgpu::Instance, wgpu::Adapter)>,
    cursor_blink_anchor: std::time::Instant,
    last_cursor_xy: Option<(i32, i32)>,
    pub(crate) next_blink_wake: Option<std::time::Instant>,
    focused: bool,
    pub(crate) exit_code: Option<u8>,
}

pub(crate) struct RunHandler {
    pub root: Box<dyn Widget>,
}

fn top_pad_px(scale: u32, title_bar: TitleBar) -> u32 {
    #[cfg(target_os = "macos")]
    {
        match title_bar {
            TitleBar::Padding => 28 * scale,
            TitleBar::Overlap | TitleBar::System => 0,
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (scale, title_bar);
        0
    }
}

impl GuiState {
    const BTN_LEFT: u8 = 1 << 0;
    const BTN_RIGHT: u8 = 1 << 1;
    const BTN_MIDDLE: u8 = 1 << 2;
    const BLINK_HALF_MS: u64 = 530;

    fn button_mask(btn: MouseButton) -> u8 {
        match btn {
            MouseButton::Left => Self::BTN_LEFT,
            MouseButton::Right => Self::BTN_RIGHT,
            MouseButton::Middle => Self::BTN_MIDDLE,
        }
    }

    fn cells_for(&self, pixel_size: Vec2<u32>) -> Vec2<u16> {
        let cfg = config::get();
        let h_pad = cfg.horizontal_padding * self.scale;
        let v_pad = cfg.vertical_padding * self.scale;
        let usable_w = pixel_size.x.saturating_sub(2 * h_pad);
        let usable_h = pixel_size
            .y
            .saturating_sub(top_pad_px(self.scale, cfg.title_bar))
            .saturating_sub(2 * v_pad);
        Vec2::new(
            (usable_w / self.font.get_cell_w()).min(u16::MAX as u32) as u16,
            (usable_h / self.font.get_cell_h()).min(u16::MAX as u32) as u16,
        )
    }

    pub(crate) fn grid_origin(&self) -> Vec2<u32> {
        let cfg = config::get();
        let header = top_pad_px(self.scale, cfg.title_bar);
        let h_pad = cfg.horizontal_padding * self.scale;
        let v_pad = cfg.vertical_padding * self.scale;
        let used_x = self.cell_size.x as u32 * self.font.get_cell_w();
        let used_y = self.cell_size.y as u32 * self.font.get_cell_h();
        let extra_x = self
            .pixel_size
            .x
            .saturating_sub(used_x)
            .saturating_sub(2 * h_pad);
        let extra_y = self
            .pixel_size
            .y
            .saturating_sub(header)
            .saturating_sub(used_y)
            .saturating_sub(2 * v_pad);
        if cfg.center_grid {
            Vec2::new(h_pad + extra_x / 2, header + v_pad + extra_y / 2)
        } else {
            Vec2::new(h_pad, header + v_pad)
        }
    }

    fn reload_font_if_needed(&mut self) {
        let cfg = config::get();
        let target = cfg.font_size * self.scale as f32;
        if (self.font.get_px_size() - target).abs() > f32::EPSILON {
            if let Err(e) = self.font.set_pixel_size(target) {
                eprintln!("tuie gui: font resize failed: {e}");
            }
        }
    }

    fn relayout(&mut self) {
        self.cell_size = self.cells_for(self.pixel_size);
        let origin = self.grid_origin();
        let pixel_size = self.pixel_size;
        if let Some(backend) = self.backend.as_mut() {
            backend.resize(pixel_size, origin);
        }
        crate::runtime::sync_gui_grid_size(self.cell_size, self.font_cell_px());
    }

    fn first_held_button(&self) -> Option<MouseButton> {
        for btn in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
            if self.held_buttons & Self::button_mask(btn) != 0 {
                return Some(btn);
            }
        }
        None
    }

    fn push_scroll(&mut self, axis: Axis2D, delta_cells: f32) {
        if delta_cells.abs() < f32::EPSILON {
            return;
        }
        let dir = match (axis, delta_cells > 0.0) {
            (Axis2D::Y, true) => Direction2D::Up,
            (Axis2D::Y, false) => Direction2D::Down,
            (Axis2D::X, true) => Direction2D::Left,
            (Axis2D::X, false) => Direction2D::Right,
        };
        self.pending_events.push(RuntimeEvent::Input(InputEvent {
            chord: Chord::new(Trigger::MouseSmoothScroll(dir, delta_cells.abs()), self.modifiers),
            pos: self.pack_mouse_pos(),
            count: 1,
        }));
    }

    fn pack_mouse_pos(&self) -> Vec2<f32> {
        if self.mouse_cell.x < 0 || self.mouse_cell.y < 0 {
            return Vec2::of(-1.0);
        }
        let cell_w = self.font.get_cell_w() as f32;
        let cell_h = self.font.get_cell_h() as f32;
        Vec2::new(
            self.mouse_cell.x as f32 + self.mouse_subpx.x as f32 / cell_w,
            self.mouse_cell.y as f32 + self.mouse_subpx.y as f32 / cell_h,
        )
    }
}

impl Gui {
    pub(crate) fn new() -> std::io::Result<Self> {
        let cfg = config::get();
        let need_fontdb = cfg.font_data.is_none();
        let fontdb_worker = std::thread::spawn(move || -> Option<fontdb::Database> {
            if !need_fontdb {
                return None;
            }
            let mut db = fontdb::Database::new();
            db.load_system_fonts();
            Some(db)
        });
        let gpu_worker = std::thread::spawn(gpu::build_instance_and_adapter);

        let event_loop = EventLoop::new()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        let db = fontdb_worker.join().expect("fontdb worker panicked");
        let (instance, adapter) = gpu_worker.join().expect("gpu worker panicked")?;
        let font = FontCache::new_with_db(&cfg, db, 1)?;

        let init_cells = Vec2::new(100u16, 30u16);
        Ok(Self {
            state: GuiState {
                backend: None,
                window: None,
                pixel_size: Vec2::new(
                    init_cells.x as u32 * font.get_cell_w(),
                    init_cells.y as u32 * font.get_cell_h(),
                ),
                cell_size: init_cells,
                scale: 1,
                font,
                pending_events: Vec::new(),
                modifiers: Modifiers::new(),
                mouse_cell: Vec2::of(-1),
                mouse_subpx: Vec2::of(-1),
                held_buttons: 0,
                dragging: false,
                last_click: std::time::Instant::now(),
                click_pos: Vec2::of(-1),
                click_count: 0,
                gpu_prebuilt: Some((instance, adapter)),
                cursor_blink_anchor: std::time::Instant::now(),
                last_cursor_xy: None,
                next_blink_wake: None,
                focused: true,
                exit_code: None,
            },
            event_loop: Some(event_loop),
        })
    }
}

impl RunHandler {
    pub(crate) fn new(root: Box<dyn Widget>) -> Self {
        Self { root }
    }

    fn tick(&mut self, el: &ActiveEventLoop) -> Option<std::time::Instant> {
        let events = crate::runtime::take_pending_gui_events();
        if let Some(code) = events.iter().find_map(|e| match e {
            RuntimeEvent::Quit(c) => Some(*c),
            _ => None,
        }) {
            crate::runtime::with_gui_state(|s| s.exit_code = Some(code));
            el.exit();
            return None;
        }
        let timeout = match crate::runtime::update(&mut *self.root, &events) {
            Ok(t) => t,
            Err(_) => {
                el.exit();
                return None;
            }
        };
        timeout.map(|d| std::time::Instant::now() + d)
    }
}

impl ApplicationHandler<()> for RunHandler {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        crate::runtime::with_gui_state(|s| s.resumed(el));
    }

    fn window_event(
        &mut self,
        el: &ActiveEventLoop,
        id: WindowId,
        event: WindowEvent,
    ) {
        crate::runtime::with_gui_state(|s| s.window_event(el, id, event));
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let pending_timeout = self.tick(el);
        if let Some(deadline) = pending_timeout {
            el.set_control_flow(ControlFlow::WaitUntil(deadline));
        } else {
            el.set_control_flow(ControlFlow::Wait);
        }
    }
}

fn resolve_color(color: Color) -> u32 {
    #[cfg(feature = "harmonious")]
    if let Some(rgb) = crate::theme::harmonious::resolve_rgb(color) {
        return pack_rgb(rgb.r, rgb.g, rgb.b);
    }
    match color {
        Color::Foreground => 0xFFD0D0D0,
        Color::Background => 0xFF101010,
        Color::Rgb(r, g, b) => pack_rgb(r, g, b),
        Color::Indexed(n) => fallback_ansi(n),
    }
}

fn pack_rgb(r: u8, g: u8, b: u8) -> u32 {
    0xFF000000 | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

fn mix_rgba(a: [u8; 4], b: [u8; 4]) -> [u8; 4] {
    [
        ((a[0] as u16 + b[0] as u16) / 2) as u8,
        ((a[1] as u16 + b[1] as u16) / 2) as u8,
        ((a[2] as u16 + b[2] as u16) / 2) as u8,
        a[3],
    ]
}

#[cfg(not(feature = "harmonious"))]
fn fallback_ansi(n: u8) -> u32 {
    static BASIC: [(u8, u8, u8); 16] = [
        (0, 0, 0),
        (170, 0, 0),
        (0, 170, 0),
        (170, 85, 0),
        (0, 0, 170),
        (170, 0, 170),
        (0, 170, 170),
        (170, 170, 170),
        (85, 85, 85),
        (255, 85, 85),
        (85, 255, 85),
        (255, 255, 85),
        (85, 85, 255),
        (255, 85, 255),
        (85, 255, 255),
        (255, 255, 255),
    ];
    let (r, g, b) = if (n as usize) < 16 {
        BASIC[n as usize]
    } else if n >= 232 {
        let v = (n - 232).saturating_mul(10).saturating_add(8);
        (v, v, v)
    } else {
        let cube = n - 16;
        let r = (cube / 36) * 51;
        let g = ((cube % 36) / 6) * 51;
        let b = (cube % 6) * 51;
        (r, g, b)
    };
    pack_rgb(r, g, b)
}

#[cfg(feature = "harmonious")]
fn fallback_ansi(_n: u8) -> u32 {
    0xFF000000
}

impl ApplicationHandler for GuiState {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let cfg = config::get();
        let attrs = Window::default_attributes()
            .with_title("tuie")
            .with_inner_size(PhysicalSize::new(self.pixel_size.x, self.pixel_size.y));
        #[cfg(target_os = "macos")]
        let attrs = {
            use winit::platform::macos::WindowAttributesExtMacOS;
            match cfg.title_bar {
                TitleBar::Padding | TitleBar::Overlap => attrs
                    .with_titlebar_transparent(true)
                    .with_fullsize_content_view(true)
                    .with_title_hidden(true),
                TitleBar::System => attrs,
            }
        };
        let initial_origin = Vec2::new(0u32, top_pad_px(1, cfg.title_bar));
        let prebuilt = self.gpu_prebuilt.take();
        let (window, mut backend) = match gpu::create_window_and_backend(event_loop, attrs, initial_origin, prebuilt) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("tuie gui: failed to create GPU backend: {e}");
                event_loop.exit();
                return;
            }
        };
        self.scale = window.scale_factor().round().max(1.0) as u32;
        self.reload_font_if_needed();
        let inner = window.inner_size();
        self.pixel_size = Vec2::new(inner.width.max(1), inner.height.max(1));
        self.cell_size = self.cells_for(self.pixel_size);
        backend.resize(self.pixel_size, self.grid_origin());
        #[cfg(feature = "harmonious")]
        apply_window_theme(window.theme(), &cfg);
        self.window = Some(window);
        self.backend = Some(backend);
        crate::runtime::sync_gui_grid_size(self.cell_size, self.font_cell_px());
        crate::runtime::dirty_layout();
        self.pending_events.push(RuntimeEvent::Focus(true));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                self.exit_code = Some(0);
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                self.pixel_size = Vec2::new(new_size.width.max(1), new_size.height.max(1));
                self.relayout();
                crate::runtime::dirty_paint();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let new_scale = scale_factor.round().max(1.0) as u32;
                if new_scale != self.scale {
                    self.scale = new_scale;
                    self.reload_font_if_needed();
                    self.relayout();
                }
            }
            WindowEvent::Focused(f) => {
                self.focused = f;
                self.cursor_blink_anchor = std::time::Instant::now();
                self.pending_events.push(RuntimeEvent::Focus(f));
                crate::runtime::dirty_paint();
            }
            #[cfg(feature = "harmonious")]
            WindowEvent::ThemeChanged(t) => {
                apply_window_theme(Some(t), &config::get());
                crate::runtime::dirty_paint();
            }
            WindowEvent::ModifiersChanged(m) => {
                self.modifiers = winit_modifiers_to_tuie(m.state());
            }
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                if key_event.state == ElementState::Pressed {
                    if let Some(chord) = winit_key_to_chord(&key_event, self.modifiers) {
                        self.cursor_blink_anchor = std::time::Instant::now();
                        crate::runtime::dirty_paint();
                        self.pending_events.push(RuntimeEvent::Input(InputEvent {
                            chord,
                            pos: self.pack_mouse_pos(),
                            count: 1,
                        }));
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let origin = self.grid_origin();
                let x_in_grid = position.x as i32 - origin.x as i32;
                let y_in_grid = position.y as i32 - origin.y as i32;
                let cell_w = self.font.get_cell_w() as i32;
                let cell_h = self.font.get_cell_h() as i32;
                let (cell_x, sub_x) = if x_in_grid < 0 {
                    (-1, -1)
                } else {
                    (x_in_grid / cell_w, x_in_grid % cell_w)
                };
                let (cell_y, sub_y) = if y_in_grid < 0 {
                    (-1, -1)
                } else {
                    (y_in_grid / cell_h, y_in_grid % cell_h)
                };
                let new_cell = Vec2::new(cell_x, cell_y);
                let new_subpx = Vec2::new(sub_x, sub_y);
                let prev_half_x = self.mouse_cell.x * 2
                    + if self.mouse_subpx.x >= cell_w / 2 { 1 } else { 0 };
                let new_half_x = cell_x * 2 + if sub_x >= cell_w / 2 { 1 } else { 0 };
                let cell_changed =
                    new_half_x != prev_half_x || new_cell.y != self.mouse_cell.y;
                let subpx_changed = new_subpx != self.mouse_subpx;
                let held = self.first_held_button();
                self.mouse_subpx = new_subpx;
                self.mouse_cell = new_cell;
                let should_emit = cell_changed || (held.is_some() && subpx_changed);
                if should_emit {
                    let (trigger, count) = if let Some(btn) = held {
                        if !self.dragging {
                            self.dragging = true;
                            self.pending_events.push(RuntimeEvent::DragHold(true));
                        }
                        (Trigger::MouseDrag(btn), self.click_count)
                    } else {
                        (Trigger::MouseHover, 1)
                    };
                    self.pending_events.push(RuntimeEvent::Input(InputEvent {
                        chord: Chord::new(trigger, self.modifiers),
                        pos: self.pack_mouse_pos(),
                        count,
                    }));
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let btn = match button {
                    WMouseButton::Left => MouseButton::Left,
                    WMouseButton::Right => MouseButton::Right,
                    WMouseButton::Middle => MouseButton::Middle,
                    _ => return,
                };
                let mask = Self::button_mask(btn);
                let trigger = match state {
                    ElementState::Pressed => {
                        self.held_buttons |= mask;
                        let now = std::time::Instant::now();
                        let is_multi = self.click_pos == self.mouse_cell
                            && now.duration_since(self.last_click)
                                < crate::runtime::event::RuntimeEventReader::REPEAT_WINDOW;
                        if is_multi {
                            self.click_count = self.click_count
                                % crate::runtime::event::RuntimeEventReader::MAX_REPEAT_COUNT
                                + 1;
                        } else {
                            self.click_count = 1;
                        }
                        self.click_pos = self.mouse_cell;
                        self.last_click = now;
                        Trigger::MouseDown(btn)
                    }
                    ElementState::Released => {
                        self.held_buttons &= !mask;
                        if self.dragging && self.held_buttons == 0 {
                            self.dragging = false;
                            self.pending_events.push(RuntimeEvent::DragHold(false));
                        }
                        Trigger::MouseUp(btn)
                    }
                };
                self.pending_events.push(RuntimeEvent::Input(InputEvent {
                    chord: Chord::new(trigger, self.modifiers),
                    pos: self.pack_mouse_pos(),
                    count: self.click_count,
                }));
            }
            WindowEvent::MouseWheel { delta, phase, .. } => {
                match phase {
                    TouchPhase::Started => {
                        if !self.dragging {
                            self.dragging = true;
                            self.pending_events.push(RuntimeEvent::DragHold(true));
                        }
                    }
                    TouchPhase::Ended | TouchPhase::Cancelled => {
                        if self.dragging && self.held_buttons == 0 {
                            self.dragging = false;
                            self.pending_events.push(RuntimeEvent::DragHold(false));
                        }
                    }
                    TouchPhase::Moved => {}
                }
                let delta_px = match delta {
                    MouseScrollDelta::LineDelta(x, y) => Vec2::new(
                        x * self.font.get_cell_w() as f32,
                        y * self.font.get_cell_h() as f32,
                    ),
                    MouseScrollDelta::PixelDelta(p) => {
                        Vec2::new(p.x as f32 * 0.5, p.y as f32 * 0.5)
                    }
                };
                let cell_w = self.font.get_cell_w().max(1) as f32;
                let cell_h = self.font.get_cell_h().max(1) as f32;
                self.push_scroll(Axis2D::Y, delta_px.y / cell_h);
                self.push_scroll(Axis2D::X, delta_px.x / cell_w);
            }
            _ => {}
        }
    }
}

impl GuiState {
    pub(crate) fn font_cell_px(&self) -> Vec2<u16> {
        Vec2::new(self.font.get_cell_w() as u16, self.font.get_cell_h() as u16)
    }

    pub(crate) fn present(
        &mut self,
        renderer: &mut crate::render::GridRenderer,
        cursor: Option<(CursorShape, Vec2<i32>)>,
        cursor_subpixel_px: Vec2<i32>,
        focus_chain: &[WidgetId],
        raw_writer: &mut dyn std::io::Write,
    ) {
        let grid_origin = self.grid_origin();
        let Some(backend) = self.backend.as_mut() else {
            return;
        };
        let cfg = config::get();
        let body_clear = resolve_color(Color::Background);
        backend.begin_frame(
            body_clear,
            self.cell_size,
            cfg.extend_sides,
            cfg.extend_header,
            cfg.extend_footer,
        );
        let body_clear_rgba = gpu::u32_to_rgba(body_clear);
        let cursor_shape = cursor.map(|(s, _)| s).unwrap_or(CursorShape::Block);
        let cursor_xy: Option<(i32, i32)> = cursor.map(|(_shape, pos)| (pos.x, pos.y));

        if cursor_xy != self.last_cursor_xy {
            self.cursor_blink_anchor = std::time::Instant::now();
            self.last_cursor_xy = cursor_xy;
        }
        let cursor_xy = if self.focused
            && crate::runtime::config::get().cursor_blink
            && cursor_xy.is_some()
        {
            let elapsed = self.cursor_blink_anchor.elapsed().as_millis() as u64;
            let phase = elapsed / Self::BLINK_HALF_MS;
            self.next_blink_wake = Some(
                self.cursor_blink_anchor
                    + std::time::Duration::from_millis((phase + 1) * Self::BLINK_HALF_MS),
            );
            if phase % 2 == 0 {
                cursor_xy
            } else {
                None
            }
        } else {
            self.next_blink_wake = None;
            cursor_xy
        };

        let font = &mut self.font;
        let scratch_bounds = Vec2::new(
            self.cell_size.x.saturating_add(2),
            self.cell_size.y.saturating_add(2),
        );
        let cursor_overlay = render_grid_pass(
            backend,
            renderer,
            font,
            scratch_bounds,
            body_clear_rgba,
            cursor_xy,
        );
        let defer_cursor = cursor_subpixel_px.x != 0 || cursor_subpixel_px.y != 0;
        let cursor_style = cfg.cursor_style;
        let deferred = defer_cursor.then_some(DeferredCursor {
            shape: cursor_shape,
            style: cursor_style,
            focused: self.focused,
            xy: cursor_xy,
            subpixel_px: cursor_subpixel_px,
        });
        if !defer_cursor {
            let cursor_xy_grid = cursor_xy.and_then(|(cx, cy)| {
                if cx >= 0 && cy >= 0 {
                    Some((cx as u16, cy as u16))
                } else {
                    None
                }
            });
            paint_cursor_overlay(
                backend,
                font,
                cursor_shape,
                cursor_xy_grid,
                cursor_overlay.clone(),
                cursor_style,
                self.focused,
            );
        }
        let cell_px = Vec2::new(font.get_cell_w(), font.get_cell_h());
        let root_pad = Vec2::new(grid_origin.x as i32, grid_origin.y as i32);
        backend.flush_passes(cell_px, root_pad, true, None, 1.0);

        let cell_px_i32 = Vec2::new(font.get_cell_w() as i32, font.get_cell_h() as i32);
        let mut cursor_clip_slot: Option<(Vec2<i32>, Vec2<u32>)> = None;
        let mut cursor_overlay_slot: Option<(String, GridCellStyle, bool)> = cursor_overlay
            .as_ref()
            .map(|(_, _, g, s, w)| (g.clone(), *s, *w));
        drain_offset_entries(
            backend,
            renderer,
            font,
            raw_writer,
            cell_px,
            cell_px_i32,
            root_pad,
            self.cell_size,
            body_clear_rgba,
            focus_chain,
            cursor_xy,
            &mut cursor_clip_slot,
            &mut cursor_overlay_slot,
            deferred,
        );

        backend.end_frame();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn render_grid_pass(
    backend: &mut Backend,
    renderer: &GridRenderer,
    font: &mut FontCache,
    bounds: Vec2<u16>,
    body_clear_rgba: [u8; 4],
    cursor_xy: Option<(i32, i32)>,
) -> Option<(u16, u16, String, GridCellStyle, bool)> {
    use crate::render::style::StyleAttribute;
    let fg_default_rgba = gpu::u32_to_rgba(resolve_color(Color::Foreground));
    let mut last_fg = (Color::Foreground, fg_default_rgba);
    let mut last_bg = (Color::Background, body_clear_rgba);
    let mut last_uc = (Color::Foreground, fg_default_rgba);
    let mut cursor_overlay: Option<(u16, u16, String, GridCellStyle, bool)> = None;
    #[cfg(feature = "images")]
    let mut scanner = image_scan::PlaceholderScanner::new();
    renderer.gui_for_each_cell(bounds, |x, y, glyph, style, wide| {
        #[cfg(feature = "images")]
        let is_placeholder = scanner.feed(backend, x, y, glyph, style.fg, style.underline_color);
        #[cfg(not(feature = "images"))]
        let is_placeholder = false;
        let fg_rgba = if style.fg == last_fg.0 {
            last_fg.1
        } else {
            let v = gpu::u32_to_rgba(resolve_color(style.fg));
            last_fg = (style.fg, v);
            v
        };
        let bg_rgba = if style.bg == last_bg.0 {
            last_bg.1
        } else {
            let v = gpu::u32_to_rgba(resolve_color(style.bg));
            last_bg = (style.bg, v);
            v
        };
        let underline_rgba = if style.underline_color == Color::Foreground {
            fg_rgba
        } else if style.underline_color == last_uc.0 {
            last_uc.1
        } else {
            let v = gpu::u32_to_rgba(resolve_color(style.underline_color));
            last_uc = (style.underline_color, v);
            v
        };
        let mut fg_rgba = fg_rgba;
        let mut bg_rgba = bg_rgba;
        if style.attrs & StyleAttribute::Dim as u8 != 0 {
            fg_rgba = mix_rgba(fg_rgba, bg_rgba);
        }
        if style.has_reverse() {
            std::mem::swap(&mut fg_rgba, &mut bg_rgba);
        }
        let cell = gpu::CellRender {
            fg_rgba,
            bg_rgba,
            body_clear_rgba,
            underline_rgba,
            wide,
            bold: style.attrs & StyleAttribute::Bold as u8 != 0,
            italic: style.attrs & StyleAttribute::Italic as u8 != 0,
            strikethrough: style.attrs & StyleAttribute::Strikethrough as u8 != 0,
            underline: style.underline,
        };
        if is_placeholder {
            if cell.bg_rgba != cell.body_clear_rgba {
                backend.push_cell(x, y, "", &cell, font);
            }
            return;
        }
        backend.push_cell(x, y, glyph, &cell, font);
        if let Some((cx, cy)) = cursor_xy {
            if cx == x as i32 && cy == y as i32 {
                cursor_overlay = Some((x, y, glyph.to_string(), *style, wide));
            }
        }
    });
    #[cfg(feature = "images")]
    scanner.end_frame(backend);
    cursor_overlay
}

fn paint_cursor_overlay(
    backend: &mut Backend,
    font: &mut FontCache,
    shape: CursorShape,
    cursor_xy: Option<(u16, u16)>,
    overlay: Option<(u16, u16, String, GridCellStyle, bool)>,
    cursor_style: Style,
    focused: bool,
) {
    if matches!(shape, CursorShape::Block) && !focused {
        let Some((cx, cy)) = cursor_xy else {
            return;
        };
        let fg_rgba = gpu::u32_to_rgba(resolve_color(Color::Foreground));
        backend.push_decoration(cx, cy, gpu::DecoStyle::CursorBlockOutline, fg_rgba, font);
        return;
    }

    let (cx, cy, glyph, underlying, wide) = if let Some((cx, cy, g, s, w)) = overlay.as_ref() {
        (*cx, *cy, g.as_str(), Some(*s), *w)
    } else if let Some((cx, cy)) = cursor_xy {
        (cx, cy, "", None, false)
    } else {
        return;
    };
    let underlying_style = underlying
        .as_ref()
        .map(grid_cell_style_to_style)
        .unwrap_or_else(Style::new);
    let merged = underlying_style.apply(cursor_style);
    let reversed = cursor_style.has_reverse();

    let fg_color = merged.get_fg().unwrap_or(Color::Foreground);
    let bg_color = merged.get_bg().unwrap_or(Color::Background);
    let mut fg_rgba = gpu::u32_to_rgba(resolve_color(fg_color));
    let mut bg_rgba = gpu::u32_to_rgba(resolve_color(bg_color));
    if merged.has_dim() {
        fg_rgba = mix_rgba(fg_rgba, bg_rgba);
    }
    if reversed {
        std::mem::swap(&mut fg_rgba, &mut bg_rgba);
    }

    match shape {
        CursorShape::Block => {
            let underline_rgba = match merged.get_underline_color() {
                Some(c) => gpu::u32_to_rgba(resolve_color(c)),
                None => fg_rgba,
            };
            let force_clear = [bg_rgba[0] ^ 0x01, bg_rgba[1], bg_rgba[2], bg_rgba[3]];
            let cell = gpu::CellRender {
                fg_rgba,
                bg_rgba,
                body_clear_rgba: force_clear,
                underline_rgba,
                wide,
                bold: merged.has_bold(),
                italic: merged.has_italic(),
                strikethrough: merged.has_strikethrough(),
                underline: merged.get_underline().unwrap_or(UnderlineType::None),
            };
            backend.push_cell(cx, cy, glyph, &cell, font);
        }
        CursorShape::Beam | CursorShape::Underline => {
            let style = if matches!(shape, CursorShape::Beam) {
                gpu::DecoStyle::CursorBeam
            } else {
                gpu::DecoStyle::CursorUnderline
            };
            backend.push_decoration(cx, cy, style, bg_rgba, font);
        }
    }
}

/// A cursor deferred to a later layer boundary.
#[derive(Clone, Copy)]
struct DeferredCursor {
    shape: CursorShape,
    style: Style,
    focused: bool,
    xy: Option<(i32, i32)>,
    subpixel_px: Vec2<i32>,
}

fn paint_deferred_cursor(
    backend: &mut Backend,
    font: &mut FontCache,
    dc: &DeferredCursor,
    root_pad: Vec2<i32>,
    cell_px: Vec2<u32>,
    clip: Option<(Vec2<i32>, Vec2<u32>)>,
    overlay: Option<(String, GridCellStyle, bool)>,
) {
    let Some((cx, cy)) = dc.xy else {
        return;
    };
    backend.begin_offset_pass(Vec2::new(1, 1));
    let deferred_overlay = overlay.map(|(glyph, s, w)| (0u16, 0u16, glyph, s, w));
    paint_cursor_overlay(backend, font, dc.shape, Some((0, 0)), deferred_overlay, dc.style, dc.focused);
    let cursor_screen_px = root_pad
        + Vec2::new(cx * cell_px.x as i32, cy * cell_px.y as i32)
        + dc.subpixel_px;
    backend.flush_passes(cell_px, cursor_screen_px, false, clip, 1.0);
}

#[allow(clippy::too_many_arguments)]
fn drain_offset_entries(
    backend: &mut Backend,
    renderer: &mut GridRenderer,
    font: &mut FontCache,
    raw_writer: &mut dyn std::io::Write,
    cell_px: Vec2<u32>,
    cell_px_i32: Vec2<i32>,
    root_pad: Vec2<i32>,
    root_cells: Vec2<u16>,
    body_clear_rgba: [u8; 4],
    focus_chain: &[WidgetId],
    root_cursor_xy: Option<(i32, i32)>,
    cursor_clip_slot: &mut Option<(Vec2<i32>, Vec2<u32>)>,
    cursor_overlay_slot: &mut Option<(String, GridCellStyle, bool)>,
    deferred: Option<DeferredCursor>,
) {
    let mut cursor_painted = false;
    loop {
        let Some(entry) = renderer.pop_defer_entry() else {
            break;
        };
        if let Some(dc) = deferred {
            if !cursor_painted
                && cursor_clip_slot.is_some()
                && !matches!(entry.kind, crate::render::Kind::Offset { .. })
            {
                paint_deferred_cursor(
                    backend,
                    font,
                    &dc,
                    root_pad,
                    cell_px,
                    *cursor_clip_slot,
                    cursor_overlay_slot.clone(),
                );
                cursor_painted = true;
            }
        }
        let phys_size: Vec2<u16>;
        let screen_pos_px: Vec2<i32>;
        let scissor: Option<(Vec2<i32>, Vec2<u32>)>;
        let bg_alpha: f32;
        let mut local_cursor_xy: Option<(u16, u16)> = None;
        match &entry.kind {
            crate::render::Kind::Offset {
                viewport_size_cells,
                content_offset_cells,
                subcell_offset_px,
                ..
            } => {
                let viewport_cell_px_offset = Vec2::new(
                    entry.cell_pos_in_parent.x * cell_px_i32.x,
                    entry.cell_pos_in_parent.y * cell_px_i32.y,
                );
                let viewport_screen_px =
                    entry.parent_screen_pos_px + viewport_cell_px_offset;
                let content_cell_px_offset = Vec2::new(
                    content_offset_cells.x * cell_px_i32.x,
                    content_offset_cells.y * cell_px_i32.y,
                );
                screen_pos_px =
                    viewport_screen_px + content_cell_px_offset + *subcell_offset_px;
                let viewport_size_px = Vec2::new(
                    viewport_size_cells.x as u32 * cell_px.x,
                    viewport_size_cells.y as u32 * cell_px.y,
                );
                let own_clip = (viewport_screen_px, viewport_size_px);
                let resolved_clip = match entry.parent_clip_screen_px {
                    Some(parent) => crate::render::intersect_clip(own_clip, parent),
                    None => own_clip,
                };
                scissor = Some(resolved_clip);
                // SAFETY: see `QueuedEntry`'s safety invariant (same as the
                // deref below). `entry.widget` is live for the duration of
                // this present pass.
                let entry_id = unsafe { &*entry.widget }.get_id();
                let in_sel = focus_chain.iter().any(|id| *id == entry_id);
                if in_sel {
                    let content_min = entry.snapshot.anchor;
                    let content_max_x = content_min.x + entry.snapshot.physical_size.x as i32;
                    let content_max_y = content_min.y + entry.snapshot.physical_size.y as i32;
                    let in_cells = root_cursor_xy.is_some_and(|(cx, cy)| {
                        cx >= content_min.x && cx < content_max_x
                            && cy >= content_min.y && cy < content_max_y
                    });
                    if in_cells {
                        let mut cur_clip = resolved_clip;
                        cur_clip.1.x = cur_clip.1.x.saturating_add(cell_px.x);
                        *cursor_clip_slot = Some(match *cursor_clip_slot {
                            Some(existing) => crate::render::intersect_clip(existing, cur_clip),
                            None => cur_clip,
                        });
                    }
                }
                if let Some((cx, cy)) = root_cursor_xy {
                    let target_x = root_pad.x + cx * cell_px_i32.x;
                    let target_y = root_pad.y + cy * cell_px_i32.y;
                    let origin_x = screen_pos_px.x - subcell_offset_px.x;
                    let origin_y = screen_pos_px.y - subcell_offset_px.y;
                    let dx = target_x - origin_x;
                    let dy = target_y - origin_y;
                    if dx >= 0 && dy >= 0 {
                        let lx = dx / cell_px_i32.x;
                        let ly = dy / cell_px_i32.y;
                        if lx < entry.snapshot.physical_size.x as i32
                            && ly < entry.snapshot.physical_size.y as i32
                        {
                            local_cursor_xy = Some((lx as u16, ly as u16));
                        }
                    }
                }
            }
            crate::render::Kind::Z
            | crate::render::Kind::Layer
            | crate::render::Kind::Popup => {
                let cell_px_offset = Vec2::new(
                    entry.cell_pos_in_parent.x * cell_px_i32.x,
                    entry.cell_pos_in_parent.y * cell_px_i32.y,
                );
                screen_pos_px = entry.parent_screen_pos_px + cell_px_offset;
                scissor = entry.parent_clip_screen_px;
            }
        }
        // SAFETY: see `QueuedEntry`'s safety invariant. `entry.widget` was
        // captured at queue time from a live `&dyn Widget`. The queue is
        // cleared per paint and fully drained inside this present pass, and
        // nothing mutates the widget tree between push and drain.
        let widget: &dyn Widget = unsafe { &*entry.widget };
        bg_alpha = widget.get_style().get_blend().unwrap_or(100) as f32 / 100.0;
        phys_size = entry.snapshot.physical_size;
        let (is_layer, mut pass_scissor) = match entry.kind {
            crate::render::Kind::Layer => (true, scissor),
            crate::render::Kind::Popup => (false, None),
            _ => (false, scissor),
        };
        renderer.render_defer_entry(widget, entry, screen_pos_px, scissor, raw_writer);
        backend.begin_offset_pass(phys_size);
        if is_layer {
            let cfg = config::get();
            let layer_w = phys_size.x as i32 * cell_px_i32.x;
            let layer_h = phys_size.y as i32 * cell_px_i32.y;
            let grid_w = root_cells.x as i32 * cell_px_i32.x;
            let grid_h = root_cells.y as i32 * cell_px_i32.y;
            let full_w = screen_pos_px.x <= root_pad.x && screen_pos_px.x + layer_w >= root_pad.x + grid_w;
            let extend_sides = cfg.extend_sides && full_w;
            let extend_header = cfg.extend_header && full_w && screen_pos_px.y <= root_pad.y;
            let extend_footer = cfg.extend_footer && full_w && screen_pos_px.y + layer_h >= root_pad.y + grid_h;
            if extend_sides || extend_header || extend_footer {
                backend.set_pass_extend(extend_sides, extend_header, extend_footer);
                pass_scissor = None;
            }
        }
        let captured = render_grid_pass(
            backend,
            renderer,
            font,
            phys_size,
            body_clear_rgba,
            local_cursor_xy.map(|(x, y)| (x as i32, y as i32)),
        );
        if let Some((_, _, glyph, style, wide)) = captured {
            *cursor_overlay_slot = Some((glyph, style, wide));
        }
        backend.flush_passes(cell_px, screen_pos_px, is_layer, pass_scissor, bg_alpha);
    }
    if let Some(dc) = deferred {
        if !cursor_painted {
            paint_deferred_cursor(
                backend,
                font,
                &dc,
                root_pad,
                cell_px,
                *cursor_clip_slot,
                cursor_overlay_slot.clone(),
            );
        }
    }
}

fn winit_modifiers_to_tuie(m: ModifiersState) -> Modifiers {
    let mut mods = Modifiers::new();
    mods.set(Modifier::Shift, m.shift_key());
    mods.set(Modifier::Ctrl, m.control_key());
    mods.set(Modifier::Alt, m.alt_key());
    mods.set(Modifier::Super, m.super_key());
    mods
}

fn winit_key_to_chord(event: &KeyEvent, modifiers: Modifiers) -> Option<Chord> {
    let key = match &event.logical_key {
        WKey::Named(named) => match named {
            NamedKey::Enter => Key::Enter,
            NamedKey::Escape => Key::Esc,
            NamedKey::Backspace => Key::Backspace,
            NamedKey::Tab => Key::Tab,
            NamedKey::ArrowUp => Key::Arrow(Direction2D::Up),
            NamedKey::ArrowDown => Key::Arrow(Direction2D::Down),
            NamedKey::ArrowLeft => Key::Arrow(Direction2D::Left),
            NamedKey::ArrowRight => Key::Arrow(Direction2D::Right),
            NamedKey::Home => Key::Home,
            NamedKey::End => Key::End,
            NamedKey::PageUp => Key::PageUp,
            NamedKey::PageDown => Key::PageDown,
            NamedKey::Delete => Key::Delete,
            NamedKey::Insert => Key::Insert,
            NamedKey::F1 => Key::F(1),
            NamedKey::F2 => Key::F(2),
            NamedKey::F3 => Key::F(3),
            NamedKey::F4 => Key::F(4),
            NamedKey::F5 => Key::F(5),
            NamedKey::F6 => Key::F(6),
            NamedKey::F7 => Key::F(7),
            NamedKey::F8 => Key::F(8),
            NamedKey::F9 => Key::F(9),
            NamedKey::F10 => Key::F(10),
            NamedKey::F11 => Key::F(11),
            NamedKey::F12 => Key::F(12),
            NamedKey::Space => Key::Char(' '),
            _ => return None,
        },
        WKey::Character(s) => {
            let c = s.chars().next()?;
            Key::Char(c)
        }
        _ => return None,
    };
    let mut mods = modifiers;
    if let Key::Char(_) = key {
        mods.set(Modifier::Shift, false);
    }
    Some(Chord::new(Trigger::Key(key), mods))
}
