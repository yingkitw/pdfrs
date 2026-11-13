import pdfrs


def test_version_matches_crate():
    assert pdfrs.version()


def test_render_markdown_to_pdf():
    data = pdfrs.render_markdown_to_pdf("# Hello\n\nFrom **Python**.")
    assert data.startswith(b"%PDF")
    report = pdfrs.validate_pdf_bytes(data)
    assert report["valid"]
    assert report["page_count"] >= 1


def test_merge_pdfs():
    a = pdfrs.render_markdown_to_pdf("# One")
    b = pdfrs.render_markdown_to_pdf("# Two")
    merged = pdfrs.merge_pdfs([a, b])
    assert merged.startswith(b"%PDF")
    report = pdfrs.validate_pdf_bytes(merged)
    assert report["valid"]
    assert report["page_count"] >= 2
