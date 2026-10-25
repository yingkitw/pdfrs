//! Shared font resources: base-14 Type1 fonts and the embedded Type0/CIDFont
//! used when a document contains non-ASCII characters.

use super::content_stream::{
    FONT_COURIER, FONT_HELVETICA, FONT_HELVETICA_BOLD, FONT_HELVETICA_BOLD_OBLIQUE,
    FONT_HELVETICA_OBLIQUE,
};
use super::objects::PdfGenerator;
use super::unicode_support::UnicodeFontEncoder;

#[derive(Debug, Clone, Copy)]
pub(super) struct FontResourceIds {
    pub(super) helvetica: u32,
    pub(super) helvetica_bold: u32,
    pub(super) helvetica_oblique: u32,
    pub(super) helvetica_bold_oblique: u32,
    pub(super) courier: u32,
}

pub(super) fn add_shared_font_resources(
    generator: &mut PdfGenerator,
    unicode_font: Option<(
        &[u8],
        &UnicodeFontEncoder,
        &std::collections::BTreeSet<char>,
    )>,
) -> FontResourceIds {
    let helvetica_id = if let Some((bytes, encoder, chars)) = unicode_font {
        let font_file_id =
            generator.add_stream_object(format!("<< /Length {} >>\n", bytes.len()), bytes.to_vec());

        let descriptor_id = generator.add_object(format!(
            "<< /Type /FontDescriptor\n/FontName /UnicodeTT\n/Flags 4\n/FontBBox [0 -200 1000 900]\n/ItalicAngle 0\n/Ascent 800\n/Descent -200\n/CapHeight 700\n/StemV 80\n/MissingWidth 500\n/FontFile2 {} 0 R\n>>\n",
            font_file_id
        ));

        let widths = encoder.build_cid_widths_array(chars);
        let cid_font_id = generator.add_object(format!(
            "<< /Type /Font\n/Subtype /CIDFontType2\n/BaseFont /UnicodeTT\n/CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >>\n/FontDescriptor {} 0 R\n/DW 500\n/W {}\n/CIDToGIDMap /Identity\n>>\n",
            descriptor_id, widths
        ));

        let tounicode = encoder.build_tounicode_cmap(chars);
        let tounicode_id =
            generator.add_stream_object(format!("<< /Length {} >>\n", tounicode.len()), tounicode);

        generator.add_object(format!(
            "<< /Type /Font\n/Subtype /Type0\n/BaseFont /UnicodeTT\n/Encoding /Identity-H\n/DescendantFonts [{} 0 R]\n/ToUnicode {} 0 R\n>>\n",
            cid_font_id, tounicode_id
        ))
    } else {
        generator.add_object(format!(
            "<< /Type /Font\n/Subtype /Type1\n/BaseFont /{}\n>>\n",
            FONT_HELVETICA
        ))
    };

    let (helvetica_bold_id, helvetica_oblique_id, helvetica_bold_oblique_id) =
        if unicode_font.is_some() {
            // Reuse the same embedded Type0/CIDFont for style variants to keep
            // unicode/maths glyph coverage in italic/bold rendering paths.
            (helvetica_id, helvetica_id, helvetica_id)
        } else {
            (
                generator.add_object(format!(
                    "<< /Type /Font\n/Subtype /Type1\n/BaseFont /{}\n>>\n",
                    FONT_HELVETICA_BOLD
                )),
                generator.add_object(format!(
                    "<< /Type /Font\n/Subtype /Type1\n/BaseFont /{}\n>>\n",
                    FONT_HELVETICA_OBLIQUE
                )),
                generator.add_object(format!(
                    "<< /Type /Font\n/Subtype /Type1\n/BaseFont /{}\n>>\n",
                    FONT_HELVETICA_BOLD_OBLIQUE
                )),
            )
        };

    let courier_id = generator.add_object(format!(
        "<< /Type /Font\n/Subtype /Type1\n/BaseFont /{}\n>>\n",
        FONT_COURIER
    ));

    FontResourceIds {
        helvetica: helvetica_id,
        helvetica_bold: helvetica_bold_id,
        helvetica_oblique: helvetica_oblique_id,
        helvetica_bold_oblique: helvetica_bold_oblique_id,
        courier: courier_id,
    }
}
