use std::path::Path;

use crate::img::Image;
use pixels::{Pixels, PixelsBuilder, SurfaceTexture, wgpu::Color};
use winit::{
    dpi::{LogicalSize, PhysicalSize},
    window::Window,
};

/// Transparent/background color.
const CLEAR_COLOR: Color = Color::BLUE;

pub struct ImageView {
    zoom: f32,
    pan: (i32, i32),
    pixels: Pixels,
    pub image: Image,
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
        pixels.frame_mut().copy_from_slice(image.next_frame());

        Ok(Self {
            zoom: 1.,
            pan: (0, 0),
            pixels,
            image,
        })
    }

    pub fn draw(&self) -> bool {
        self.pixels.render().is_ok()
    }

    pub fn advance(&mut self) -> bool {
        let data = self.image.next_frame();
        self.pixels.frame_mut().copy_from_slice(data);
        self.draw()
    }
}
