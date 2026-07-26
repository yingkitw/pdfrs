//! PDF annotation types: text, link, highlight, and 3D (U3D).

use anyhow::{anyhow, Result};
use std::fs;

/// A text annotation to be placed on a PDF page
#[derive(Debug, Clone)]
pub struct TextAnnotation {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub content: String,
    pub title: String,
}

/// A link annotation (clickable URL region)
#[derive(Debug, Clone)]
pub struct LinkAnnotation {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub url: String,
}

/// A highlight annotation (colored rectangle over text)
#[derive(Debug, Clone)]
pub struct HighlightAnnotation {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color_r: f32,
    pub color_g: f32,
    pub color_b: f32,
}

/// A 3D annotation referencing an embedded U3D stream (PDF 1.6+ / ISO 32000 §13.6).
#[derive(Debug, Clone)]
pub struct ThreeDAnnotation {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// Optional tooltip / contents string shown by viewers.
    pub contents: String,
    /// Activate when the page is opened (`/A /PO`).
    pub activate_on_open: bool,
}

impl Default for ThreeDAnnotation {
    fn default() -> Self {
        Self {
            x: 72.0,
            y: 200.0,
            width: 400.0,
            height: 300.0,
            contents: "3D Model".to_string(),
            activate_on_open: true,
        }
    }
}

/// Create a single-page PDF embedding U3D data as a `/Subtype /3D` annotation.
///
/// The U3D bytes are stored in a `/Type /3D` / `/Subtype /U3D` stream referenced by
/// the annotation's `/3DD` entry. Viewers that support U3D (e.g. Adobe Acrobat) can
/// render the model in the annotation rectangle.
pub fn create_pdf_with_3d_annotation(
    output_file: &str,
    page_label: &str,
    u3d_data: &[u8],
    annot: &ThreeDAnnotation,
) -> Result<()> {
    let bytes = create_pdf_with_3d_annotation_bytes(page_label, u3d_data, annot)?;
    fs::write(output_file, bytes)?;
    println!(
        "[3d] Created {} with U3D annotation ({} bytes model)",
        output_file,
        u3d_data.len()
    );
    Ok(())
}

/// In-memory variant of [`create_pdf_with_3d_annotation`].
pub fn create_pdf_with_3d_annotation_bytes(
    page_label: &str,
    u3d_data: &[u8],
    annot: &ThreeDAnnotation,
) -> Result<Vec<u8>> {
    if u3d_data.is_empty() {
        return Err(anyhow!("U3D data must not be empty"));
    }

    let layout = crate::pdf_generator::PageLayout::portrait();
    let mut generator = crate::pdf_generator::PdfGenerator::new();

    // 1. 3D stream (U3D)
    let stream_dict = format!(
        "<< /Type /3D\n/Subtype /U3D\n/Length {} >>\n",
        u3d_data.len()
    );
    let stream_id = generator.add_stream_object(stream_dict, u3d_data.to_vec());

    // 2. 3D annotation
    let activation = if annot.activate_on_open {
        "/3DA << /A /PO /TB true /NP true >>\n"
    } else {
        "/3DA << /A /XA /TB true /NP true >>\n"
    };
    let annot_dict = format!(
        "<< /Type /Annot\n\
         /Subtype /3D\n\
         /Rect [{} {} {} {}]\n\
         /Contents ({})\n\
         /3DD {} 0 R\n\
         {}\
         /F 4\n\
         >>\n",
        annot.x,
        annot.y,
        annot.x + annot.width,
        annot.y + annot.height,
        super::escape_pdf_meta(&annot.contents),
        stream_id,
        activation,
    );
    let annot_id = generator.add_object(annot_dict);

    // 3. Page content (simple label above the 3D rect)
    let label_y = (annot.y + annot.height + 20.0).min(layout.height - 36.0);
    let page_stream = format!(
        "BT\n/F1 14 Tf\n1 0 0 1 72 {} Tm\n({}) Tj\nET\n",
        label_y,
        super::escape_pdf_meta(page_label),
    );
    let content_id = generator.add_stream_object(
        format!("<< /Length {} >>\n", page_stream.len()),
        page_stream.into_bytes(),
    );

    // Pre-compute pages/catalog IDs: content, font, page, pages, catalog
    let font_id = content_id + 1;
    let page_id = content_id + 2;
    let pages_id = content_id + 3;

    let page_dict = format!(
        "<< /Type /Page\n\
         /Parent {} 0 R\n\
         /MediaBox [0 0 {} {}]\n\
         /Contents {} 0 R\n\
         /Annots [{} 0 R]\n\
         /Resources << /Font << /F1 {} 0 R >> >>\n\
         >>\n",
        pages_id, layout.width, layout.height, content_id, annot_id, font_id
    );
    let actual_font_id = generator
        .add_object("<< /Type /Font\n/Subtype /Type1\n/BaseFont /Helvetica\n>>\n".to_string());
    assert_eq!(actual_font_id, font_id);
    let actual_page_id = generator.add_object(page_dict);
    assert_eq!(actual_page_id, page_id);

    let pages_dict = format!(
        "<< /Type /Pages\n/Kids [{} 0 R]\n/Count 1\n>>\n",
        page_id
    );
    let actual_pages_id = generator.add_object(pages_dict);
    assert_eq!(actual_pages_id, pages_id);
    generator.add_object(format!(
        "<< /Type /Catalog\n/Pages {} 0 R\n>>\n",
        actual_pages_id
    ));

    Ok(generator.generate())
}

/// Returns true if the PDF bytes contain a 3D annotation and a U3D stream.
pub fn pdf_contains_3d_u3d(pdf_bytes: &[u8]) -> bool {
    let text = String::from_utf8_lossy(pdf_bytes);
    text.contains("/Subtype /3D") && text.contains("/Subtype /U3D") && text.contains("/3DD")
}

/// Create a PDF with text, link, and highlight annotations
pub fn create_pdf_with_all_annotations(
    output_file: &str,
    text: &str,
    annotations: &[TextAnnotation],
    links: &[LinkAnnotation],
    highlights: &[HighlightAnnotation],
) -> Result<()> {
    let elements = crate::elements::parse_markdown(text);
    let layout = crate::pdf_generator::PageLayout::portrait();
    let page_streams = super::build_page_streams(&elements, 12.0, true, layout, None)?;
    if page_streams.is_empty() {
        return Err(anyhow!("No page content generated"));
    }

    let mut generator = crate::pdf_generator::PdfGenerator::new();
    let mut annot_ids: Vec<u32> = Vec::new();

    for annot in annotations {
        let annot_dict = format!(
            "<< /Type /Annot\n/Subtype /Text\n/Rect [{} {} {} {}]\n/Contents ({})\n/T ({})\n/Open false\n>>\n",
            annot.x, annot.y, annot.x + annot.width, annot.y + annot.height,
            super::escape_pdf_meta(&annot.content), super::escape_pdf_meta(&annot.title),
        );
        annot_ids.push(generator.add_object(annot_dict));
    }

    for link in links {
        let link_dict = format!(
            "<< /Type /Annot\n/Subtype /Link\n/Rect [{} {} {} {}]\n/Border [0 0 0]\n/A << /Type /Action\n/S /URI\n/URI ({}) >>\n>>\n",
            link.x, link.y, link.x + link.width, link.y + link.height,
            super::escape_pdf_meta(&link.url),
        );
        annot_ids.push(generator.add_object(link_dict));
    }

    for hl in highlights {
        let hl_dict = format!(
            "<< /Type /Annot\n/Subtype /Highlight\n/Rect [{} {} {} {}]\n/C [{} {} {}]\n/QuadPoints [{} {} {} {} {} {} {} {}]\n>>\n",
            hl.x, hl.y, hl.x + hl.width, hl.y + hl.height,
            hl.color_r, hl.color_g, hl.color_b,
            hl.x, hl.y + hl.height, hl.x + hl.width, hl.y + hl.height,
            hl.x, hl.y, hl.x + hl.width, hl.y,
        );
        annot_ids.push(generator.add_object(hl_dict));
    }

    let annot_offset = annot_ids.len() as u32;
    let pages_obj_id = annot_offset + (page_streams.len() as u32) * 3 + 1;
    let mut page_ids = Vec::new();

    for (i, page_stream) in page_streams.iter().enumerate() {
        let content_id = generator.add_stream_object(
            format!("<< /Length {} >>\n", page_stream.len()),
            page_stream.clone(),
        );
        let font_id = content_id + 2;
        let annots_str = if i == 0 && !annot_ids.is_empty() {
            let refs: Vec<String> = annot_ids.iter().map(|id| format!("{} 0 R", id)).collect();
            format!("/Annots [{}]\n", refs.join(" "))
        } else {
            String::new()
        };
        let page_dict = format!(
            "<< /Type /Page\n/Parent {} 0 R\n/MediaBox [0 0 {} {}]\n/Contents {} 0 R\n{}/Resources << /Font << /F1 {} 0 R >> >>\n>>\n",
            pages_obj_id, layout.width, layout.height, content_id, annots_str, font_id
        );
        let page_id = generator.add_object(page_dict);
        page_ids.push(page_id);
        generator.add_object("<< /Type /Font\n/Subtype /Type1\n/BaseFont /Helvetica\n>>\n".to_string());
    }

    let kids: Vec<String> = page_ids.iter().map(|id| format!("{} 0 R", id)).collect();
    let pages_dict = format!("<< /Type /Pages\n/Kids [{}]\n/Count {}\n>>\n", kids.join(" "), page_ids.len());
    let actual_pages_id = generator.add_object(pages_dict);
    assert_eq!(actual_pages_id, pages_obj_id);
    generator.add_object(format!("<< /Type /Catalog\n/Pages {} 0 R\n>>\n", actual_pages_id));

    let pdf_data = generator.generate();
    let mut file = std::fs::File::create(output_file)?;
    std::io::Write::write_all(&mut file, &pdf_data)?;
    println!(
        "[annotate] Created {} with {} text, {} link, {} highlight annotations",
        output_file, annotations.len(), links.len(), highlights.len()
    );
    Ok(())
}

/// Create a single-page PDF with text annotations (backward compatible)
pub fn create_pdf_with_annotations(
    output_file: &str,
    text: &str,
    annotations: &[TextAnnotation],
    links: &[LinkAnnotation],
) -> Result<()> {
    let elements = crate::elements::parse_markdown(text);
    let layout = crate::pdf_generator::PageLayout::portrait();

    // Build page content
    let page_streams = super::build_page_streams(&elements, 12.0, true, layout, None)?;
    if page_streams.is_empty() {
        return Err(anyhow!("No page content generated"));
    }

    let mut generator = crate::pdf_generator::PdfGenerator::new();

    // Build annotation objects first, collect their IDs
    let mut annot_ids: Vec<u32> = Vec::new();

    for annot in annotations {
        let annot_dict = format!(
            "<< /Type /Annot\n\
             /Subtype /Text\n\
             /Rect [{} {} {} {}]\n\
             /Contents ({})\n\
             /T ({})\n\
             /Open false\n\
             >>\n",
            annot.x,
            annot.y,
            annot.x + annot.width,
            annot.y + annot.height,
            super::escape_pdf_meta(&annot.content),
            super::escape_pdf_meta(&annot.title),
        );
        annot_ids.push(generator.add_object(annot_dict));
    }

    for link in links {
        let link_dict = format!(
            "<< /Type /Annot\n\
             /Subtype /Link\n\
             /Rect [{} {} {} {}]\n\
             /Border [0 0 0]\n\
             /A << /Type /Action\n/S /URI\n/URI ({}) >>\n\
             >>\n",
            link.x,
            link.y,
            link.x + link.width,
            link.y + link.height,
            super::escape_pdf_meta(&link.url),
        );
        annot_ids.push(generator.add_object(link_dict));
    }

    let annot_offset = annot_ids.len() as u32;

    // Now add page content streams and pages
    // pages_obj_id = annot_offset + page_streams.len() * 3 + 1
    let pages_obj_id = annot_offset + (page_streams.len() as u32) * 3 + 1;

    let mut page_ids = Vec::new();
    for (i, page_stream) in page_streams.iter().enumerate() {
        let content_id = generator.add_stream_object(
            format!("<< /Length {} >>\n", page_stream.len()),
            page_stream.clone(),
        );
        let font_id = content_id + 2;

        // Only first page gets annotations
        let annots_str = if i == 0 && !annot_ids.is_empty() {
            let refs: Vec<String> = annot_ids.iter().map(|id| format!("{} 0 R", id)).collect();
            format!("/Annots [{}]\n", refs.join(" "))
        } else {
            String::new()
        };

        let page_dict = format!(
            "<< /Type /Page\n\
             /Parent {} 0 R\n\
             /MediaBox [0 0 {} {}]\n\
             /Contents {} 0 R\n\
             {}\
             /Resources << /Font << /F1 {} 0 R >> >>\n\
             >>\n",
            pages_obj_id, layout.width, layout.height, content_id, annots_str, font_id
        );
        let page_id = generator.add_object(page_dict);
        page_ids.push(page_id);

        let font_dict = "<< /Type /Font\n/Subtype /Type1\n/BaseFont /Helvetica\n>>\n".to_string();
        generator.add_object(font_dict);
    }

    let kids: Vec<String> = page_ids.iter().map(|id| format!("{} 0 R", id)).collect();
    let pages_dict = format!(
        "<< /Type /Pages\n/Kids [{}]\n/Count {}\n>>\n",
        kids.join(" "),
        page_ids.len()
    );
    let actual_pages_id = generator.add_object(pages_dict);
    assert_eq!(actual_pages_id, pages_obj_id);

    let catalog_dict = format!(
        "<< /Type /Catalog\n/Pages {} 0 R\n>>\n",
        actual_pages_id
    );
    generator.add_object(catalog_dict);

    let pdf_data = generator.generate();
    let mut file = std::fs::File::create(output_file)?;
    std::io::Write::write_all(&mut file, &pdf_data)?;
    println!(
        "[annotate] Created {} with {} text annotations, {} link annotations",
        output_file,
        annotations.len(),
        links.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_pdf_with_3d_annotation_bytes() {
        let u3d = b"U3D\0fake-model-bytes-for-structure-test";
        let annot = ThreeDAnnotation {
            contents: "Demo".into(),
            activate_on_open: true,
            ..Default::default()
        };
        let bytes = create_pdf_with_3d_annotation_bytes("3D Demo", u3d, &annot).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        assert!(pdf_contains_3d_u3d(&bytes));
        assert!(bytes.windows(u3d.len()).any(|w| w == u3d));
        let validation = crate::pdf::validate_pdf_bytes(&bytes);
        assert!(validation.valid, "{:?}", validation.errors);
    }

    #[test]
    fn test_3d_annotation_rejects_empty_u3d() {
        let annot = ThreeDAnnotation::default();
        let err = create_pdf_with_3d_annotation_bytes("x", b"", &annot).unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn test_text_annotation_struct() {
        let annot = TextAnnotation {
            x: 100.0,
            y: 700.0,
            width: 200.0,
            height: 20.0,
            content: "A note".into(),
            title: "Author".into(),
        };
        assert_eq!(annot.content, "A note");
        assert_eq!(annot.x, 100.0);
    }

    #[test]
    fn test_link_annotation_struct() {
        let link = LinkAnnotation {
            x: 72.0,
            y: 500.0,
            width: 100.0,
            height: 15.0,
            url: "https://example.com".into(),
        };
        assert_eq!(link.url, "https://example.com");
    }

    #[test]
    fn test_highlight_annotation_struct() {
        let hl = HighlightAnnotation {
            x: 72.0,
            y: 700.0,
            width: 200.0,
            height: 12.0,
            color_r: 1.0,
            color_g: 1.0,
            color_b: 0.0,
        };
        assert_eq!(hl.color_r, 1.0);
        assert_eq!(hl.color_g, 1.0);
        assert_eq!(hl.color_b, 0.0);
    }
}
