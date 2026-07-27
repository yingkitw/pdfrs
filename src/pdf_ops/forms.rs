//! Interactive form fields: creation, detection, and filling.

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::fs;

/// Form field types.
///
/// Represents the type of interactive form field that can be added to a PDF.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FormFieldType {
    /// Text input field
    Text,
    /// Checkbox field
    Checkbox,
    /// Radio button field
    Radio,
    /// Dropdown/combobox field
    Dropdown,
}

/// A form field to be added to a PDF.
///
/// Represents an interactive form field with its properties including
/// position, dimensions, default value, options (for radio/dropdown), and
/// whether the field is required.
///
/// # Fields
///
/// * `name` - Unique identifier for the form field
/// * `field_type` - Type of form field (Text, Checkbox, Radio, Dropdown)
/// * `x` - X position on the page (in PDF points)
/// * `y` - Y position on the page (in PDF points)
/// * `width` - Width of the field (in PDF points)
/// * `height` - Height of the field (in PDF points)
/// * `default_value` - Optional default value for the field
/// * `options` - List of options (for radio buttons and dropdowns)
/// * `required` - Whether the field must be filled
///
/// # Example
///
/// ```rust,no_run
/// use pdfrs::pdf_ops::{FormField, FormFieldType};
///
/// let field = FormField {
///     name: "firstName".to_string(),
///     field_type: FormFieldType::Text,
///     x: 100.0,
///     y: 700.0,
///     width: 200.0,
///     height: 20.0,
///     default_value: Some("John".to_string()),
///     options: vec![],
///     required: true,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormField {
    pub name: String,
    pub field_type: FormFieldType,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub default_value: Option<String>,
    pub options: Vec<String>, // For radio/dropdown
    pub required: bool,
}

/// Create a PDF with an AcroForm containing interactive form fields
pub fn create_pdf_with_form_fields(
    output_file: &str,
    text: &str,
    form_fields: &[FormField],
) -> Result<()> {
    let elements = crate::elements::parse_markdown(text);
    let layout = crate::pdf_generator::PageLayout::portrait();
    let page_streams = super::build_page_streams(&elements, 12.0, true, layout, None)?;
    if page_streams.is_empty() {
        return Err(anyhow!("No page content generated"));
    }

    let mut generator = crate::pdf_generator::PdfGenerator::new();
    let mut field_ids: Vec<u32> = Vec::new();

    // Create form field annotations
    for field in form_fields {
        let field_dict = create_form_field_dict(field);
        field_ids.push(generator.add_object(field_dict));
    }

    // Create AcroForm dictionary
    let kids_refs: Vec<String> = field_ids.iter().map(|id| format!("{} 0 R", id)).collect();
    let acroform_dict = format!("<< /Fields [{}]\n>>\n", kids_refs.join(" "));
    let acroform_id = generator.add_object(acroform_dict);

    let field_offset = field_ids.len() as u32;
    let pages_obj_id = field_offset + 1 + (page_streams.len() as u32) * 3 + 1;
    let mut page_ids = Vec::new();

    for (i, page_stream) in page_streams.iter().enumerate() {
        let content_id = generator.add_stream_object(
            format!("<< /Length {} >>\n", page_stream.len()),
            page_stream.clone(),
        );
        let font_id = content_id + 2;

        // Only first page gets form fields
        let annots_str = if i == 0 && !field_ids.is_empty() {
            let refs: Vec<String> = field_ids.iter().map(|id| format!("{} 0 R", id)).collect();
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
        generator
            .add_object("<< /Type /Font\n/Subtype /Type1\n/BaseFont /Helvetica\n>>\n".to_string());
    }

    let kids: Vec<String> = page_ids.iter().map(|id| format!("{} 0 R", id)).collect();
    let pages_dict = format!(
        "<< /Type /Pages\n/Kids [{}]\n/Count {}\n>>\n",
        kids.join(" "),
        page_ids.len()
    );
    let actual_pages_id = generator.add_object(pages_dict);

    let catalog_dict = format!(
        "<< /Type /Catalog\n/Pages {} 0 R\n/AcroForm {} 0 R\n>>\n",
        actual_pages_id, acroform_id
    );
    generator.add_object(catalog_dict);

    let pdf_data = generator.generate();
    let mut file = std::fs::File::create(output_file)?;
    std::io::Write::write_all(&mut file, &pdf_data)?;
    println!(
        "[form] Created {} with {} form fields",
        output_file,
        form_fields.len()
    );
    Ok(())
}

/// Create a form field annotation dictionary
fn create_form_field_dict(field: &FormField) -> String {
    let base_dict = format!(
        "<< /Type /Annot\n/Subtype /Widget\n\
         /Rect [{} {} {} {}]\n\
         /FT {}\n\
         /T ({})\n",
        field.x,
        field.y,
        field.x + field.width,
        field.y + field.height,
        field_type_to_pdf(&field.field_type),
        super::escape_pdf_meta(&field.name)
    );

    let mut dict = base_dict;

    // Add default value if present
    if let Some(ref value) = field.default_value {
        dict.push_str(&format!("/V ({})\n", super::escape_pdf_meta(value)));
    }

    // Add field-type specific properties
    match field.field_type {
        FormFieldType::Text => {
            dict.push_str(&format!(
                "/Ff {}\n",
                if field.required { 2 } else { 0 } // 2 = Required flag
            ));
            // Appearance for text field
            dict.push_str("/AP << /N << /Type /Appearance\n/Length 0 >> >>\n");
        }
        FormFieldType::Checkbox => {
            dict.push_str(&format!(
                "/V /Off\n/Ff {}\n",
                if field.required { 2 } else { 0 }
            ));
            // Appearance for checkbox
            dict.push_str("/AP << /N << /Type /Appearance\n/Length 0 >> >>\n");
        }
        FormFieldType::Radio => {
            if !field.options.is_empty() {
                let opts: Vec<String> = field
                    .options
                    .iter()
                    .map(|o| format!("({})", super::escape_pdf_meta(o)))
                    .collect();
                dict.push_str(&format!("/Opt [{}]\n", opts.join(" ")));
            }
            dict.push_str(&format!(
                "/V /Off\n/Ff {}\n",
                if field.required { 2 } else { 0 }
            ));
        }
        FormFieldType::Dropdown => {
            if !field.options.is_empty() {
                let opts: Vec<String> = field
                    .options
                    .iter()
                    .map(|o| format!("({})", super::escape_pdf_meta(o)))
                    .collect();
                dict.push_str(&format!("/Opt [{}]\n", opts.join(" ")));
            }
            dict.push_str(&format!(
                "/Ff {}131072\n",
                if field.required { 2 + 131072 } else { 131072 } // 131072 = Combo flags
            ));
        }
    }

    dict.push_str(">>\n");
    dict
}

/// Convert FormFieldType to PDF field type string
fn field_type_to_pdf(field_type: &FormFieldType) -> String {
    match field_type {
        FormFieldType::Text => "/Tx".to_string(),
        FormFieldType::Checkbox => "/Btn".to_string(),
        FormFieldType::Radio => "/Btn".to_string(),
        FormFieldType::Dropdown => "/Ch".to_string(),
    }
}

/// A form field detected in an existing PDF document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectedFormField {
    pub name: String,
    pub field_type: String,
    pub value: Option<String>,
    pub options: Vec<String>,
    pub required: bool,
}

/// Detect all interactive form fields in an existing PDF.
///
/// Scans the PDF for widget annotations with field type entries
/// and returns their names, types, current values, and available options.
///
/// # Returns
///
/// A vector of `DetectedFormField` structs, one per field found.
///
/// # Example
///
/// ```rust,no_run
/// use pdfrs::pdf_ops::detect_form_fields;
///
/// let fields = detect_form_fields("form.pdf").unwrap();
/// for f in &fields {
///     println!("{}: {:?}", f.name, f.value);
/// }
/// ```
pub fn detect_form_fields(input_file: &str) -> Result<Vec<DetectedFormField>> {
    let pdf_bytes = fs::read(input_file)?;
    let content = String::from_utf8_lossy(&pdf_bytes);

    let mut fields = Vec::new();

    // Find all PDF objects and check if they are widget annotations
    let obj_re = regex::Regex::new(r"(?s)(\d+)\s+0\s+obj(.*?)endobj").unwrap();
    let opt_re = regex::Regex::new(r"\(([^)]*)\)").unwrap();

    for caps in obj_re.captures_iter(&content) {
        let obj_text = &caps[0];
        let obj_body = &caps[2];

        // Must be an annotation widget
        if !obj_body.contains("/Type /Annot") || !obj_body.contains("/Subtype /Widget") {
            continue;
        }

        let dict_text = obj_text;

        // Extract /T (field name)
        let name = super::extract_pdf_dict_value(dict_text, "/T")
            .unwrap_or_default()
            .trim_matches(|c| c == '(' || c == ')')
            .to_string();

        if name.is_empty() {
            continue;
        }

        // Extract /FT (field type)
        let field_type = super::extract_pdf_dict_value(dict_text, "/FT")
            .unwrap_or_default()
            .trim_start_matches('/')
            .to_string();

        // Map PDF type to readable string
        let type_str = match field_type.as_str() {
            "Tx" => "text",
            "Btn" => {
                // Distinguish checkbox/radio by presence of /Opt or /V style
                if super::extract_pdf_dict_value(dict_text, "/Opt").is_some() {
                    "radio"
                } else {
                    "checkbox"
                }
            }
            "Ch" => "dropdown",
            _ => "unknown",
        };

        // Extract /V (value)
        let value = super::extract_pdf_dict_value(dict_text, "/V").map(|v| {
            if v.starts_with('(') && v.ends_with(')') {
                v[1..v.len() - 1].to_string()
            } else if v.starts_with('<') && v.ends_with('>') {
                crate::pdf::decode_pdf_hex_string(&v[1..v.len() - 1])
            } else {
                v.to_string()
            }
        });

        // Extract /Opt (options list)
        let options = if let Some(opt_raw) = super::extract_pdf_dict_value(dict_text, "/Opt") {
            // /Opt can be [(Option1) (Option2)] or an array reference
            opt_re
                .captures_iter(&opt_raw)
                .map(|c| c[1].to_string())
                .collect()
        } else {
            Vec::new()
        };

        // Extract /Ff flags — bit 30 (value 2) = required
        let required = super::extract_pdf_dict_value(dict_text, "/Ff")
            .and_then(|f| f.parse::<u32>().ok())
            .map(|flags| (flags & 2) != 0)
            .unwrap_or(false);

        fields.push(DetectedFormField {
            name,
            field_type: type_str.to_string(),
            value,
            options,
            required,
        });
    }

    Ok(fields)
}

/// Fill existing form fields in a PDF with new values and write the result.
///
/// Reads the input PDF, finds form fields by name, updates their /V values,
/// and writes an incremental update to the output file.
///
/// # Arguments
///
/// * `input_file` - Path to the input PDF with form fields
/// * `output_file` - Path where the filled PDF will be written
/// * `field_values` - HashMap mapping field names to new values
///
/// # Returns
///
/// Returns `Ok(())` if successful, or an error if filling fails.
///
/// # Example
///
/// ```rust,no_run
/// use std::collections::HashMap;
/// use pdfrs::pdf_ops::fill_form_fields;
///
/// let mut values = HashMap::new();
/// values.insert("firstName".to_string(), "Alice".to_string());
/// values.insert("age".to_string(), "30".to_string());
/// fill_form_fields("form.pdf", "filled.pdf", &values).unwrap();
/// ```
pub fn fill_form_fields(
    input_file: &str,
    output_file: &str,
    field_values: &std::collections::HashMap<String, String>,
) -> Result<()> {
    let pdf_bytes = fs::read(input_file)?;
    let content = String::from_utf8_lossy(&pdf_bytes);

    if field_values.is_empty() {
        fs::write(output_file, &pdf_bytes)?;
        return Ok(());
    }

    // Find all PDF objects and check if they are widget annotations
    let obj_re = regex::Regex::new(r"(?s)(\d+)\s+0\s+obj(.*?)endobj").unwrap();
    let v_re = regex::Regex::new(r"/V\s*\([^)]*\)").unwrap();

    let mut updated_bytes = pdf_bytes.clone();
    let mut offset_delta: isize = 0;

    for caps in obj_re.captures_iter(&content) {
        let dict_text = &caps[0];
        let obj_body = &caps[2];
        let full_match_start = caps.get(0).unwrap().start();

        // Must be a widget annotation
        if !obj_body.contains("/Type /Annot") || !obj_body.contains("/Subtype /Widget") {
            continue;
        }

        // Extract field name
        let name = super::extract_pdf_dict_value(dict_text, "/T")
            .unwrap_or_default()
            .trim_matches(|c| c == '(' || c == ')')
            .to_string();

        if name.is_empty() || !field_values.contains_key(&name) {
            continue;
        }

        let new_value = &field_values[&name];
        let escaped_value = super::escape_pdf_meta(new_value);

        let adjusted_start = ((full_match_start as isize) + offset_delta) as usize;
        let adjusted_end = adjusted_start + dict_text.len();

        if adjusted_end > updated_bytes.len() {
            continue;
        }

        let local_dict = String::from_utf8_lossy(&updated_bytes[adjusted_start..adjusted_end]);

        // Replace existing /V (...) or add /V before the closing >>
        let updated_dict = if local_dict.contains("/V ") {
            let new_v = format!("/V ({})", escaped_value);
            v_re.replace(&local_dict, &new_v).to_string()
        } else {
            // Insert /V before the final >>
            local_dict.replace(">>", &format!("/V ({})\n>>", escaped_value))
        };

        if updated_dict != *local_dict {
            let old_len = local_dict.len();
            let new_len = updated_dict.len();
            updated_bytes.splice(adjusted_start..adjusted_end, updated_dict.bytes());
            offset_delta += (new_len as isize) - (old_len as isize);
        }
    }

    fs::write(output_file, &updated_bytes)?;
    println!(
        "[fill] Updated {} field(s) in {}",
        field_values.len(),
        output_file
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_form_field_struct() {
        let field = FormField {
            name: "firstName".to_string(),
            field_type: FormFieldType::Text,
            x: 100.0,
            y: 700.0,
            width: 200.0,
            height: 20.0,
            default_value: Some("John".to_string()),
            options: vec![],
            required: true,
        };
        assert_eq!(field.name, "firstName");
        assert_eq!(field.field_type, FormFieldType::Text);
        assert!(field.required);
        assert_eq!(field.default_value, Some("John".to_string()));
    }

    #[test]
    fn test_field_type_to_pdf() {
        assert_eq!(field_type_to_pdf(&FormFieldType::Text), "/Tx");
        assert_eq!(field_type_to_pdf(&FormFieldType::Checkbox), "/Btn");
        assert_eq!(field_type_to_pdf(&FormFieldType::Radio), "/Btn");
        assert_eq!(field_type_to_pdf(&FormFieldType::Dropdown), "/Ch");
    }

    #[test]
    fn test_create_form_field_dict_text() {
        let field = FormField {
            name: "username".to_string(),
            field_type: FormFieldType::Text,
            x: 50.0,
            y: 600.0,
            width: 150.0,
            height: 18.0,
            default_value: Some("default".to_string()),
            options: vec![],
            required: false,
        };
        let dict = create_form_field_dict(&field);
        assert!(dict.contains("/Type /Annot"));
        assert!(dict.contains("/Subtype /Widget"));
        assert!(dict.contains("/T (username)"));
        assert!(dict.contains("/FT /Tx"));
        assert!(dict.contains("/V (default)"));
        assert!(dict.contains("/Rect [50 600 200 618]"));
    }

    #[test]
    fn test_create_form_field_dict_checkbox() {
        let field = FormField {
            name: "agree".to_string(),
            field_type: FormFieldType::Checkbox,
            x: 50.0,
            y: 550.0,
            width: 15.0,
            height: 15.0,
            default_value: None,
            options: vec![],
            required: true,
        };
        let dict = create_form_field_dict(&field);
        assert!(dict.contains("/FT /Btn"));
        assert!(dict.contains("/T (agree)"));
        assert!(dict.contains("/Ff 2")); // Required flag
        assert!(dict.contains("/V /Off"));
    }

    #[test]
    fn test_create_form_field_dict_dropdown() {
        let field = FormField {
            name: "country".to_string(),
            field_type: FormFieldType::Dropdown,
            x: 50.0,
            y: 500.0,
            width: 100.0,
            height: 20.0,
            default_value: Some("USA".to_string()),
            options: vec![
                "USA".to_string(),
                "Canada".to_string(),
                "Mexico".to_string(),
            ],
            required: false,
        };
        let dict = create_form_field_dict(&field);
        assert!(dict.contains("/FT /Ch"));
        assert!(dict.contains("/T (country)"));
        assert!(dict.contains("/V (USA)"));
        assert!(dict.contains("(USA)"));
        assert!(dict.contains("(Canada)"));
        assert!(dict.contains("(Mexico)"));
        assert!(dict.contains("/Ff 131072")); // Combo flag
    }
}
