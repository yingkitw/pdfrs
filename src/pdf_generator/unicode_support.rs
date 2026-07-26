use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub(crate) struct UnicodeFontEncoder {
    font_bytes: Vec<u8>,
    fallback_gid: u16,
    glyph_cache: RefCell<HashMap<char, u16>>,
    original_font_bytes: Option<Vec<u8>>,
    remapper: Option<subsetter::GlyphRemapper>,
}

impl UnicodeFontEncoder {
    fn from_font_bytes(font_bytes: Vec<u8>) -> Option<Self> {
        let face = ttf_parser::Face::parse(&font_bytes, 0).ok()?;
        let fallback_gid = face.glyph_index('?').map(|g| g.0).unwrap_or(0);
        Some(Self {
            font_bytes,
            fallback_gid,
            glyph_cache: RefCell::new(HashMap::new()),
            original_font_bytes: None,
            remapper: None,
        })
    }

    fn from_subset_font(
        subset_font_bytes: Vec<u8>,
        original_font_bytes: Vec<u8>,
        remapper: subsetter::GlyphRemapper,
    ) -> Option<Self> {
        let face = ttf_parser::Face::parse(&subset_font_bytes, 0).ok()?;
        let fallback_gid = face.glyph_index('?').map(|g| g.0).unwrap_or(0);
        Some(Self {
            font_bytes: subset_font_bytes,
            fallback_gid,
            glyph_cache: RefCell::new(HashMap::new()),
            original_font_bytes: Some(original_font_bytes),
            remapper: Some(remapper),
        })
    }

    fn glyph_id_for_char(&self, ch: char) -> u16 {
        if let Some(gid) = self.glyph_cache.borrow().get(&ch).copied() {
            return gid;
        }

        let gid = if let Some(ref original) = self.original_font_bytes {
            let original_gid = ttf_parser::Face::parse(original, 0)
                .ok()
                .and_then(|face| face.glyph_index(ch).map(|g| g.0))
                .unwrap_or(self.fallback_gid);
            self.remapper
                .as_ref()
                .and_then(|r| r.get(original_gid))
                .unwrap_or(original_gid)
        } else {
            ttf_parser::Face::parse(&self.font_bytes, 0)
                .ok()
                .and_then(|face| face.glyph_index(ch).map(|g| g.0))
                .unwrap_or(self.fallback_gid)
        };

        self.glyph_cache.borrow_mut().insert(ch, gid);
        gid
    }

    /// True when the underlying (pre-subset) font has a glyph for `ch`.
    fn has_glyph(&self, ch: char) -> bool {
        let bytes = self
            .original_font_bytes
            .as_ref()
            .unwrap_or(&self.font_bytes);
        ttf_parser::Face::parse(bytes, 0)
            .ok()
            .and_then(|face| face.glyph_index(ch))
            .is_some()
    }

    /// Map unsupported characters to `'?'` so missing glyphs don't hijack ToUnicode.
    fn display_char(&self, ch: char) -> char {
        if self.has_glyph(ch) {
            ch
        } else {
            '?'
        }
    }

    /// Horizontal advance in PDF font units (1/1000 em).
    fn advance_for_gid(&self, gid: u16) -> u16 {
        let Ok(face) = ttf_parser::Face::parse(&self.font_bytes, 0) else {
            return 500;
        };
        let upem = face.units_per_em().max(1) as u32;
        let adv = face
            .glyph_hor_advance(ttf_parser::GlyphId(gid))
            .unwrap_or((upem / 2) as u16) as u32;
        ((adv * 1000 + upem / 2) / upem) as u16
    }

    pub(super) fn encode_text_as_glyph_ids(&self, text: &str) -> String {
        let mut bytes = Vec::with_capacity(text.chars().count() * 2);
        for ch in text.chars() {
            let gid = self.glyph_id_for_char(self.display_char(ch));
            bytes.push((gid >> 8) as u8);
            bytes.push((gid & 0xFF) as u8);
        }

        let hex: String = bytes.iter().map(|b| format!("{:02X}", b)).collect();
        format!("<{}>", hex)
    }

    /// Estimate rendered width using real glyph advances (matches `/W` in the CIDFont).
    pub(super) fn estimate_width(&self, text: &str, font_size: f32) -> f32 {
        let mut units: u32 = 0;
        for ch in text.chars() {
            let gid = self.glyph_id_for_char(self.display_char(ch));
            units += self.advance_for_gid(gid) as u32;
        }
        units as f32 * font_size / 1000.0
    }

    /// Build a `/W` array covering every glyph used by `chars` (plus ASCII fallbacks).
    pub(super) fn build_cid_widths_array(&self, chars: &BTreeSet<char>) -> String {
        let mut widths: BTreeMap<u16, u16> = BTreeMap::new();
        for ch in chars {
            let gid = self.glyph_id_for_char(self.display_char(*ch));
            widths.insert(gid, self.advance_for_gid(gid));
        }
        // Ensure common page chrome / punctuation is covered even if not in the set.
        for ch in " Page0123456789.-_/?".chars() {
            let gid = self.glyph_id_for_char(ch);
            widths.entry(gid).or_insert_with(|| self.advance_for_gid(gid));
        }

        let mut parts = Vec::new();
        let mut run_start: Option<u16> = None;
        let mut run_widths: Vec<u16> = Vec::new();

        let flush = |parts: &mut Vec<String>, start: &mut Option<u16>, run: &mut Vec<u16>| {
            if let Some(s) = start.take() {
                let list = run
                    .iter()
                    .map(|w| w.to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                parts.push(format!("{} [{}]", s, list));
                run.clear();
            }
        };

        let mut prev: Option<u16> = None;
        for (gid, width) in widths {
            match (run_start, prev) {
                (Some(_), Some(p)) if gid == p.saturating_add(1) => {
                    run_widths.push(width);
                    prev = Some(gid);
                }
                _ => {
                    flush(&mut parts, &mut run_start, &mut run_widths);
                    run_start = Some(gid);
                    run_widths.push(width);
                    prev = Some(gid);
                }
            }
        }
        flush(&mut parts, &mut run_start, &mut run_widths);

        format!("[{}]", parts.join(" "))
    }

    /// Build a ToUnicode CMap so extractors can map glyph IDs back to Unicode.
    pub(super) fn build_tounicode_cmap(&self, chars: &BTreeSet<char>) -> Vec<u8> {
        let mut pairs: BTreeMap<u16, char> = BTreeMap::new();
        for ch in chars {
            let display = self.display_char(*ch);
            let gid = self.glyph_id_for_char(display);
            // Only record the display character so missing glyphs don't steal '?'.
            pairs.entry(gid).or_insert(display);
        }
        for ch in " Page0123456789.-_/?".chars() {
            let gid = self.glyph_id_for_char(ch);
            pairs.entry(gid).or_insert(ch);
        }

        let mut out = String::new();
        out.push_str("/CIDInit /ProcSet findresource begin\n");
        out.push_str("12 dict begin\nbegincmap\n");
        out.push_str("/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n");
        out.push_str("/CMapName /Adobe-Identity-UCS def\n");
        out.push_str("/CMapType 2 def\n");
        out.push_str("1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n");

        let entries: Vec<(u16, char)> = pairs.into_iter().collect();
        for chunk in entries.chunks(100) {
            out.push_str(&format!("{} beginbfchar\n", chunk.len()));
            for (gid, ch) in chunk {
                let mut buf = [0u16; 2];
                let encoded = (*ch).encode_utf16(&mut buf);
                let uni: String = encoded.iter().map(|u| format!("{:04X}", u)).collect();
                out.push_str(&format!("<{:04X}> <{}>\n", gid, uni));
            }
            out.push_str("endbfchar\n");
        }

        out.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
        out.into_bytes()
    }
}

fn resolve_unicode_ttf_path() -> Option<String> {
    if let Ok(path) = std::env::var("PDFRS_UNICODE_FONT_PATH")
        && !path.trim().is_empty() && Path::new(&path).exists()
    {
        return Some(path);
    }

    // macOS-first defaults with broad CJK coverage (TTF preferred over TTC).
    let candidates = [
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/Library/Fonts/Arial Unicode.ttf",
        "/System/Library/Fonts/Supplemental/Songti.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/AppleSDGothicNeo.ttc",
    ];

    candidates
        .iter()
        .find(|p| Path::new(p).exists())
        .map(|p| (*p).to_string())
}

fn load_unicode_font_bytes() -> Option<Vec<u8>> {
    let mut paths = Vec::new();
    if let Some(p) = resolve_unicode_ttf_path() {
        paths.push(p);
    }
    // Always try the known TTF first even if resolve picked a TTC.
    paths.insert(
        0,
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf".to_string(),
    );
    paths.push("/Library/Fonts/Arial Unicode.ttf".to_string());

    let mut seen = HashSet::new();
    for path in paths {
        if !seen.insert(path.clone()) {
            continue;
        }
        if !Path::new(&path).exists() {
            continue;
        }
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        if ttf_parser::Face::parse(&bytes, 0).is_ok() {
            return Some(bytes);
        }
    }
    None
}

#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn prepare_unicode_font_support() -> Option<(Vec<u8>, UnicodeFontEncoder)> {
    let bytes = load_unicode_font_bytes()?;
    let encoder = UnicodeFontEncoder::from_font_bytes(bytes.clone())?;
    Some((bytes, encoder))
}

pub(super) fn prepare_unicode_font_support_with_subsetting(
    chars: Option<&BTreeSet<char>>,
) -> Option<(Vec<u8>, UnicodeFontEncoder)> {
    let original_bytes = load_unicode_font_bytes()?;

    let (subset_bytes, remapper) = if let Some(used_chars) = chars {
        let face = ttf_parser::Face::parse(&original_bytes, 0).ok()?;
        let mut glyph_ids = HashSet::new();
        for ch in used_chars {
            if let Some(gid) = face.glyph_index(*ch) {
                glyph_ids.insert(gid.0);
            }
        }
        // Always include page chrome / punctuation so footers and wrapping stay valid.
        for ch in " Page0123456789.-_/,:;!?()[]{}\"'`+*=<>@#$%&|\\".chars() {
            if let Some(gid) = face.glyph_index(ch) {
                glyph_ids.insert(gid.0);
            }
        }
        // Always include .notdef (GID 0)
        glyph_ids.insert(0);

        let mut remapper = subsetter::GlyphRemapper::new();
        for gid in &glyph_ids {
            remapper.remap(*gid);
        }

        let subset_bytes = subsetter::subset(&original_bytes, 0, &remapper).ok()?;
        (subset_bytes, Some(remapper))
    } else {
        (original_bytes.clone(), None)
    };

    let encoder = if let Some(remapper) = remapper {
        UnicodeFontEncoder::from_subset_font(subset_bytes.clone(), original_bytes, remapper)?
    } else {
        UnicodeFontEncoder::from_font_bytes(subset_bytes.clone())?
    };

    Some((subset_bytes, encoder))
}
