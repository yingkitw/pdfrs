//! `inspect` subcommand handlers.

use pdfrs::{i18n, pdf, redact, search};

pub(crate) fn cmd_validate(locale: pdfrs::i18n::Locale, input: String) {
    match pdf::validate_pdf(&input) {
        Ok(result) => {
            let result = i18n::localize_validation(locale, &result);
            println!(
                "{}",
                i18n::tf(locale, i18n::MsgId::ValidationResultFor, &[&input])
            );
            let yes_no = if result.valid {
                i18n::t(locale, i18n::MsgId::Yes)
            } else {
                i18n::t(locale, i18n::MsgId::No)
            };
            println!("  {}: {}", i18n::t(locale, i18n::MsgId::ValidLabel), yes_no);
            println!(
                "  {}: {}",
                i18n::t(locale, i18n::MsgId::PagesLabel),
                i18n::format_integer(locale, result.page_count as u64)
            );
            println!(
                "  {}: {}",
                i18n::t(locale, i18n::MsgId::ObjectsLabel),
                i18n::format_integer(locale, result.object_count as u64)
            );
            if !result.errors.is_empty() {
                println!("  {}:", i18n::t(locale, i18n::MsgId::ErrorsLabel));
                for e in &result.errors {
                    println!("    - {}", e);
                }
            }
            if !result.warnings.is_empty() {
                println!("  {}:", i18n::t(locale, i18n::MsgId::WarningsLabel));
                for w in &result.warnings {
                    println!("    - {}", w);
                }
            }
        }
        Err(e) => eprintln!(
            "{}",
            i18n::tf(locale, i18n::MsgId::ErrorValidatingPdf, &[&e.to_string()])
        ),
    }
}

pub(crate) fn cmd_validate_pdfa(input: String) {
    match pdf::validate_pdf_a(&input) {
        Ok(result) => {
            println!("PDF/A validation result for {}:", input);
            println!("  Level: {}", result.level);
            println!("  Compliant: {}", result.compliant);
            println!("  Embedded fonts: {}", result.embedded_fonts);
            println!("  Has XMP metadata: {}", result.has_xmp);
            println!("  Has encryption: {}", result.has_encryption);
            if !result.errors.is_empty() {
                println!("  Errors:");
                for e in &result.errors {
                    println!("    - {}", e);
                }
            }
            if !result.warnings.is_empty() {
                println!("  Warnings:");
                for w in &result.warnings {
                    println!("    - {}", w);
                }
            }
        }
        Err(e) => eprintln!("Error validating PDF/A: {}", e),
    }
}

pub(crate) fn cmd_validate_pdfa3(input: String) {
    match pdf::validate_pdf_a3(&input) {
        Ok(result) => {
            println!("PDF/A-3b validation result for {}:", input);
            println!("  Level: {}", result.level);
            println!("  Compliant: {}", result.compliant);
            println!("  Embedded fonts: {}", result.embedded_fonts);
            println!("  Has XMP metadata: {}", result.has_xmp);
            println!("  Has encryption: {}", result.has_encryption);
            if !result.errors.is_empty() {
                println!("  Errors:");
                for e in &result.errors {
                    println!("    - {}", e);
                }
            }
            if !result.warnings.is_empty() {
                println!("  Warnings:");
                for w in &result.warnings {
                    println!("    - {}", w);
                }
            }
        }
        Err(e) => eprintln!("Error validating PDF/A-3b: {}", e),
    }
}

pub(crate) fn cmd_validate_pdfua(input: String) {
    match pdf::validate_pdf_ua(&input) {
        Ok(result) => {
            println!("PDF/UA validation result for {}:", input);
            println!("  Compliant: {}", result.compliant);
            println!("  MarkInfo: {}", result.has_mark_info);
            println!("  StructTreeRoot: {}", result.has_struct_tree);
            println!("  Lang: {}", result.has_lang);
            println!("  Title: {}", result.has_title);
            println!("  Fonts embedded: {}", result.fonts_embedded);
            if !result.errors.is_empty() {
                println!("  Errors:");
                for e in &result.errors {
                    println!("    - {}", e);
                }
            }
            if !result.warnings.is_empty() {
                println!("  Warnings:");
                for w in &result.warnings {
                    println!("    - {}", w);
                }
            }
        }
        Err(e) => eprintln!("Error validating PDF/UA: {}", e),
    }
}

pub(crate) fn cmd_check_screen_reader(input: String) {
    {
        match pdf::check_screen_reader_compliance(&input) {
            Ok(report) => {
                println!("Screen reader compliance for {}:", input);
                println!("  Compliant: {}", report.compliant);
                println!("  Text extractable: {}", report.text_extractable);
                println!("  Extracted text length: {}", report.extracted_text_length);
                if !report.structure_element_types.is_empty() {
                    println!(
                        "  Structure types: {}",
                        report.structure_element_types.join(", ")
                    );
                }
                if !report.issues.is_empty() {
                    println!("  Issues:");
                    for issue in &report.issues {
                        println!("    - {}", issue);
                    }
                }
                if !report.warnings.is_empty() {
                    println!("  Warnings:");
                    for warning in &report.warnings {
                        println!("    - {}", warning);
                    }
                }
            }
            Err(e) => eprintln!("Error checking screen reader compliance: {}", e),
        }
    }
}

pub(crate) fn cmd_diff_pdfs(old: String, new: String) {
    match (std::fs::read(&old), std::fs::read(&new)) {
        (Ok(old_bytes), Ok(new_bytes)) => match pdf::diff_pdf_bytes(&old_bytes, &new_bytes) {
            Ok(diff) => {
                println!("PDF diff: {} -> {}", old, new);
                println!(
                    "  Objects: {} -> {}",
                    diff.object_count_old, diff.object_count_new
                );
                println!("  Pages: {} -> {}", diff.pages_old, diff.pages_new);
                println!("  Text similarity: {:.1}%", diff.text_similarity * 100.0);
                println!("  Added objects: {:?}", diff.added_objects);
                println!("  Removed objects: {:?}", diff.removed_objects);
                println!("  Modified objects: {:?}", diff.modified_objects);
                println!("  Metadata changed: {}", diff.metadata_changed);
                println!("  Embedded files (old): {}", diff.has_embedded_files_old);
                println!("  Embedded files (new): {}", diff.has_embedded_files_new);
            }
            Err(e) => eprintln!("Error diffing PDFs: {}", e),
        },
        _ => eprintln!("Error reading one or both PDF files"),
    }
}

pub(crate) fn cmd_search_pdf(
    input: String,
    query: String,
    case_insensitive: bool,
    json: Option<String>,
) {
    {
        let bytes = match std::fs::read(&input) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Error reading PDF: {}", e);
                return;
            }
        };
        let hits = search::search_text(&bytes, &query, case_insensitive);
        if let Some(path) = json {
            let json_value = serde_json::json!({
                "query": query,
                "case_insensitive": case_insensitive,
                "total": hits.len(),
                "hits": hits.iter().map(|h| serde_json::json!({
                    "page": h.page,
                    "text": h.text,
                    "snippet": h.snippet,
                    "bbox": {
                        "x": h.bbox.x,
                        "y": h.bbox.y,
                        "width": h.bbox.width,
                        "height": h.bbox.height,
                    },
                })).collect::<Vec<_>>(),
            });
            let pretty = serde_json::to_string_pretty(&json_value).unwrap_or_default();
            if let Err(e) = std::fs::write(&path, pretty) {
                eprintln!("Error writing JSON: {}", e);
            }
        }
        if hits.is_empty() {
            println!("No matches for {:?}", query);
        } else {
            println!("Found {} match(es) for {:?}:", hits.len(), query);
            for h in &hits {
                println!(
                    "  page {} [{:.1},{:.1} {:.1}×{:.1}]: {}",
                    h.page + 1,
                    h.bbox.x,
                    h.bbox.y,
                    h.bbox.width,
                    h.bbox.height,
                    h.snippet
                );
            }
        }
    }
}

pub(crate) fn cmd_redact_pdf(input: String, output: String, region: Vec<String>, strip: bool) {
    {
        if region.is_empty() {
            eprintln!("Error: provide at least one --region page,x,y,w,h");
            return;
        }
        let mut regions = Vec::new();
        for spec in &region {
            let parts: Vec<&str> = spec.split(',').collect();
            if parts.len() != 5 {
                eprintln!("Error parsing region '{}': expected page,x,y,w,h", spec);
                return;
            }
            let parsed: Result<Vec<f32>, _> =
                parts.iter().map(|s| s.trim().parse::<f32>()).collect();
            let nums = match parsed {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Error parsing region numbers '{}': {}", spec, e);
                    return;
                }
            };
            regions.push(redact::RedactionRegion {
                page: nums[0] as usize,
                x: nums[1],
                y: nums[2],
                width: nums[3],
                height: nums[4],
            });
        }
        let bytes = match std::fs::read(&input) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Error reading PDF: {}", e);
                return;
            }
        };
        let style = if strip {
            redact::RedactionStyle::Strip
        } else {
            redact::RedactionStyle::BlackBox
        };
        match redact::redact_pdf_bytes_with_style(&bytes, &regions, style) {
            Ok(out) => match std::fs::write(&output, out) {
                Ok(_) => println!(
                    "Wrote redacted PDF {} ({} region(s), style={:?})",
                    output,
                    regions.len(),
                    style
                ),
                Err(e) => eprintln!("Error writing PDF: {}", e),
            },
            Err(e) => eprintln!("Error redacting PDF: {}", e),
        }
    }
}
