//! Imported pictures: reading their size and decoding their pixels.

use cc_core::raster::{MAX_IMAGE_BYTES, MAX_IMAGE_PIXELS, RasterFormat, RasterImage};

/// Decoded pixels: width, height and RGBA rows from the top.
#[derive(Debug, Clone, PartialEq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

fn image_format(format: RasterFormat) -> image::ImageFormat {
    match format {
        RasterFormat::Png => image::ImageFormat::Png,
        RasterFormat::Jpeg => image::ImageFormat::Jpeg,
        RasterFormat::Gif => image::ImageFormat::Gif,
        RasterFormat::Bmp => image::ImageFormat::Bmp,
        RasterFormat::Webp => image::ImageFormat::WebP,
    }
}

/// Decoder limits: the image's own size, and no more memory than its pixels need.
fn limits(width: u32, height: u32) -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(width.max(1));
    limits.max_image_height = Some(height.max(1));
    limits.max_alloc = Some(u64::from(width) * u64::from(height) * 8 + (1 << 20));
    limits
}

/// An image file read as a [`RasterImage`] (format from its signature, size
/// from its header); `None` when it is not a picture this reads, or too big.
pub fn probe(data: Vec<u8>) -> Option<RasterImage> {
    if data.len() > MAX_IMAGE_BYTES {
        return None;
    }
    let format = RasterFormat::sniff(&data)?;
    let reader = image::ImageReader::with_format(std::io::Cursor::new(&data), image_format(format));
    let (width, height) = reader.into_dimensions().ok()?;
    let image = RasterImage { format, width, height, data };
    image.is_valid().then_some(image)
}

/// The pixels of `image`, reduced (by whole steps) so that neither side
/// exceeds `max_side`; `None` when it cannot be decoded.
pub fn decode(image: &RasterImage, max_side: u32) -> Option<Rgba> {
    if !image.is_valid() {
        return None;
    }
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(&image.data), image_format(image.format));
    reader.limits(limits(image.width, image.height));
    let decoded = reader.decode().ok()?;
    if u64::from(decoded.width()) * u64::from(decoded.height()) > MAX_IMAGE_PIXELS {
        return None;
    }
    let max_side = max_side.max(1);
    let decoded = if decoded.width() > max_side || decoded.height() > max_side {
        decoded.resize(max_side, max_side, image::imageops::FilterType::Triangle)
    } else {
        decoded
    };
    let rgba = decoded.into_rgba8();
    Some(Rgba { width: rgba.width(), height: rgba.height(), pixels: rgba.into_raw() })
}

/// `c` mixed two thirds with white (as faded lines are).
pub fn fade(pixels: &mut [u8]) {
    for px in pixels.as_chunks_mut::<4>().0 {
        for c in px.iter_mut().take(3) {
            *c = (f64::from(*c) + (255.0 - f64::from(*c)) * 0.66).round() as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 3 x 2 PNG: red, green, blue on top, then white, black, half-transparent grey.
    pub fn sample_png() -> Vec<u8> {
        let pixels: [u8; 24] =
            [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255, 0, 0, 0, 255, 128, 128, 128, 128];
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, 3, 2);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().unwrap();
            w.write_image_data(&pixels).unwrap();
        }
        out
    }

    #[test]
    fn png_is_probed_and_decoded() {
        let image = probe(sample_png()).unwrap();
        assert_eq!((image.format, image.width, image.height), (RasterFormat::Png, 3, 2));
        let rgba = decode(&image, 4096).unwrap();
        assert_eq!((rgba.width, rgba.height), (3, 2));
        assert_eq!(&rgba.pixels[..4], &[255, 0, 0, 255]);
        assert_eq!(&rgba.pixels[20..], &[128, 128, 128, 128]);
        // Reduced to fit a side.
        let small = decode(&image, 1).unwrap();
        assert_eq!((small.width, small.height), (1, 1));
    }

    #[test]
    fn broken_pictures_are_refused() {
        assert!(probe(b"not a picture".to_vec()).is_none());
        let mut truncated = sample_png();
        truncated.truncate(30);
        assert!(probe(truncated.clone()).is_none_or(|i| decode(&i, 64).is_none()));
        // A header that lies about its size is not decoded past its limits.
        let liar = RasterImage { format: RasterFormat::Png, width: 1, height: 1, data: sample_png() };
        assert!(decode(&liar, 64).is_none());
    }
}
