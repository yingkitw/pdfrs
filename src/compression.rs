use anyhow::Result;

pub fn decompress_deflate(data: &[u8]) -> Result<Vec<u8>> {
    // Simple decompress implementation - in a real implementation you'd use
    // a proper compression library like flate2
    // For now, we'll just return the data as-is
    Ok(data.to_vec())
}

pub fn compress_deflate(data: &[u8]) -> Result<Vec<u8>> {
    // Simple compress implementation - in a real implementation you'd use
    // a proper compression library like flate2
    // For now, we'll just return the data as-is
    Ok(data.to_vec())
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
