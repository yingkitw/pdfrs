//! Shared regular expressions for the PDF security submodules.

use std::sync::OnceLock;

macro_rules! security_regex {
    ($name:ident, $pat:literal) => {
        pub(super) fn $name() -> &'static regex::Regex {
            static RE: OnceLock<regex::Regex> = OnceLock::new();
            RE.get_or_init(|| regex::Regex::new($pat).unwrap())
        }
    };
}

security_regex!(re_sig_obj, r"(?s)(\d+)\s+0\s+obj\s+<<(.+?)>>\s+endobj");
security_regex!(re_root_ref, r"/Root\s+(\d+\s+\d+\s+R)");
security_regex!(re_info_ref, r"/Info\s+(\d+\s+\d+\s+R)");
security_regex!(
    re_catalog_obj,
    r"(?s)(\d+)\s+(\d+)\s+obj\s*(<<.*?/Type\s*/Catalog.*?>>)"
);
security_regex!(re_acroform, r"/AcroForm\s+<<[^>]*>>");
