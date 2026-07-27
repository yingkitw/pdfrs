//! Data compression and decompression utilities
//!
//! Wraps `flate2` for zlib/Deflate compression used in PDF streams.
//! Provides [`compress_deflate`] for writing and [`decompress_deflate`] for reading.

use anyhow::Result;
use flate2::Compression;
use flate2::bufread::ZlibDecoder;
use flate2::write::ZlibEncoder;
use std::io::{Read, Write};

pub fn decompress_deflate(data: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = ZlibDecoder::new(data);
    let mut result = Vec::new();
    decoder.read_to_end(&mut result)?;
    Ok(result)
}

pub fn compress_deflate(data: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data)?;
    Ok(encoder.finish()?)
}

/// Compress data with a specific compression level (0-9)
pub fn compress_deflate_with_level(data: &[u8], level: u8) -> Result<Vec<u8>> {
    let level = level.clamp(0, 9);
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(level as u32));
    encoder.write_all(data)?;
    Ok(encoder.finish()?)
}

pub fn decode_hex_string(hex_str: &str) -> Result<Vec<u8>> {
    let hex_str = hex_str.trim();
    let mut result = Vec::new();

    for i in (0..hex_str.len()).step_by(2) {
        if i + 1 < hex_str.len() {
            let byte_str = &hex_str[i..i + 2];
            let byte = u8::from_str_radix(byte_str, 16)
                .map_err(|_| anyhow::anyhow!("Invalid hex string: {}", byte_str))?;
            result.push(byte);
        }
    }

    Ok(result)
}

pub fn encode_hex_string(data: &[u8]) -> String {
    data.iter().map(|byte| format!("{:02X}", byte)).collect()
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn roundtrip_hex(s in "([0-9a-fA-F]{2})*") {
            let bytes = decode_hex_string(&s).unwrap();
            let encoded = encode_hex_string(&bytes);
            assert_eq!(s.to_lowercase(), encoded.to_lowercase());
        }
    }

    proptest! {
        #[test]
        fn compress_decompress_roundtrip(data in prop::collection::vec(any::<u8>(), 0..10000)) {
            // Note: This test will use our stub compression which just returns the data as-is
            // In production with real compression, this would verify roundtrip
            let compressed = compress_deflate(&data).unwrap();
            let decompressed = decompress_deflate(&compressed).unwrap();
            assert_eq!(data, decompressed);
        }
    }
}
