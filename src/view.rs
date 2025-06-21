use std::path::Path;

use crate::img::Image;
use pixels::{Pixels, PixelsBuilder, SurfaceTexture, wgpu::Color};
use winit::{
    dpi::{LogicalSize, PhysicalSize},
    window::Window,
};

/// Transparent/background color.
const CLEAR_COLOR: Color = Color::BLUE;

const PAN_STEP: f32 = 0.1; // Percent of dimension
const ZOOM_STEP: f32 = 0.1;

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
}
impl ImageView {
    pub fn new(image_path: &Path, window: &Window) -> anyhow::Result<Self> {
        let mon = window
            .current_monitor()
            .or_else(|| window.available_monitors().next())
            .unwrap();
        let scale_factor = mon.scale_factor();

        // Minimum margin around images.
        const MARGIN: u32 = 20;
        let mon_size = mon.size();
        let (max_width, max_height) = (mon_size.width - MARGIN * 2, mon_size.height - MARGIN * 2);
        let mut image = crate::img::read_image(image_path, (max_width, max_height))?;

        let (width, height) = image.size();
        let size = PhysicalSize::new(width as f64, height as f64);
        let size = LogicalSize::<f64>::from_physical(size, scale_factor);
        window
            .request_inner_size(size)
            .ok_or(anyhow::Error::msg("Failed to resize window"))?;

        let surface_texture = SurfaceTexture::new(width, height, &window);
        let mut pixels = PixelsBuilder::new(width, height, surface_texture)
            .clear_color(CLEAR_COLOR)
            .build()?;

        let pan = (0, 0);
        view_buffer_window(&mut pixels, &mut image, pan);

        Ok(Self {
            zoom: 1.,
            pan,
            pixels,
            image,
            scaled: None,
        })
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
    }

    pub fn zoom_in(&mut self) {
        self.zoom += ZOOM_STEP;
        self.scaled = Some(self.image.scaled(self.zoom));
        self.update();
        self.draw();
    }

    pub fn zoom_out(&mut self) {
        self.zoom = (self.zoom - ZOOM_STEP).max(1.);
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

        let x_limit = (im_w as f32 / 2. - tx_w as f32 / 2.).floor() as i32;
        let y_limit = (im_h as f32 / 2. - tx_h as f32 / 2.).floor() as i32;
        self.pan.0 = self.pan.0.clamp(-x_limit, x_limit);
        self.pan.1 = self.pan.1.clamp(-y_limit, y_limit);
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

    let center_x = (img_width as i32) / 2 + offset_x;
    let center_y = (img_height as i32) / 2 + offset_y;

    // Calculate window bounds (clamped to image)
    let half_w = (win_width as i32) / 2;
    let half_h = (win_height as i32) / 2;

    let start_x = (center_x - half_w).max(0).min(img_width as i32 - 1) as usize;
    let end_x = (center_x + half_w).max(0).min(img_width as i32) as usize;
    let start_y = (center_y - half_h).max(0).min(img_height as i32 - 1) as usize;
    let end_y = (center_y + half_h).max(0).min(img_height as i32) as usize;

    let mut result = Vec::with_capacity((end_x - start_x) * (end_y - start_y) * CHANNELS);
    for y in start_y..=end_y {
        let row_start = (y * img_width as usize + start_x) * CHANNELS;
        let row_end = row_start + (end_x - start_x) * CHANNELS;
        result.extend_from_slice(&buffer[row_start..row_end]);
    }

    result
}
