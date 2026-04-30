use std::{path::Path, sync::OnceLock};

use crate::img::Image;
use fontdue::{Font, FontSettings};
use pixels::{Pixels, PixelsBuilder, SurfaceTexture, wgpu::Color};
use winit::{
    dpi::{LogicalSize, PhysicalSize},
    window::Window,
};

static FONT: OnceLock<Font> = OnceLock::new();

/// Transparent/background color.
const CLEAR_COLOR: Color = Color {
    r: 0.01,
    g: 0.01,
    b: 0.01,
    a: 1.00,
};

const PAN_STEP: f32 = 0.1; // Percent of dimension
const ZOOM_STEP: f32 = 0.1;
const MIN_ZOOM: f32 = 0.5;

pub struct ViewOpts {
    pub show_label: bool,
    pub label: String,

    /// If `true`, the window will be resized to fit the image.
    /// If `false`, the image will be resized to fit the window.
    ///
    /// Note that resizing the window to fit the image can mess up
    /// the window positioning if it's already been positioned by the WM.
    pub resize_window: bool,
    pub max_side: Option<u32>,
}

pub struct ImageView {
    /// Current zoom level.
    ///
    /// We already scale down the image if needed to fit,
    /// so we limit the minimum zoom to 1.0.
    zoom: f32,

    /// How the image is panned in the view, center-anchored.
    pan: (i32, i32),

    /// What we draw the image to.
    pixels: Pixels,

    /// The (source) image we're displaying.
    pub image: Image,

    /// If the image is zoomed, we cache the scaled image here.
    scaled: Option<Image>,

    label: String,
    show_label: bool,
}
impl ImageView {
    pub fn new(image_path: &Path, window: &Window, opts: ViewOpts) -> anyhow::Result<Self> {
        let mon = window
            .current_monitor()
            .or_else(|| window.available_monitors().next())
            .unwrap();
        let scale_factor = mon.scale_factor();

        let mon_size = mon.size();
        let max_bounds = match opts.max_side {
            Some(side) => {
                let phys_side = (side as f64 * scale_factor).round() as u32;
                (phys_side, phys_side)
            }
            None => (mon_size.width, mon_size.height),
        };

        let image = crate::img::read_image(image_path, max_bounds)?;
        let (mut width, mut height) = image.size();

        if opts.resize_window {
            let size = PhysicalSize::new(width as f64, height as f64);
            let size = LogicalSize::<f64>::from_physical(size, scale_factor);
            window
                .request_inner_size(size)
                .ok_or(anyhow::Error::msg("Failed to resize window"))?;

        // Fit this image to the existing window size.
        } else {
            let inner_size = window.inner_size();
            width = inner_size.width.max(1);
            height = inner_size.height.max(1);
        }

        let surface_texture = SurfaceTexture::new(width, height, &window);
        let pixels = PixelsBuilder::new(width, height, surface_texture)
            .clear_color(CLEAR_COLOR)
            .build()?;

        let mut view = Self {
            zoom: 1.,
            pan: (0, 0),
            pixels,
            image,
            scaled: None,
            label: opts.label,
            show_label: opts.show_label,
        };

        if !opts.resize_window {
            view.resize(width, height, true)?;
        } else {
            view.update();
        }

        Ok(view)
    }

    pub fn draw(&self) -> bool {
        self.pixels.render().is_ok()
    }

    /// Advance the image frame.
    pub fn advance(&mut self) -> bool {
        self.update();
        self.draw()
    }

    /// Write the current transformed image view to the texture.
    fn update(&mut self) {
        self.clamp_pan();
        let image = self.scaled.as_mut().unwrap_or(&mut self.image);
        view_buffer_window(&mut self.pixels, image, self.pan);

        if self.show_label {
            self.draw_label();
        }
    }

    pub fn resize(&mut self, width: u32, height: u32, fit_image: bool) -> anyhow::Result<()> {
        self.pixels.resize_surface(width, height)?;
        self.pixels.resize_buffer(width, height)?;

        if fit_image {
            let (w, h) = self.image.size();
            let zoom = (width as f32 / w as f32).min(height as f32 / h as f32);
            self.set_zoom(zoom);
        }
        Ok(())
    }

    pub fn zoom_in(&mut self) {
        self.set_zoom(self.zoom + ZOOM_STEP);
    }

    pub fn zoom_out(&mut self) {
        self.set_zoom((self.zoom - ZOOM_STEP).max(MIN_ZOOM));
    }

    fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom;
        self.scaled = Some(self.image.scaled(self.zoom));
        self.update();
        self.draw();
    }

    pub fn pan_up(&mut self) {
        let (_, height) = self.image_size();
        self.pan.1 -= (height as f32 * PAN_STEP).floor() as i32;
        self.update();
        self.draw();
    }

    pub fn pan_down(&mut self) {
        let (_, height) = self.image_size();
        self.pan.1 += (height as f32 * PAN_STEP).floor() as i32;
        self.update();
        self.draw();
    }

    pub fn pan_right(&mut self) {
        let (width, _) = self.image_size();
        self.pan.0 += (width as f32 * PAN_STEP).floor() as i32;
        self.update();
        self.draw();
    }

    pub fn pan_left(&mut self) {
        let (width, _) = self.image_size();
        self.pan.0 -= (width as f32 * PAN_STEP).floor() as i32;
        self.update();
        self.draw();
    }

    /// Get current image size.
    ///
    /// If the image is scaled, this will give the scaled size.
    fn image_size(&self) -> (u32, u32) {
        let image = self.scaled.as_ref().unwrap_or(&self.image);
        image.size()
    }

    /// Limit the pan to the view size.
    fn clamp_pan(&mut self) {
        let (im_w, im_h) = self.image_size();

        let texture = self.pixels.texture();
        let (tx_w, tx_h) = (texture.width(), texture.height());

        let x_limit = ((im_w as f32 / 2. - tx_w as f32 / 2.).floor() as i32).max(0);
        let y_limit = ((im_h as f32 / 2. - tx_h as f32 / 2.).floor() as i32).max(0);
        self.pan.0 = self.pan.0.clamp(-x_limit, x_limit);
        self.pan.1 = self.pan.1.clamp(-y_limit, y_limit);
    }

    pub fn toggle_label(&mut self) {
        self.show_label = !self.show_label;
        self.update();
        self.draw();
    }

    pub fn is_label_visible(&self) -> bool {
        self.show_label
    }

    fn draw_label(&mut self) {
        let font = FONT.get_or_init(|| {
            let font_data = include_bytes!("../font.ttf") as &[u8];
            Font::from_bytes(font_data, FontSettings::default()).expect("Failed to load font.ttf")
        });

        let texture = self.pixels.texture();
        let width = texture.width() as i32;
        let height = texture.height() as i32;
        let frame = self.pixels.frame_mut();

        let font_size = 20.0;
        let padding = 15.0;

        // Calculate the total width of the string to right-align it
        let mut total_width = 0.0;
        for c in self.label.chars() {
            total_width += font.metrics(c, font_size).advance_width;
        }

        let start_x = width as f32 - total_width - padding;
        let start_y = height as f32 - font_size - padding;

        // Draw shadow
        let passes = [
            (2.0, 2.0, 0.0),   // Shadow: X offset, Y offset, Color
            (0.0, 0.0, 255.0), // Text: X offset, Y offset, Color
        ];

        for (offset_x, offset_y, color_val) in passes {
            let mut cursor_x = start_x + offset_x;
            let cursor_y = start_y + offset_y;

            for c in self.label.chars() {
                let (metrics, bitmap) = font.rasterize(c, font_size);

                for (i, &coverage) in bitmap.iter().enumerate() {
                    let lx = (i % metrics.width) as i32;
                    let ly = (i / metrics.width) as i32;

                    // Calculate screen pixel coordinates
                    let px = cursor_x as i32 + metrics.xmin + lx;
                    // Font y-axis usually originates from the baseline
                    let py =
                        cursor_y as i32 + font_size as i32 - metrics.height as i32 - metrics.ymin
                            + ly;

                    // If pixel is within bounds and has some opacity
                    if px >= 0 && px < width && py >= 0 && py < height && coverage > 0 {
                        let idx = ((py * width + px) * 4) as usize;
                        let alpha = coverage as f32 / 255.0;

                        // Blend RGB
                        for color_channel in 0..3 {
                            let bg = frame[idx + color_channel] as f32;
                            frame[idx + color_channel] =
                                ((color_val * alpha) + (bg * (1.0 - alpha))) as u8;
                        }

                        // Blend Alpha
                        let bg_alpha = frame[idx + 3] as f32;
                        frame[idx + 3] = ((255.0 * alpha) + (bg_alpha * (1.0 - alpha))) as u8;
                    }
                }
                cursor_x += metrics.advance_width;
            }
        }
    }
}

/// Extract a window on the image that fits into the surface texture area,
/// accounting for any pan.
fn view_buffer_window(pixels: &mut Pixels, image: &mut Image, pan: (i32, i32)) {
    let texture = pixels.texture();
    let (w, h) = (texture.width(), texture.height());
    let size = image.size();
    let data = image.next_frame();
    let view = buffer_window(data, size, (w, h), pan);
    pixels.frame_mut().copy_from_slice(&view);
}

/// Extract image data to fit into a window, with offset.
fn buffer_window(
    buffer: &[u8],
    (img_width, img_height): (u32, u32),
    (win_width, win_height): (u32, u32),
    (offset_x, offset_y): (i32, i32), // Center-anchored offset
) -> Vec<u8> {
    // Assumes RGBA (i.e. 4 channels).
    const CHANNELS: usize = 4;

    // Padding for when `img` is smaller than `win`, to keep it centered.
    let padding_x = (win_width.saturating_sub(img_width) / 2) as usize;
    let padding_y = (win_height.saturating_sub(img_height) / 2) as usize;

    // Range of pixels to copy from the image, based on the window and any pan offset.
    let (start_x, end_x) = centered_span(img_width as i32, win_width as i32, offset_x);
    let (start_y, end_y) = centered_span(img_height as i32, win_height as i32, offset_y);

    // Copy the in-window image pixels.
    let end_y = end_y.min(img_height as usize);
    let slice_width = (end_x - start_x).min(img_width as usize);
    let mut result = vec![0u8; (win_width * win_height) as usize * CHANNELS];
    for (i, y) in (start_y..end_y).enumerate() {
        let a = flat_idx(start_x, y, img_width as usize) * CHANNELS;
        let b = a + slice_width * CHANNELS;
        let im_row = &buffer[a..b];

        let x = padding_x;
        let y = padding_y + i;
        let idx = flat_idx(x, y, win_width as usize) * CHANNELS;
        let end_idx = idx + slice_width * CHANNELS;
        result[idx..end_idx].copy_from_slice(im_row);
    }

    result
}

fn flat_idx(x: usize, y: usize, w: usize) -> usize {
    y * w + x
}

fn centered_span(img_dim: i32, win_dim: i32, offset: i32) -> (usize, usize) {
    let img_center = img_dim / 2 + offset;
    let win_center = win_dim / 2;
    let start = (img_center - win_center).max(0).min(img_dim) as usize;
    let end = start + win_dim as usize;
    (start, end)
}
