use std::{fs::File, io::BufReader, path::Path};

use fast_image_resize::{FilterType, ResizeAlg, ResizeOptions, Resizer, images::Image as FIRImage};
use image::{
    AnimationDecoder, DynamicImage, GenericImageView, ImageBuffer, ImageDecoder, ImageResult,
    RgbaImage,
    codecs::{gif::GifDecoder, webp::WebPDecoder},
};

/// We can have either a single image
/// or a sequence of images (i.e. an animated gif).
pub enum Image {
    Single {
        data: Vec<u8>,
        size: (u32, u32),
    },
    Sequence {
        frames: Vec<Vec<u8>>,
        delays: Vec<f64>,
        index: usize,
        size: (u32, u32),
    },
}
impl Image {
    pub fn size(&self) -> (u32, u32) {
        match self {
            Self::Single { size, .. } => *size,
            Self::Sequence { size, .. } => *size,
        }
    }

    pub fn delays(&self) -> Option<&[f64]> {
        match self {
            Image::Sequence { delays, .. } => Some(delays),
            _ => None,
        }
    }

    /// Get the next frame.
    pub fn next_frame(&mut self) -> &[u8] {
        match self {
            Self::Single { data, .. } => data,
            Self::Sequence { index, frames, .. } => {
                let data = &frames[*index % frames.len()];
                *index += 1;
                data
            }
        }
    }
}

/// Read frames from an animated format.
///
/// NOTE: `webp` decoding is slow and there's
/// some compositing bug: <https://github.com/image-rs/image/issues/2320>
fn read_frames<'a, D: AnimationDecoder<'a> + ImageDecoder>(decoder: D) -> Image {
    let size = decoder.dimensions();
    let decoded = decoder
        .into_frames()
        .collect_frames()
        .expect("Failed to decode frames");
    let (frames, delays): (Vec<_>, Vec<_>) = decoded
        .into_iter()
        .map(|f| {
            let (num, den) = f.delay().numer_denom_ms();
            let delay = (num as f64 / den as f64) / 1000.;
            (f.into_buffer().into_raw(), delay)
        })
        .unzip();

    Image::Sequence {
        frames,
        delays,
        size,
        index: 0,
    }
}

fn resize(src: DynamicImage, (dst_width, dst_height): (u32, u32)) -> DynamicImage {
    let src_width = src.width();
    let src_height = src.height();
    let src_image = FIRImage::from_vec_u8(
        src_width,
        src_height,
        src.to_rgba8().into_raw(),
        fast_image_resize::PixelType::U8x4,
    )
    .unwrap();
    let mut dst_image = FIRImage::new(dst_width, dst_height, src_image.pixel_type());
    let mut resizer = Resizer::new();
    resizer
        .resize(
            &src_image,
            &mut dst_image,
            &ResizeOptions::default().resize_alg(ResizeAlg::Convolution(FilterType::Hamming)),
        )
        .unwrap();

    let image_buffer: RgbaImage =
        ImageBuffer::from_raw(dst_width, dst_height, dst_image.into_vec())
            .expect("Failed to convert resized buffer to ImageBuffer");
    DynamicImage::ImageRgba8(image_buffer)
}

fn read_single(path: &Path, (max_width, max_height): (u32, u32)) -> ImageResult<Image> {
    image::open(path).map(|mut img| {
        // Resize to fit if needed.
        let mut size = img.dimensions();
        let width_scale = max_width as f32 / size.0 as f32;
        let height_scale = max_height as f32 / size.1 as f32;
        let scale = width_scale.min(height_scale);
        if scale < 1. {
            let target_width = (scale * size.0 as f32).round() as u32;
            let target_height = (scale * size.1 as f32).round() as u32;
            img = resize(img, (target_width, target_height));
            size = img.dimensions();
        }
        let rgba = img.to_rgba8();
        let pixels: Vec<u8> = rgba.into_raw();
        Image::Single { data: pixels, size }
    })
}

pub fn read_image(path: &Path, max_size: (u32, u32)) -> ImageResult<Image> {
    let ext = path.extension().and_then(|ext| ext.to_str());
    match ext {
        Some("gif") => {
            let file = File::open(path).expect("Failed to read gif");
            let reader = BufReader::new(file);
            let decoder = GifDecoder::new(reader).expect("Failed to decode gif");
            Ok(read_frames(decoder))
        }
        Some("webp") => {
            let file = File::open(path).expect("Failed to read webp");
            let reader = BufReader::new(file);
            let decoder = WebPDecoder::new(reader).expect("Failed to decode webp");
            if decoder.has_animation() {
                Ok(read_frames(decoder))
            } else {
                read_single(path, max_size)
            }
        }
        _ => read_single(path, max_size),
    }
}
