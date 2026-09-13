//! PNG encoder.

use crate::error::{PdfError, Result};
use flate2::Compression;
use flate2::write::ZlibEncoder;
use std::io::Write;

pub(super) fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>> {
    if rgba.len() < (width as usize) * (height as usize) * 4 {
        return Err(PdfError::InvalidInput(
            "pixel buffer too small for dimensions".into(),
        ));
    }
    let mut out = Vec::with_capacity(rgba.len() / 4 + rgba.len() / 8);
    out.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);

    // IHDR
    write_chunk(&mut out, b"IHDR", &ihdr_data(width, height));

    // IDAT — raw scanlines with filter byte 0 each.
    let mut raw = Vec::with_capacity(rgba.len() + height as usize);
    let stride = width as usize * 4;
    for y in 0..height as usize {
        raw.push(0u8); // filter: None
        raw.extend_from_slice(&rgba[y * stride..y * stride + stride]);
    }
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
    enc.write_all(&raw)?;
    let compressed = enc.finish()?;
    write_chunk(&mut out, b"IDAT", &compressed);

    // IEND
    write_chunk(&mut out, b"IEND", &[]);

    Ok(out)
}

fn ihdr_data(width: u32, height: u32) -> Vec<u8> {
    let mut v = Vec::with_capacity(13);
    v.extend_from_slice(&width.to_be_bytes());
    v.extend_from_slice(&height.to_be_bytes());
    v.push(8); // bit depth
    v.push(6); // color type: RGBA
    v.push(0); // compression
    v.push(0); // filter
    v.push(0); // interlace
    v
}

fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = Crc::new();
    crc.update(kind);
    crc.update(data);
    out.extend_from_slice(&crc.finalize().to_be_bytes());
}

/// Minimal CRC-32 (PNG polynomial).
struct Crc(u32);

const CRC_TABLE: [u32; 256] = {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xedb8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
};

impl Crc {
    fn new() -> Self {
        Crc(0xFFFF_FFFF)
    }
    fn update(&mut self, data: &[u8]) {
        for &b in data {
            let idx = ((self.0 ^ b as u32) & 0xFF) as usize;
            self.0 = CRC_TABLE[idx] ^ (self.0 >> 8);
        }
    }
    fn finalize(&self) -> u32 {
        self.0 ^ 0xFFFF_FFFF
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_signature_is_correct() {
        let pixels = vec![0u8; 4];
        let png = encode_png(1, 1, &pixels).unwrap();
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    }
}
