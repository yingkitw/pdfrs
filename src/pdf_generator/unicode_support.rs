use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub(super) struct UnicodeFontEncoder {
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

    pub(super) fn encode_text_as_glyph_ids(&self, text: &str) -> String {
        let mut bytes = Vec::with_capacity(text.chars().count() * 2);
        for ch in text.chars() {
            let gid = self.glyph_id_for_char(ch);
            bytes.push((gid >> 8) as u8);
            bytes.push((gid & 0xFF) as u8);
        }

        let hex: String = bytes.iter().map(|b| format!("{:02X}", b)).collect();
        format!("<{}>", hex)
    }
}

fn resolve_unicode_ttf_path() -> Option<String> {
    if let Ok(path) = std::env::var("PDFRS_UNICODE_FONT_PATH")
        && !path.trim().is_empty() && Path::new(&path).exists() {
            return Some(path);
        }

    // macOS-first defaults (current project target environment).
    let candidates = [
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/Library/Fonts/Arial Unicode.ttf",
    ];

    candidates
        .iter()
        .find(|p| Path::new(p).exists())
        .map(|p| (*p).to_string())
}

fn load_unicode_font_bytes() -> Option<Vec<u8>> {
    let path = resolve_unicode_ttf_path()?;
    fs::read(path).ok()
}

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
        UnicodeFontEncoder::from_subset_font(
            subset_bytes.clone(),
            original_bytes,
            remapper,
        )?
    } else {
        UnicodeFontEncoder::from_font_bytes(subset_bytes.clone())?
    };

    Some((subset_bytes, encoder))
}
