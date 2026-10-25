//! Baseline JPEG decoding (DCTDecode).
//!
//! Supports baseline sequential DCT (SOF0) and extended sequential (SOF1)
//! Huffman-coded images with 8-bit precision: grayscale, YCbCr (3-component)
//! and CMYK/YCCK (4-component) colour spaces, with arbitrary sampling
//! factors and restart intervals. Progressive (SOF2) and arithmetic-coded
//! images are rejected, and the caller skips the image instead of failing the
//! whole page.
//!
//! The decoder is intentionally self-contained: the crate ships no image
//! codec dependency, and PDF images are frequently stored as DCTDecode
//! streams that would otherwise be invisible to the rasterizer.

/// A decoded image: 8-bit RGBA samples.
#[derive(Debug, Clone)]
pub(super) struct DecodedImage {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Row-major RGBA samples, `width * height * 4` bytes.
    pub rgba: Vec<u8>,
}

/// Decode a baseline JPEG byte stream.
pub(super) fn decode_jpeg(data: &[u8]) -> Option<DecodedImage> {
    JpegParser::new(data).run()
}

const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

/// `COS[u][x] = 0.5 * c(u) * cos((2x + 1) * u * PI / 16)`, the separable
/// 8-point inverse DCT basis.
fn cos_table() -> [[f32; 8]; 8] {
    let mut table = [[0.0f32; 8]; 8];
    for (u, row) in table.iter_mut().enumerate() {
        let cu = if u == 0 {
            (1.0f32 / 2.0) * (1.0f32 / 2f32.sqrt())
        } else {
            0.5
        };
        for (x, cell) in row.iter_mut().enumerate() {
            let angle = (2 * x + 1) as f32 * u as f32 * std::f32::consts::PI / 16.0;
            *cell = cu * angle.cos();
        }
    }
    table
}

/// A canonical Huffman table.
struct HuffTable {
    min_code: [i32; 17],
    max_code: [i32; 17],
    val_ptr: [i32; 17],
    values: Vec<u8>,
}

impl HuffTable {
    fn build(counts: &[u8; 16], values: Vec<u8>) -> Self {
        let mut min_code = [0i32; 17];
        let mut max_code = [-1i32; 17];
        let mut val_ptr = [0i32; 17];
        let mut code = 0i32;
        let mut k = 0i32;
        for length in 1..=16usize {
            val_ptr[length] = k;
            min_code[length] = code;
            let count = counts[length - 1] as i32;
            if count > 0 {
                code += count;
                k += count;
                max_code[length] = code - 1;
            }
            code <<= 1;
        }
        HuffTable {
            min_code,
            max_code,
            val_ptr,
            values,
        }
    }
}

/// One image component (a colour plane).
struct Component {
    id: u8,
    /// Horizontal sampling factor.
    h: usize,
    /// Vertical sampling factor.
    v: usize,
    /// Quantization table index.
    quant: usize,
    /// DC Huffman table index.
    dc_table: usize,
    /// AC Huffman table index.
    ac_table: usize,
    /// Decoded samples, `plane_width * plane_height` bytes.
    samples: Vec<u8>,
    plane_width: usize,
    plane_height: usize,
}

/// Bit reader over entropy-coded data, with `0xFF00` byte unstuffing.
struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bit_buffer: u32,
    bit_count: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8], pos: usize) -> Self {
        BitReader {
            data,
            pos,
            bit_buffer: 0,
            bit_count: 0,
        }
    }

    /// Read one bit; `None` at end of data.
    fn read_bit(&mut self) -> Option<u32> {
        if self.bit_count == 0 {
            let byte = *self.data.get(self.pos)?;
            self.pos += 1;
            if byte == 0xFF {
                // 0xFF00 is a stuffed literal 0xFF; 0xFFD9 ends the stream.
                match self.data.get(self.pos) {
                    Some(0x00) => self.pos += 1,
                    _ => {
                        // Marker: back up and report end of data.
                        self.pos -= 1;
                        return None;
                    }
                }
            }
            self.bit_buffer = byte as u32;
            self.bit_count = 8;
        }
        self.bit_count -= 1;
        Some((self.bit_buffer >> self.bit_count) & 1)
    }

    /// Read `n` bits.
    fn read_bits(&mut self, n: u32) -> Option<i32> {
        let mut value = 0i32;
        for _ in 0..n {
            value = (value << 1) | self.read_bit()? as i32;
        }
        Some(value)
    }

    /// Discard buffered bits and align to the next byte boundary.
    fn align(&mut self) {
        self.bit_count = 0;
    }
}

struct JpegParser<'a> {
    data: &'a [u8],
    pos: usize,
    quant_tables: [[u16; 64]; 4],
    dc_tables: Vec<Option<HuffTable>>,
    ac_tables: Vec<Option<HuffTable>>,
    width: usize,
    height: usize,
    components: Vec<Component>,
    restart_interval: usize,
    adobe_transform: Option<u8>,
}

impl<'a> JpegParser<'a> {
    fn new(data: &'a [u8]) -> Self {
        JpegParser {
            data,
            pos: 0,
            quant_tables: [[0u16; 64]; 4],
            dc_tables: (0..4).map(|_| None).collect(),
            ac_tables: (0..4).map(|_| None).collect(),
            width: 0,
            height: 0,
            components: Vec::new(),
            restart_interval: 0,
            adobe_transform: None,
        }
    }

    fn run(mut self) -> Option<DecodedImage> {
        // SOI
        if self.data.len() < 4 || self.data[0] != 0xFF || self.data[1] != 0xD8 {
            return None;
        }
        self.pos = 2;

        while self.pos < self.data.len() {
            // Find the next marker.
            while self.pos < self.data.len() && self.data[self.pos] != 0xFF {
                self.pos += 1;
            }
            while self.pos < self.data.len() && self.data[self.pos] == 0xFF {
                self.pos += 1;
            }
            if self.pos >= self.data.len() {
                break;
            }
            let marker = self.data[self.pos];
            self.pos += 1;
            match marker {
                0xD9 => break, // EOI
                0xDB => self.read_quant_tables()?,
                0xC4 => self.read_huffman_tables()?,
                0xDD => self.read_restart_interval()?,
                0xEE => self.read_adobe_app14()?,
                0xC0 | 0xC1 => self.read_sof()?,
                0xC2 => return None, // progressive is not supported
                0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => return None,
                0xDA => {
                    self.read_sos()?;
                    self.decode_scan()?;
                    break;
                }
                0xD0..=0xD7 => {}
                _ => {
                    // Any other marker with a length: skip it.
                    let len = self.read_u16()?;
                    self.pos = self.pos.checked_add(len.saturating_sub(2) as usize)?;
                }
            }
        }

        if self.width == 0 || self.height == 0 || self.components.is_empty() {
            return None;
        }
        Some(self.to_rgba())
    }

    fn read_u16(&mut self) -> Option<u16> {
        if self.pos + 1 >= self.data.len() {
            return None;
        }
        let value = u16::from_be_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        Some(value)
    }

    fn read_quant_tables(&mut self) -> Option<()> {
        let len = self.read_u16()? as usize;
        let end = self.pos + len - 2;
        while self.pos < end && self.pos < self.data.len() {
            let spec = self.data[self.pos];
            self.pos += 1;
            let precision = spec >> 4;
            let index = (spec & 0x0F) as usize;
            if index >= 4 {
                return None;
            }
            for zigzag in ZIGZAG.iter() {
                let value = if precision == 0 {
                    let v = *self.data.get(self.pos)? as u16;
                    self.pos += 1;
                    v
                } else {
                    let v = u16::from_be_bytes([
                        *self.data.get(self.pos)?,
                        *self.data.get(self.pos + 1)?,
                    ]);
                    self.pos += 2;
                    v
                };
                self.quant_tables[index][*zigzag] = value;
            }
        }
        self.pos = end;
        Some(())
    }

    fn read_huffman_tables(&mut self) -> Option<()> {
        let len = self.read_u16()? as usize;
        let end = self.pos + len - 2;
        while self.pos < end && self.pos < self.data.len() {
            let spec = self.data[self.pos];
            self.pos += 1;
            let is_ac = spec >> 4 == 1;
            let index = (spec & 0x0F) as usize;
            if index >= 4 {
                return None;
            }
            let mut counts = [0u8; 16];
            let mut total = 0usize;
            for count in counts.iter_mut() {
                *count = *self.data.get(self.pos)?;
                self.pos += 1;
                total += *count as usize;
            }
            if self.pos + total > self.data.len() {
                return None;
            }
            let values = self.data[self.pos..self.pos + total].to_vec();
            self.pos += total;
            let table = HuffTable::build(&counts, values);
            if is_ac {
                self.ac_tables[index] = Some(table);
            } else {
                self.dc_tables[index] = Some(table);
            }
        }
        self.pos = end;
        Some(())
    }

    fn read_restart_interval(&mut self) -> Option<()> {
        let len = self.read_u16()? as usize;
        if len < 2 {
            return None;
        }
        self.restart_interval = self.read_u16()? as usize;
        self.pos += len.saturating_sub(4);
        Some(())
    }

    fn read_adobe_app14(&mut self) -> Option<()> {
        let len = self.read_u16()? as usize;
        let end = self.pos + len - 2;
        if end <= self.pos {
            self.pos = end;
            return Some(());
        }
        let body = &self.data[self.pos..end.min(self.data.len())];
        if body.len() >= 12 && &body[0..5] == b"Adobe" {
            self.adobe_transform = Some(body[11]);
        }
        self.pos = end;
        Some(())
    }

    fn read_sof(&mut self) -> Option<()> {
        let len = self.read_u16()? as usize;
        let end = self.pos + len - 2;
        let precision = *self.data.get(self.pos)?;
        self.pos += 1;
        if precision != 8 {
            return None;
        }
        self.height = self.read_u16()? as usize;
        self.width = self.read_u16()? as usize;
        let count = *self.data.get(self.pos)? as usize;
        self.pos += 1;
        if count == 0 || count > 4 {
            return None;
        }
        self.components.clear();
        for _ in 0..count {
            let id = *self.data.get(self.pos)?;
            let sampling = *self.data.get(self.pos + 1)?;
            let quant = *self.data.get(self.pos + 2)? as usize;
            self.pos += 3;
            let h = ((sampling >> 4) & 0x0F).max(1) as usize;
            let v = (sampling & 0x0F).max(1) as usize;
            self.components.push(Component {
                id,
                h,
                v,
                quant: quant.min(3),
                dc_table: 0,
                ac_table: 0,
                samples: Vec::new(),
                plane_width: 0,
                plane_height: 0,
            });
        }
        self.pos = end;
        Some(())
    }

    fn read_sos(&mut self) -> Option<()> {
        let len = self.read_u16()? as usize;
        let end = self.pos + len - 2;
        let count = *self.data.get(self.pos)? as usize;
        self.pos += 1;
        if count == 0 || count > self.components.len() {
            return None;
        }
        for _ in 0..count {
            let id = *self.data.get(self.pos)?;
            let tables = *self.data.get(self.pos + 1)?;
            self.pos += 2;
            if let Some(component) = self.components.iter_mut().find(|c| c.id == id) {
                component.dc_table = ((tables >> 4) & 0x0F).min(3) as usize;
                component.ac_table = (tables & 0x0F).min(3) as usize;
            }
        }
        self.pos = end;
        Some(())
    }

    /// Decode the entropy-coded scan into per-component sample planes.
    fn decode_scan(&mut self) -> Option<()> {
        let max_h = self.components.iter().map(|c| c.h).max().unwrap_or(1);
        let max_v = self.components.iter().map(|c| c.v).max().unwrap_or(1);
        let mcu_width = max_h * 8;
        let mcu_height = max_v * 8;
        let mcus_x = self.width.div_ceil(mcu_width);
        let mcus_y = self.height.div_ceil(mcu_height);

        // Move the components out of `self` so their sample planes can be
        // written while the shared tables stay borrowed.
        let mut components = std::mem::take(&mut self.components);
        for component in &mut components {
            component.plane_width = mcus_x * component.h * 8;
            component.plane_height = mcus_y * component.v * 8;
            component.samples = vec![0u8; component.plane_width * component.plane_height];
        }

        let mut reader = BitReader::new(self.data, self.pos);
        let mut predictors = vec![0i32; components.len()];
        let cos = cos_table();
        let mut pixels = [0f32; 64];

        let mut mcu_index = 0usize;
        for mcu_y in 0..mcus_y {
            for mcu_x in 0..mcus_x {
                if self.restart_interval > 0
                    && mcu_index > 0
                    && mcu_index.is_multiple_of(self.restart_interval)
                {
                    reader.align();
                    self.skip_restart_marker(&mut reader);
                    predictors.iter_mut().for_each(|p| *p = 0);
                }
                mcu_index += 1;

                for (ci, component) in components.iter_mut().enumerate() {
                    for by in 0..component.v {
                        for bx in 0..component.h {
                            let mut block = [0i32; 64];
                            let dc_table = self.dc_tables[component.dc_table].as_ref()?;
                            let ac_table = self.ac_tables[component.ac_table].as_ref()?;
                            if !decode_block(
                                &mut reader,
                                dc_table,
                                ac_table,
                                &mut predictors[ci],
                                &mut block,
                            ) {
                                self.components = components;
                                return None;
                            }
                            let quant = &self.quant_tables[component.quant];
                            for i in 0..64 {
                                block[i] *= quant[i] as i32;
                            }
                            idct(&block, &mut pixels, &cos);

                            let origin_x = (mcu_x * component.h + bx) * 8;
                            let origin_y = (mcu_y * component.v + by) * 8;
                            for y in 0..8 {
                                for x in 0..8 {
                                    let px = origin_x + x;
                                    let py = origin_y + y;
                                    if px >= component.plane_width || py >= component.plane_height {
                                        continue;
                                    }
                                    let value = (pixels[y * 8 + x] + 128.0).round();
                                    component.samples[py * component.plane_width + px] =
                                        value.clamp(0.0, 255.0) as u8;
                                }
                            }
                        }
                    }
                }
            }
        }
        self.pos = reader.pos;
        self.components = components;
        Some(())
    }

    /// Skip an `RSTn` marker, aligning to the next marker.
    fn skip_restart_marker(&mut self, reader: &mut BitReader<'_>) {
        let mut pos = reader.pos;
        while pos + 1 < self.data.len() {
            if self.data[pos] == 0xFF && (0xD0..=0xD7).contains(&self.data[pos + 1]) {
                reader.pos = pos + 2;
                return;
            }
            pos += 1;
        }
        reader.pos = pos;
    }

    /// Convert the decoded component planes into RGBA samples.
    fn to_rgba(&self) -> DecodedImage {
        let width = self.width;
        let height = self.height;
        let mut rgba = vec![255u8; width * height * 4];

        let max_h = self.components.iter().map(|c| c.h).max().unwrap_or(1);
        let max_v = self.components.iter().map(|c| c.v).max().unwrap_or(1);

        let sample = |component: &Component, x: usize, y: usize| -> u8 {
            let sx = (x * component.h / max_h).min(component.plane_width.saturating_sub(1));
            let sy = (y * component.v / max_v).min(component.plane_height.saturating_sub(1));
            component
                .samples
                .get(sy * component.plane_width + sx)
                .copied()
                .unwrap_or(0)
        };

        for y in 0..height {
            for x in 0..width {
                let i = (y * width + x) * 4;
                match self.components.len() {
                    1 => {
                        let g = sample(&self.components[0], x, y);
                        rgba[i] = g;
                        rgba[i + 1] = g;
                        rgba[i + 2] = g;
                    }
                    3 => {
                        let c = &self.components;
                        let yv = sample(&c[0], x, y) as f32;
                        let cb = sample(&c[1], x, y) as f32 - 128.0;
                        let cr = sample(&c[2], x, y) as f32 - 128.0;
                        let (r, g, b) = if self.adobe_transform == Some(0) {
                            (yv, cb + 128.0, cr + 128.0)
                        } else {
                            (
                                yv + 1.402 * cr,
                                yv - 0.344136 * cb - 0.714136 * cr,
                                yv + 1.772 * cb,
                            )
                        };
                        rgba[i] = r.clamp(0.0, 255.0) as u8;
                        rgba[i + 1] = g.clamp(0.0, 255.0) as u8;
                        rgba[i + 2] = b.clamp(0.0, 255.0) as u8;
                    }
                    4 => {
                        let c = &self.components;
                        let mut cmyk = [
                            sample(&c[0], x, y) as f32,
                            sample(&c[1], x, y) as f32,
                            sample(&c[2], x, y) as f32,
                            sample(&c[3], x, y) as f32,
                        ];
                        if self.adobe_transform != Some(0) {
                            let (y0, cb, cr, k) =
                                (cmyk[0], cmyk[1] - 128.0, cmyk[2] - 128.0, cmyk[3]);
                            cmyk = [
                                y0 + 1.402 * cr,
                                y0 - 0.344136 * cb - 0.714136 * cr,
                                y0 + 1.772 * cb,
                                k,
                            ];
                        }
                        let (c0, m0, y0, k0) = (
                            cmyk[0] / 255.0,
                            cmyk[1] / 255.0,
                            cmyk[2] / 255.0,
                            cmyk[3] / 255.0,
                        );
                        rgba[i] = (255.0 * (1.0 - c0) * (1.0 - k0)).clamp(0.0, 255.0) as u8;
                        rgba[i + 1] = (255.0 * (1.0 - m0) * (1.0 - k0)).clamp(0.0, 255.0) as u8;
                        rgba[i + 2] = (255.0 * (1.0 - y0) * (1.0 - k0)).clamp(0.0, 255.0) as u8;
                    }
                    _ => {}
                }
            }
        }

        DecodedImage {
            width: width as u32,
            height: height as u32,
            rgba,
        }
    }
}

/// Decode one 8×8 block: DC difference plus AC coefficients.
fn decode_block(
    reader: &mut BitReader<'_>,
    dc_table: &HuffTable,
    ac_table: &HuffTable,
    predictor: &mut i32,
    out: &mut [i32; 64],
) -> bool {
    let t = match decode_huffman(reader, dc_table) {
        Some(v) => v,
        None => return false,
    };
    let diff = if t == 0 {
        0
    } else {
        match reader.read_bits(t as u32) {
            Some(v) => extend(v, t),
            None => return false,
        }
    };
    *predictor += diff;
    out[0] = *predictor;

    let mut k = 1usize;
    while k < 64 {
        let rs = match decode_huffman(reader, ac_table) {
            Some(v) => v,
            None => return false,
        };
        let s = rs & 0x0F;
        let r = rs >> 4;
        if s == 0 {
            if r == 15 {
                k += 16;
                continue;
            }
            break; // end of block
        }
        k += r as usize;
        if k > 63 {
            break;
        }
        let value = match reader.read_bits(s as u32) {
            Some(v) => extend(v, s),
            None => return false,
        };
        out[ZIGZAG[k]] = value;
        k += 1;
    }
    true
}

/// Sign-extend a `t`-bit JPEG coefficient.
fn extend(value: i32, t: u8) -> i32 {
    if t == 0 {
        return 0;
    }
    if value < (1 << (t - 1)) {
        value - (1 << t) + 1
    } else {
        value
    }
}

/// Decode one Huffman symbol.
fn decode_huffman(reader: &mut BitReader<'_>, table: &HuffTable) -> Option<u8> {
    let mut code = 0i32;
    for length in 1..=16usize {
        let bit = reader.read_bit()?;
        code = (code << 1) | bit as i32;
        if table.max_code[length] >= 0 && code <= table.max_code[length] {
            let index = table.val_ptr[length] + (code - table.min_code[length]);
            return table.values.get(index as usize).copied();
        }
    }
    None
}

/// Separable inverse DCT of one 8×8 block into `out` (level-shifted by 128).
fn idct(input: &[i32; 64], out: &mut [f32; 64], cos: &[[f32; 8]; 8]) {
    let mut tmp = [0f32; 64];
    // Rows: tmp[y][x] = sum_u COS[u][x] * F[y][u]
    for y in 0..8 {
        for x in 0..8 {
            let mut sum = 0f32;
            for u in 0..8 {
                sum += cos[u][x] * input[y * 8 + u] as f32;
            }
            tmp[y * 8 + x] = sum;
        }
    }
    // Columns: out[y][x] = sum_v COS[v][y] * tmp[v][x]
    for y in 0..8 {
        for x in 0..8 {
            let mut sum = 0f32;
            for v in 0..8 {
                sum += cos[v][y] * tmp[v * 8 + x];
            }
            out[y * 8 + x] = sum;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 16x16 baseline JPEG: red in the top-left, blue in the top-right and
    /// green in the bottom half. Encoded with a standard encoder at quality
    /// 95, so it exercises the real Huffman and quantisation tables.
    const SAMPLE_JPEG_HEX: &str = concat!(
        "ffd8ffe000104a46494600010100000100010000ffdb0043000201010101010201010102020202020403020202020504",
        "040304060506060605060606070908060709070606080b08090a0a0a0a0a06080b0c0b0a0c090a0a0affdb0043010202",
        "02020202050303050a0706070a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a",
        "0a0a0a0a0a0a0a0a0a0a0a0a0a0affc00011080010001003012200021101031101ffc4001f0000010501010101010100",
        "000000000000000102030405060708090a0bffc400b5100002010303020403050504040000017d010203000411051221",
        "31410613516107227114328191a1082342b1c11552d1f02433627282090a161718191a25262728292a3435363738393a",
        "434445464748494a535455565758595a636465666768696a737475767778797a838485868788898a9293949596979899",
        "9aa2a3a4a5a6a7a8a9aab2b3b4b5b6b7b8b9bac2c3c4c5c6c7c8c9cad2d3d4d5d6d7d8d9dae1e2e3e4e5e6e7e8e9eaf1",
        "f2f3f4f5f6f7f8f9faffc4001f0100030101010101010101010000000000000102030405060708090a0bffc400b51100",
        "020102040403040705040400010277000102031104052131061241510761711322328108144291a1b1c109233352f015",
        "6272d10a162434e125f11718191a262728292a35363738393a434445464748494a535455565758595a63646566676869",
        "6a737475767778797a82838485868788898a92939495969798999aa2a3a4a5a6a7a8a9aab2b3b4b5b6b7b8b9bac2c3c4",
        "c5c6c7c8c9cad2d3d4d5d6d7d8d9dae2e3e4e5e6e7e8e9eaf2f3f4f5f6f7f8f9faffda000c03010002110311003f00f8",
        "bebe43afde4a2bf2bfa32fd393fe25cffb5bfe31ff00af7d7bd87fcc5fb0e4f61edbfea1ab73737b6feef2f2f5be9f89",
        "fd21bf6917fc47afeccff8c5bea7f53f6dff0031dedb9fdb7b2ffa83a5cbcbecbfbd7e6e96d7ffd9",
    );

    fn sample_jpeg() -> Vec<u8> {
        let bytes = SAMPLE_JPEG_HEX.as_bytes();
        bytes
            .chunks(2)
            .map(|pair| {
                let hex = std::str::from_utf8(pair).expect("hex pair");
                u8::from_str_radix(hex, 16).expect("hex digit")
            })
            .collect()
    }

    /// The RGB triple at a pixel of the decoded image.
    fn pixel(image: &DecodedImage, x: u32, y: u32) -> (u8, u8, u8) {
        let index = ((y * image.width + x) * 4) as usize;
        (
            image.rgba[index],
            image.rgba[index + 1],
            image.rgba[index + 2],
        )
    }

    #[test]
    fn rejects_non_jpeg_data() {
        assert!(decode_jpeg(b"not a jpeg").is_none());
        assert!(decode_jpeg(&[]).is_none());
    }

    #[test]
    fn decodes_baseline_jpeg_with_correct_dimensions() {
        let decoded = decode_jpeg(&sample_jpeg()).expect("should decode");
        assert_eq!(decoded.width, 16);
        assert_eq!(decoded.height, 16);
        assert_eq!(decoded.rgba.len(), 16 * 16 * 4);
    }

    #[test]
    fn decodes_baseline_jpeg_with_correct_colours() {
        let decoded = decode_jpeg(&sample_jpeg()).expect("should decode");
        // Top-left quadrant is red, top-right is blue, bottom half is green.
        let (r, g, b) = pixel(&decoded, 3, 3);
        assert!(
            r > 200 && g < 60 && b < 60,
            "expected red, got ({r}, {g}, {b})"
        );

        let (r, g, b) = pixel(&decoded, 12, 3);
        assert!(
            b > 200 && r < 60 && g < 60,
            "expected blue, got ({r}, {g}, {b})"
        );

        let (r, g, b) = pixel(&decoded, 8, 12);
        assert!(
            g > 200 && r < 60 && b < 60,
            "expected green, got ({r}, {g}, {b})"
        );
    }

    #[test]
    fn rejects_progressive_jpeg() {
        let mut jpeg = sample_jpeg();
        // Replace the SOF0 marker with SOF2 (progressive).
        let pos = jpeg
            .windows(2)
            .position(|w| w == [0xFF, 0xC0])
            .expect("sof0 marker");
        jpeg[pos + 1] = 0xC2;
        assert!(decode_jpeg(&jpeg).is_none());
    }
}
