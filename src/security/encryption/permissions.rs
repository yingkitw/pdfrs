//! PDF permission flags for document access control.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdfPermissions {
    /// Allow printing the document
    pub print: bool,
    /// Allow copying text and graphics
    pub copy: bool,
    /// Allow modifying the document
    pub modify: bool,
    /// Allow adding or modifying annotations
    pub annotate: bool,
    /// Allow filling in form fields
    pub fill_forms: bool,
    /// Allow extracting content for accessibility
    pub extract: bool,
    /// Allow assembling the document (insert, rotate, delete pages)
    pub assemble: bool,
    /// Allow printing high-quality versions
    pub print_high_quality: bool,
}

impl Default for PdfPermissions {
    fn default() -> Self {
        Self {
            print: true,
            copy: true,
            modify: true,
            annotate: true,
            fill_forms: true,
            extract: true,
            assemble: true,
            print_high_quality: true,
        }
    }
}

impl PdfPermissions {
    /// Create permissions with all permissions granted
    pub fn all() -> Self {
        Self::default()
    }

    /// Create permissions with all permissions denied (except basic viewing)
    pub fn none() -> Self {
        Self {
            print: false,
            copy: false,
            modify: false,
            annotate: false,
            fill_forms: false,
            extract: false,
            assemble: false,
            print_high_quality: false,
        }
    }

    /// Create permissions for read-only documents (viewing only)
    pub fn read_only() -> Self {
        Self {
            print: false,
            copy: false,
            modify: false,
            annotate: false,
            fill_forms: false,
            extract: true,
            assemble: false,
            print_high_quality: false,
        }
    }

    /// Convert to PDF permission flags (as specified in PDF 1.7 spec)
    /// Returns a u32 representing the permission bits
    pub fn to_pdf_flags(&self) -> u32 {
        // Default value with reserved bits set (bits 0-2, 6-7, 10, 13-31 are reserved)
        let mut flags = 0xFFFFF0C0u32;

        // Clear permission bits first (bits 2-5, 8-9, 11-12)
        flags &= !(1 << 2); // Clear print bit
        flags &= !(1 << 3); // Clear modify bit
        flags &= !(1 << 4); // Clear copy bit
        flags &= !(1 << 5); // Clear annotate bit
        flags &= !(1 << 8); // Clear fill_forms bit
        flags &= !(1 << 9); // Clear extract bit
        flags &= !(1 << 11); // Clear assemble bit
        flags &= !(1 << 12); // Clear print_high_quality bit

        // Set permission bits based on settings
        if self.print {
            flags |= 1 << 2;
        }
        if self.modify {
            flags |= 1 << 3;
        }
        if self.copy {
            flags |= 1 << 4;
        }
        if self.annotate {
            flags |= 1 << 5;
        }
        if self.fill_forms {
            flags |= 1 << 8;
        }
        if self.extract {
            flags |= 1 << 9;
        }
        if self.assemble {
            flags |= 1 << 11;
        }
        if self.print_high_quality {
            flags |= 1 << 12;
        }

        flags
    }

    /// Parse from PDF permission flags
    pub fn from_pdf_flags(flags: u32) -> Self {
        Self {
            print: (flags & (1 << 2)) != 0,
            modify: (flags & (1 << 3)) != 0,
            copy: (flags & (1 << 4)) != 0,
            annotate: (flags & (1 << 5)) != 0,
            fill_forms: (flags & (1 << 8)) != 0,
            extract: (flags & (1 << 9)) != 0,
            assemble: (flags & (1 << 11)) != 0,
            print_high_quality: (flags & (1 << 12)) != 0,
        }
    }
}
