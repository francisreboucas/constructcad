//! Raster images kept in a project (imported pictures, scanned drawings):
//! the file's own bytes, unchanged, with the format and pixel size. Project
//! files hold the bytes as base64 text.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Largest image file kept, bytes.
pub const MAX_IMAGE_BYTES: usize = 64 << 20;
/// Most pixels an image may have (width × height).
pub const MAX_IMAGE_PIXELS: u64 = 200_000_000;

/// Encoding of an image's bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RasterFormat {
    Png,
    Jpeg,
    Gif,
    Bmp,
    Webp,
}

impl RasterFormat {
    /// The format of `bytes`, from their signature.
    pub fn sniff(bytes: &[u8]) -> Option<RasterFormat> {
        match bytes {
            [0x89, b'P', b'N', b'G', ..] => Some(RasterFormat::Png),
            [0xFF, 0xD8, 0xFF, ..] => Some(RasterFormat::Jpeg),
            [b'G', b'I', b'F', b'8', ..] => Some(RasterFormat::Gif),
            [b'B', b'M', ..] => Some(RasterFormat::Bmp),
            [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some(RasterFormat::Webp),
            _ => None,
        }
    }

    /// Media type (for data URLs).
    pub fn mime(self) -> &'static str {
        match self {
            RasterFormat::Png => "image/png",
            RasterFormat::Jpeg => "image/jpeg",
            RasterFormat::Gif => "image/gif",
            RasterFormat::Bmp => "image/bmp",
            RasterFormat::Webp => "image/webp",
        }
    }
}

/// An image file's bytes with its format and size in pixels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RasterImage {
    pub format: RasterFormat,
    pub width: u32,
    pub height: u32,
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub data: Vec<u8>,
}

impl RasterImage {
    /// Whether the size is usable and the bytes start like the format says.
    pub fn is_valid(&self) -> bool {
        self.width > 0
            && self.height > 0
            && u64::from(self.width) * u64::from(self.height) <= MAX_IMAGE_PIXELS
            && !self.data.is_empty()
            && self.data.len() <= MAX_IMAGE_BYTES
            && RasterFormat::sniff(&self.data) == Some(self.format)
    }

    /// Resolution the file declares (dots per inch, x and y), if any: a PNG's
    /// `pHYs` chunk in dots per meter, or a JPEG's JFIF density.
    pub fn dpi(&self) -> Option<(f64, f64)> {
        let d = &self.data;
        let be32 = |i: usize| d.get(i..i + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
        let be16 = |i: usize| d.get(i..i + 2).map(|b| u16::from_be_bytes([b[0], b[1]]));
        let dpi = match self.format {
            RasterFormat::Png => {
                // Chunks after the 8-byte signature: length, type, data, CRC.
                let mut at = 8;
                let mut found = None;
                for _ in 0..64 {
                    let (Some(len), Some(kind)) = (be32(at), d.get(at + 4..at + 8)) else { break };
                    if kind == b"pHYs" {
                        if let (Some(x), Some(y), Some(&unit)) = (be32(at + 8), be32(at + 12), d.get(at + 16))
                            && unit == 1
                        {
                            found = Some((f64::from(x) * 0.0254, f64::from(y) * 0.0254));
                        }
                        break;
                    }
                    if kind == b"IDAT" {
                        break;
                    }
                    at = at.saturating_add(12).saturating_add(len as usize);
                }
                found
            }
            RasterFormat::Jpeg => {
                // APP0 "JFIF\0": version, units (1 dpi, 2 dots per cm), x and y density.
                match (d.get(2..4), d.get(6..11), d.get(13), be16(14), be16(16)) {
                    (Some([0xFF, 0xE0]), Some(b"JFIF\0"), Some(&unit), Some(x), Some(y)) => match unit {
                        1 => Some((f64::from(x), f64::from(y))),
                        2 => Some((f64::from(x) * 2.54, f64::from(y) * 2.54)),
                        _ => None,
                    },
                    _ => None,
                }
            }
            _ => None,
        }?;
        (dpi.0 >= 10.0 && dpi.1 >= 10.0 && dpi.0 <= 100_000.0 && dpi.1 <= 100_000.0).then_some(dpi)
    }
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding.
pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> shift) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Decode standard base64 (whitespace ignored); `None` on any other character.
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

fn to_base64<S: Serializer>(data: &[u8], s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&base64_encode(data))
}

fn from_base64<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    let text = String::deserialize(d)?;
    if text.len() / 4 * 3 > MAX_IMAGE_BYTES + 3 {
        return Err(serde::de::Error::custom("image too large"));
    }
    base64_decode(&text).ok_or_else(|| serde::de::Error::custom("invalid base64"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips() {
        for n in 0..10 {
            let bytes: Vec<u8> = (0..n).map(|i| (i * 37 + 5) as u8).collect();
            assert_eq!(base64_decode(&base64_encode(&bytes)).as_deref(), Some(&bytes[..]));
        }
        assert_eq!(base64_encode(b"Man"), "TWFu");
        assert_eq!(base64_encode(b"Ma"), "TWE=");
        assert_eq!(base64_decode("TW\nE="), Some(b"Ma".to_vec()));
        assert_eq!(base64_decode("T*=="), None);
    }

    #[test]
    fn formats_come_from_signatures() {
        assert_eq!(RasterFormat::sniff(b"\x89PNG\r\n\x1a\n"), Some(RasterFormat::Png));
        assert_eq!(RasterFormat::sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), Some(RasterFormat::Jpeg));
        assert_eq!(RasterFormat::sniff(b"RIFF\0\0\0\0WEBPVP8 "), Some(RasterFormat::Webp));
        assert_eq!(RasterFormat::sniff(b"%PDF"), None);
    }

    #[test]
    fn jfif_density_is_read() {
        let mut d = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 16];
        d.extend_from_slice(b"JFIF\0");
        d.extend_from_slice(&[1, 1, 1, 0x01, 0x2C, 0x01, 0x2C, 0, 0]);
        let img = RasterImage { format: RasterFormat::Jpeg, width: 1, height: 1, data: d };
        assert_eq!(img.dpi(), Some((300.0, 300.0)));
    }

    #[test]
    fn hostile_images_are_invalid() {
        let png = b"\x89PNG\r\n\x1a\n".to_vec();
        let ok = RasterImage { format: RasterFormat::Png, width: 2, height: 2, data: png.clone() };
        assert!(ok.is_valid());
        assert!(!RasterImage { width: 0, ..ok.clone() }.is_valid());
        assert!(!RasterImage { width: u32::MAX, height: u32::MAX, ..ok.clone() }.is_valid());
        assert!(!RasterImage { format: RasterFormat::Jpeg, ..ok.clone() }.is_valid());
        // A truncated pHYs chunk reads as no resolution.
        let mut short = png;
        short.extend_from_slice(&[0, 0, 0, 9, b'p', b'H', b'Y', b's', 0]);
        assert_eq!(RasterImage { data: short, ..ok }.dpi(), None);
    }
}
