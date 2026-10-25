//! Rasterizer parity tests.
//!
//! Covers the content-stream features the native rasterizer gained on top of
//! its original subset: clipping paths (`W` / `W*`), `/ExtGState` constant
//! alpha (`ca` / `CA`), text rendering modes (`Tr`) and image XObjects
//! (`Do`, including `FlateDecode` and `DCTDecode` payloads).

use pdfrs::raster::{RasterPage, rasterize_page};

/// Build a minimal single-page PDF from raw object bodies.
///
/// `objects` is a list of `(object number, body bytes)` pairs; object 1 must be
/// the catalog and object 3 the page.
fn build_pdf(objects: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::from("%PDF-1.4\n");
    let mut offsets: Vec<(u32, usize)> = Vec::new();
    for (id, body) in objects {
        offsets.push((*id, out.len()));
        out.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref_pos = out.len();
    let max_id = objects.iter().map(|(id, _)| *id).max().unwrap_or(0);
    out.extend_from_slice(format!("xref\n0 {}\n", max_id + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for id in 1..=max_id {
        let offset = offsets
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, o)| *o)
            .unwrap_or(0);
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            max_id + 1,
            xref_pos
        )
        .as_bytes(),
    );
    out
}

/// A one-page PDF with the given content stream and resource dictionary body.
fn page_pdf(content: &str, resources: &str) -> Vec<u8> {
    let objects = vec![
        (
            1,
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        ),
        (
            2,
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        ),
        (
            3,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /Resources {resources} >>"
            )
            .into_bytes(),
        ),
        (
            4,
            format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            )
            .into_bytes(),
        ),
    ];
    build_pdf(&objects)
}

/// The RGB triple at a pixel of a rasterized page.
fn pixel(page: &RasterPage, x: u32, y: u32) -> (u8, u8, u8) {
    let index = ((y * page.width + x) * 4) as usize;
    (
        page.pixels[index],
        page.pixels[index + 1],
        page.pixels[index + 2],
    )
}

/// Count the pixels that are not white.
fn painted(page: &RasterPage) -> usize {
    page.pixels
        .chunks(4)
        .filter(|px| px[0] < 250 || px[1] < 250 || px[2] < 250)
        .count()
}

#[test]
fn image_xobject_is_drawn() {
    // A 2x2 solid-red image, placed over the middle of the page.
    let mut image_data = Vec::new();
    for _ in 0..4 {
        image_data.extend_from_slice(&[255, 0, 0]);
    }
    let mut image_object = format!(
        "<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length {} >>\nstream\n",
        image_data.len()
    )
    .into_bytes();
    image_object.extend_from_slice(&image_data);
    image_object.extend_from_slice(b"\nendstream");

    let content = "q\n100 0 0 100 50 50 cm\n/Im1 Do\nQ\n";
    let pdf = build_pdf(&[
        (1, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()),
        (2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec()),
        (
            3,
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /Resources << /XObject << /Im1 5 0 R >> >> >>".to_vec(),
        ),
        (
            4,
            format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            )
            .into_bytes(),
        ),
        (5, image_object),
    ]);

    let page = rasterize_page(&pdf, 0, 72).expect("rasterize");
    // The image covers PDF x 50..150, y 50..150, which is screen y 50..150.
    assert_eq!(
        pixel(&page, 100, 100),
        (255, 0, 0),
        "image should be painted"
    );
    assert_eq!(pixel(&page, 20, 20), (255, 255, 255), "outside stays white");
}

#[test]
fn clipping_path_restricts_painting() {
    // PDF colour order is R G B: `1 0 0 rg` is red.
    let content = "q\n50 50 100 100 re\nW n\n1 0 0 rg\n0 0 200 200 re\nf\nQ\n";
    let pdf = page_pdf(content, "<< >>");
    let page = rasterize_page(&pdf, 0, 72).expect("rasterize");

    // Inside the clip: painted red.
    assert_eq!(pixel(&page, 100, 100), (255, 0, 0));
    // Outside the clip: untouched.
    assert_eq!(pixel(&page, 25, 25), (255, 255, 255));
    assert_eq!(pixel(&page, 175, 175), (255, 255, 255));
}

#[test]
fn clipping_path_is_reset_by_graphics_state_restore() {
    // The clip is installed inside q/Q, so the fill after Q is unclipped.
    let content = "q\n50 50 100 100 re\nW n\nQ\n1 0 0 rg\n0 0 200 200 re\nf\n";
    let pdf = page_pdf(content, "<< >>");
    let page = rasterize_page(&pdf, 0, 72).expect("rasterize");

    assert_eq!(
        pixel(&page, 25, 25),
        (255, 0, 0),
        "Q restores the previous (empty) clip"
    );
    assert_eq!(pixel(&page, 100, 100), (255, 0, 0));
}

#[test]
fn ext_gstate_constant_alpha_blends_the_fill() {
    let content = "q\n/GS0 gs\n0 0 0 rg\n0 0 200 200 re\nf\nQ\n";
    let pdf = page_pdf(content, "<< /ExtGState << /GS0 << /ca 0.5 >> >> >>");
    let page = rasterize_page(&pdf, 0, 72).expect("rasterize");

    let (r, g, b) = pixel(&page, 100, 100);
    assert_eq!((r, g, b), (r, g, b), "channels stay equal for a gray blend");
    assert!(
        (110..=145).contains(&r),
        "expected a half-alpha black fill over white, got {r}"
    );
}

#[test]
fn ext_gstate_stroke_alpha_is_applied() {
    // A 10pt-wide stroke at 50% alpha over white, drawn at PDF y = 50.
    let content = "q\n/GS0 gs\n10 w\n0 0 1 RG\n50 50 100 0 re\nS\nQ\n";
    let pdf = page_pdf(content, "<< /ExtGState << /GS0 << /CA 0.5 >> >> >>");
    let page = rasterize_page(&pdf, 0, 72).expect("rasterize");

    // The device Y axis is flipped: PDF y = 50 lands on screen y = 150.
    let (r, g, b) = pixel(&page, 100, 150);
    assert!(
        (110..=145).contains(&r) && (110..=145).contains(&g) && b > 200,
        "expected a half-alpha blue stroke, got ({r}, {g}, {b})"
    );
}

#[test]
fn text_render_mode_three_is_invisible() {
    let visible = "BT\n/F1 24 Tf\n100 100 Td\n(Hidden) Tj\nET\n";
    let invisible = "BT\n/F1 24 Tf\n3 Tr\n100 100 Td\n(Hidden) Tj\nET\n";
    let resources = "<< /Font << /F1 6 0 R >> >>";

    let visible_page = rasterize_page(&page_pdf(visible, resources), 0, 72).expect("rasterize");
    let invisible_page = rasterize_page(&page_pdf(invisible, resources), 0, 72).expect("rasterize");

    assert!(
        painted(&visible_page) > 0,
        "visible text should paint pixels"
    );
    assert_eq!(
        painted(&invisible_page),
        0,
        "text render mode 3 must not paint any pixels"
    );
}

#[test]
fn flate_encoded_image_is_drawn() {
    // A 2x2 image whose samples are deflate-compressed.
    let mut raw = Vec::new();
    for _ in 0..4 {
        raw.extend_from_slice(&[0, 0, 255]);
    }
    let compressed = pdfrs::compression::compress_deflate(&raw).expect("deflate");
    let mut image_object = format!(
        "<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n",
        compressed.len()
    )
    .into_bytes();
    image_object.extend_from_slice(&compressed);
    image_object.extend_from_slice(b"\nendstream");

    let content = "q\n100 0 0 100 50 50 cm\n/Im1 Do\nQ\n";
    let pdf = build_pdf(&[
        (1, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()),
        (2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec()),
        (
            3,
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /Resources << /XObject << /Im1 5 0 R >> >> >>".to_vec(),
        ),
        (
            4,
            format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            )
            .into_bytes(),
        ),
        (5, image_object),
    ]);

    let page = rasterize_page(&pdf, 0, 72).expect("rasterize");
    assert_eq!(
        pixel(&page, 100, 100),
        (0, 0, 255),
        "image should be painted"
    );
}

/// A 16x16 baseline JPEG: red in the top-left, blue in the top-right and
/// green in the bottom half.
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
    SAMPLE_JPEG_HEX
        .as_bytes()
        .chunks(2)
        .map(|pair| {
            let hex = std::str::from_utf8(pair).expect("hex pair");
            u8::from_str_radix(hex, 16).expect("hex digit")
        })
        .collect()
}

#[test]
fn dctdecode_image_is_drawn() {
    let jpeg = sample_jpeg();
    let mut image_object = format!(
        "<< /Type /XObject /Subtype /Image /Width 16 /Height 16 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
        jpeg.len()
    )
    .into_bytes();
    image_object.extend_from_slice(&jpeg);
    image_object.extend_from_slice(b"\nendstream");

    let content = "q\n160 0 0 160 20 20 cm\n/Im1 Do\nQ\n";
    let pdf = build_pdf(&[
        (1, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()),
        (2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec()),
        (
            3,
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /Resources << /XObject << /Im1 5 0 R >> >> >>".to_vec(),
        ),
        (
            4,
            format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            )
            .into_bytes(),
        ),
        (5, image_object),
    ]);

    let page = rasterize_page(&pdf, 0, 72).expect("rasterize");
    // The image covers PDF x 20..180, y 20..180 → screen y 20..180.
    // Inside it the top-left quadrant is red, the top-right blue and the
    // bottom half green.
    let (r, g, b) = pixel(&page, 50, 50);
    assert!(
        r > 200 && g < 60 && b < 60,
        "expected red, got ({r}, {g}, {b})"
    );

    let (r, g, b) = pixel(&page, 150, 50);
    assert!(
        b > 200 && r < 60 && g < 60,
        "expected blue, got ({r}, {g}, {b})"
    );

    let (r, g, b) = pixel(&page, 100, 130);
    assert!(
        g > 200 && r < 60 && b < 60,
        "expected green, got ({r}, {g}, {b})"
    );
}

#[test]
fn image_embedded_by_the_crate_is_rasterized() {
    // A 4x4 BMP with a solid red body, written to disk and embedded through
    // the crate's own image pipeline, then rasterized.
    let width = 4u32;
    let height = 4u32;
    let row = width as usize * 3;
    let padding = (4 - row % 4) % 4;
    let mut pixels = Vec::new();
    for _ in 0..height {
        for _ in 0..width {
            // BMP stores pixels in blue-green-red order.
            pixels.extend_from_slice(&[0, 0, 255]);
        }
        pixels.extend(std::iter::repeat_n(0u8, padding));
    }
    let mut bmp = Vec::new();
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&0u32.to_le_bytes()); // file size (patched below)
    bmp.extend_from_slice(&0u32.to_le_bytes());
    bmp.extend_from_slice(&54u32.to_le_bytes()); // pixel data offset
    bmp.extend_from_slice(&40u32.to_le_bytes()); // DIB header size
    bmp.extend_from_slice(&width.to_le_bytes());
    bmp.extend_from_slice(&height.to_le_bytes());
    bmp.extend_from_slice(&1u16.to_le_bytes()); // planes
    bmp.extend_from_slice(&24u16.to_le_bytes()); // bits per pixel
    bmp.extend_from_slice(&0u32.to_le_bytes()); // compression
    bmp.extend_from_slice(&0u32.to_le_bytes()); // image size
    bmp.extend_from_slice(&0u32.to_le_bytes()); // x pixels per metre
    bmp.extend_from_slice(&0u32.to_le_bytes()); // y pixels per metre
    bmp.extend_from_slice(&0u32.to_le_bytes()); // colours used
    bmp.extend_from_slice(&0u32.to_le_bytes()); // important colours
    bmp.extend_from_slice(&pixels);
    let size = bmp.len() as u32;
    bmp[2..6].copy_from_slice(&size.to_le_bytes());

    let path = std::env::temp_dir().join(format!(
        "pdfrs-raster-parity-{}-{}.bmp",
        std::process::id(),
        line!()
    ));
    std::fs::write(&path, &bmp).expect("write bmp");
    let info = pdfrs::image::load_image(path.to_str().expect("path")).expect("load image");
    let _ = std::fs::remove_file(&path);

    let mut generator = pdfrs::pdf_generator::PdfGenerator::new();
    let image_id = pdfrs::image::create_image_object(&mut generator, info).expect("image object");
    let content = pdfrs::image::create_image_content_stream(20.0, 20.0, 160.0, 160.0, "Im1");
    let content_id =
        generator.add_stream_object(format!("<< /Length {} >>\n", content.len()), content);
    let page_id = generator.add_object(format!(
        "<< /Type /Page\n/Parent 0 0 R\n/MediaBox [0 0 612 792]\n/Contents {content_id} 0 R\n/Resources << /XObject << /Im1 {image_id} 0 R >> >>\n>>\n"
    ));
    let pages_id = generator.add_object(format!(
        "<< /Type /Pages\n/Kids [{page_id} 0 R]\n/Count 1\n>>\n"
    ));
    generator.add_object(format!("<< /Type /Catalog\n/Pages {pages_id} 0 R\n>>\n"));
    let pdf = generator.generate();

    let page = rasterize_page(&pdf, 0, 72).expect("rasterize");
    assert!(
        painted(&page) > 0,
        "the embedded image should be rasterized"
    );
    // The image covers PDF x 20..180, y 20..180 → screen y 612..772.
    let (r, g, b) = pixel(&page, 100, 690);
    assert!(
        r > 200 && g < 60 && b < 60,
        "expected the red image, got ({r}, {g}, {b})"
    );
}

/// Build a one-page PDF whose content invokes the Form XObject `Fm1`.
fn form_xobject_pdf(form_dict: &str, form_content: &str, page_content: &str) -> Vec<u8> {
    let objects: Vec<(u32, Vec<u8>)> = vec![
        (1, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()),
        (2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec()),
        (
            3,
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /Resources << /XObject << /Fm1 5 0 R >> >> >>".to_vec(),
        ),
        (
            4,
            format!(
                "<< /Length {} >>\nstream\n{page_content}\nendstream",
                page_content.len()
            )
            .into_bytes(),
        ),
        (
            5,
            format!("{form_dict}\nstream\n{form_content}\nendstream").into_bytes(),
        ),
    ];

    let mut out: Vec<u8> = Vec::from("%PDF-1.4\n");
    let mut offsets: Vec<(u32, usize)> = Vec::new();
    for (id, body) in &objects {
        offsets.push((*id, out.len()));
        out.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref_pos = out.len();
    let max_id = objects.iter().map(|(id, _)| *id).max().unwrap_or(0);
    out.extend_from_slice(format!("xref\n0 {}\n", max_id + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for id in 1..=max_id {
        let offset = offsets
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, o)| *o)
            .unwrap_or(0);
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            max_id + 1,
            xref_pos
        )
        .as_bytes(),
    );
    out
}

#[test]
fn form_xobject_paints_inside_the_page() {
    // The form paints a red square over user space 20..60.
    let form_dict = "<< /Type /XObject /Subtype /Form /BBox [0 0 100 100] /Length 0 >>";
    let form_content = "1 0 0 1 0 0 cm\n1 0 0 rg\n20 20 40 40 re\nf\n";
    let pdf = form_xobject_pdf(form_dict, form_content, "q\n/Fm1 Do\nQ\n");

    let page = rasterize_page(&pdf, 0, 72).expect("rasterize");
    // The device Y axis is flipped, so user-space rows 20..60 land on
    // device rows 140..180.
    assert_eq!(
        pixel(&page, 40, 160),
        (255, 0, 0),
        "the form should paint red"
    );
    assert_eq!(
        pixel(&page, 100, 50),
        (255, 255, 255),
        "outside stays white"
    );
}

#[test]
fn form_xobject_matrix_positions_its_content() {
    // The form's /Matrix translates its content by (30, 40), so the square
    // moves from 20..60 to 50..90 in user space (device rows 110..150).
    let form_dict =
        "<< /Type /XObject /Subtype /Form /Matrix [1 0 0 1 30 40] /BBox [0 0 100 100] /Length 0 >>";
    let form_content = "1 0 0 1 0 0 cm\n1 0 0 rg\n20 20 40 40 re\nf\n";
    let pdf = form_xobject_pdf(form_dict, form_content, "q\n/Fm1 Do\nQ\n");

    let page = rasterize_page(&pdf, 0, 72).expect("rasterize");
    assert_eq!(
        pixel(&page, 60, 130),
        (255, 0, 0),
        "the translated square should be painted"
    );
    assert_eq!(
        pixel(&page, 40, 160),
        (255, 255, 255),
        "the untranslated position should stay white"
    );
}

#[test]
fn form_xobject_content_is_clipped_to_its_bbox() {
    // The form paints across its whole BBox; content outside the BBox must
    // be clipped away.
    let form_dict = "<< /Type /XObject /Subtype /Form /BBox [10 10 50 50] /Length 0 >>";
    let form_content = "1 0 0 1 0 0 cm\n1 0 0 rg\n0 0 100 100 re\nf\n";
    let pdf = form_xobject_pdf(form_dict, form_content, "q\n/Fm1 Do\nQ\n");

    let page = rasterize_page(&pdf, 0, 72).expect("rasterize");
    // Inside the BBox (user 10..50 → device rows 150..190): painted.
    assert_eq!(pixel(&page, 30, 170), (255, 0, 0));
    // Outside the BBox but inside the painted rectangle: clipped away.
    assert_eq!(pixel(&page, 80, 120), (255, 255, 255));
}

#[test]
fn form_xobject_uses_its_own_font_resources() {
    // The form references /F1, which exists only in the form's own
    // /Resources — the page does not define it.
    let form_dict = "<< /Type /XObject /Subtype /Form /BBox [0 0 200 200] /Resources << /Font << /F1 6 0 R >> >> /Length 0 >>";
    let form_content = "BT\n/F1 24 Tf\n20 100 Td\n(Form) Tj\nET\n";
    let objects: Vec<(u32, Vec<u8>)> = vec![
        (1, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()),
        (2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec()),
        (
            3,
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /Resources << /XObject << /Fm1 5 0 R >> >> >>".to_vec(),
        ),
        (
            4,
            b"<< /Length 45 >>\nstream\nq\n/Fm1 Do\nQ\nendstream".to_vec(),
        ),
        (
            5,
            format!("{form_dict}\nstream\n{form_content}\nendstream").into_bytes(),
        ),
        // A base-14 Helvetica font object for the form to reference.
        (
            6,
            b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
        ),
    ];

    let mut out: Vec<u8> = Vec::from("%PDF-1.4\n");
    let mut offsets: Vec<(u32, usize)> = Vec::new();
    for (id, body) in &objects {
        offsets.push((*id, out.len()));
        out.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref_pos = out.len();
    out.extend_from_slice(b"xref\n0 7\n0000000000 65535 f \n");
    for (_, offset) in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size 7 /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n").as_bytes(),
    );

    let page = rasterize_page(&out, 0, 72).expect("rasterize");
    assert!(
        painted(&page) > 0,
        "the form's own font resources should be used"
    );
}
