//! Digital signature metadata (`DigitalSignature`) and its PDF dictionary form.

use crate::pdf_generator::escape_pdf_string;

/// Digital signature information for PDF documents
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigitalSignature {
    /// Name of the signer
    pub signer_name: String,
    /// Reason for signing (e.g., "I approve this document")
    pub reason: Option<String>,
    /// Location where the document was signed
    pub location: Option<String>,
    /// Contact information for the signer
    pub contact_info: Option<String>,
    /// Signing date (ISO 8601 format)
    pub date: Option<String>,
    /// Signature filter (e.g., "Adobe.PPKLite")
    pub filter: String,
    /// Sub-filter defining the signature format (e.g., "adbe.pkcs7.detached")
    pub sub_filter: String,
    /// The PKCS#7/CMS signature bytes (hex-encoded for PDF storage)
    pub signature_hex: Option<String>,
    /// Byte range [start1, len1, start2, len2] that was signed
    pub byte_range: Option<Vec<u32>>,
}

impl Default for DigitalSignature {
    fn default() -> Self {
        Self {
            signer_name: String::new(),
            reason: None,
            location: None,
            contact_info: None,
            date: None,
            filter: "Adobe.PPKLite".to_string(),
            sub_filter: "adbe.pkcs7.detached".to_string(),
            signature_hex: None,
            byte_range: None,
        }
    }
}

impl DigitalSignature {
    /// Create a new digital signature with the given signer name
    pub fn new(signer_name: impl Into<String>) -> Self {
        Self {
            signer_name: signer_name.into(),
            ..Default::default()
        }
    }

    /// Set the reason for signing
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    /// Set the signing location
    pub fn with_location(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }

    /// Set the contact information
    pub fn with_contact_info(mut self, contact: impl Into<String>) -> Self {
        self.contact_info = Some(contact.into());
        self
    }

    /// Set the signing date
    pub fn with_date(mut self, date: impl Into<String>) -> Self {
        self.date = Some(date.into());
        self
    }

    /// Build the PDF signature dictionary string
    pub fn to_pdf_dict(&self) -> String {
        let mut dict = format!(
            "<< /Type /Sig\n\
             /Filter /{}\n\
             /SubFilter /{}\n\
             /M (D:{})\n\
             /Name ({})\n",
            escape_pdf_name(&self.filter),
            escape_pdf_name(&self.sub_filter),
            self.date.as_deref().unwrap_or(""),
            escape_pdf_string(&self.signer_name)
        );

        if let Some(ref reason) = self.reason {
            dict.push_str(&format!(" /Reason ({})\n", escape_pdf_string(reason)));
        }
        if let Some(ref location) = self.location {
            dict.push_str(&format!(" /Location ({})\n", escape_pdf_string(location)));
        }
        if let Some(ref contact) = self.contact_info {
            dict.push_str(&format!(" /ContactInfo ({})\n", escape_pdf_string(contact)));
        }

        // ByteRange: [start1, len1, start2, len2]
        if let Some(ref range) = self.byte_range {
            let range_str: Vec<String> = range.iter().map(|v| v.to_string()).collect();
            dict.push_str(&format!(" /ByteRange [{}]\n", range_str.join(" ")));
        }

        // Contents: hex-encoded signature placeholder or actual signature
        if let Some(ref sig_hex) = self.signature_hex {
            dict.push_str(&format!(" /Contents <{}>\n", sig_hex));
        } else {
            // Placeholder for a 4096-byte signature
            dict.push_str(" /Contents <");
            dict.push_str(&"0".repeat(8192));
            dict.push_str(">\n");
        }

        dict.push_str(">>");
        dict
    }
}

fn escape_pdf_name(name: &str) -> String {
    name.replace("#", "#23")
        .replace(" ", "#20")
        .replace("/", "#2F")
        .replace("[", "#5B")
        .replace("]", "#5D")
        .replace("<", "#3C")
        .replace(">", "#3E")
        .replace("(", "#28")
        .replace(")", "#29")
}
