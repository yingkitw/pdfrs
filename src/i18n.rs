//! Multi-language messages and locale-aware formatting.
//!
//! Provides a small message catalog for validation/CLI errors and helpers for
//! locale-specific integer/decimal formatting. Locale can be selected via the
//! CLI `--lang` flag or the `PDFRS_LANG` environment variable.

use crate::pdf::PdfValidation;

/// Supported UI / error-message locales.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Locale {
    #[default]
    En,
    Es,
    De,
    Fr,
    Zh,
    He,
    Ar,
}

impl Locale {
    /// Parse a BCP-47-ish tag (`en`, `es`, `de`, `fr`, `zh`, `he`, `ar`, or longer forms like `en-US`).
    pub fn parse(s: &str) -> Option<Self> {
        let primary = s
            .split(['-', '_'])
            .next()
            .unwrap_or(s)
            .to_ascii_lowercase();
        match primary.as_str() {
            "en" => Some(Self::En),
            "es" => Some(Self::Es),
            "de" => Some(Self::De),
            "fr" => Some(Self::Fr),
            "zh" => Some(Self::Zh),
            "he" | "iw" => Some(Self::He),
            "ar" => Some(Self::Ar),
            _ => None,
        }
    }

    /// Resolve from `PDFRS_LANG`, then `LANG`, then English.
    pub fn from_env() -> Self {
        if let Ok(v) = std::env::var("PDFRS_LANG")
            && let Some(loc) = Self::parse(&v)
        {
            return loc;
        }
        if let Ok(v) = std::env::var("LANG")
            && let Some(loc) = Self::parse(&v)
        {
            return loc;
        }
        Self::En
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Es => "es",
            Self::De => "de",
            Self::Fr => "fr",
            Self::Zh => "zh",
            Self::He => "he",
            Self::Ar => "ar",
        }
    }

    /// Thousands separator for integers (`None` = no grouping).
    fn thousands_sep(self) -> Option<char> {
        match self {
            Self::En | Self::Zh | Self::He | Self::Ar => Some(','),
            Self::Es | Self::De | Self::Fr => Some('.'),
        }
    }

    fn decimal_sep(self) -> char {
        match self {
            Self::En | Self::Zh | Self::He | Self::Ar => '.',
            Self::Es | Self::De | Self::Fr => ',',
        }
    }
}

/// Stable message identifiers for catalog lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MsgId {
    MissingPdfHeader,
    MissingEof,
    MissingStartxref,
    MissingTrailer,
    MissingCatalog,
    MissingPagesTree,
    NoPageObjects,
    NoPdfObjects,
    TrailerMissingRoot,
    FileNotFound,
    CannotReadFile,
    ValidationResultFor,
    ValidLabel,
    PagesLabel,
    ObjectsLabel,
    ErrorsLabel,
    WarningsLabel,
    ErrorValidatingPdf,
    Yes,
    No,
}

/// Translate a message id for the given locale.
pub fn t(locale: Locale, id: MsgId) -> &'static str {
    match (locale, id) {
        // English
        (Locale::En, MsgId::MissingPdfHeader) => "Missing PDF header (%PDF-x.x)",
        (Locale::En, MsgId::MissingEof) => "Missing %%EOF marker at end of file",
        (Locale::En, MsgId::MissingStartxref) => "Missing startxref pointer",
        (Locale::En, MsgId::MissingTrailer) => "Missing trailer dictionary",
        (Locale::En, MsgId::MissingCatalog) => "Missing document catalog (/Type /Catalog)",
        (Locale::En, MsgId::MissingPagesTree) => "Missing pages tree (/Type /Pages)",
        (Locale::En, MsgId::NoPageObjects) => "No page objects found (/Type /Page)",
        (Locale::En, MsgId::NoPdfObjects) => "No PDF objects found",
        (Locale::En, MsgId::TrailerMissingRoot) => "Trailer missing /Root reference",
        (Locale::En, MsgId::FileNotFound) => "File not found: {0}",
        (Locale::En, MsgId::CannotReadFile) => "Cannot read file: {0}",
        (Locale::En, MsgId::ValidationResultFor) => "Validation result for {0}:",
        (Locale::En, MsgId::ValidLabel) => "Valid",
        (Locale::En, MsgId::PagesLabel) => "Pages",
        (Locale::En, MsgId::ObjectsLabel) => "Objects",
        (Locale::En, MsgId::ErrorsLabel) => "Errors",
        (Locale::En, MsgId::WarningsLabel) => "Warnings",
        (Locale::En, MsgId::ErrorValidatingPdf) => "Error validating PDF: {0}",
        (Locale::En, MsgId::Yes) => "yes",
        (Locale::En, MsgId::No) => "no",

        // Spanish
        (Locale::Es, MsgId::MissingPdfHeader) => "Falta la cabecera PDF (%PDF-x.x)",
        (Locale::Es, MsgId::MissingEof) => "Falta el marcador %%EOF al final del archivo",
        (Locale::Es, MsgId::MissingStartxref) => "Falta el puntero startxref",
        (Locale::Es, MsgId::MissingTrailer) => "Falta el diccionario trailer",
        (Locale::Es, MsgId::MissingCatalog) => "Falta el catálogo del documento (/Type /Catalog)",
        (Locale::Es, MsgId::MissingPagesTree) => "Falta el árbol de páginas (/Type /Pages)",
        (Locale::Es, MsgId::NoPageObjects) => "No se encontraron objetos de página (/Type /Page)",
        (Locale::Es, MsgId::NoPdfObjects) => "No se encontraron objetos PDF",
        (Locale::Es, MsgId::TrailerMissingRoot) => "El trailer no tiene referencia /Root",
        (Locale::Es, MsgId::FileNotFound) => "Archivo no encontrado: {0}",
        (Locale::Es, MsgId::CannotReadFile) => "No se puede leer el archivo: {0}",
        (Locale::Es, MsgId::ValidationResultFor) => "Resultado de validación para {0}:",
        (Locale::Es, MsgId::ValidLabel) => "Válido",
        (Locale::Es, MsgId::PagesLabel) => "Páginas",
        (Locale::Es, MsgId::ObjectsLabel) => "Objetos",
        (Locale::Es, MsgId::ErrorsLabel) => "Errores",
        (Locale::Es, MsgId::WarningsLabel) => "Advertencias",
        (Locale::Es, MsgId::ErrorValidatingPdf) => "Error al validar el PDF: {0}",
        (Locale::Es, MsgId::Yes) => "sí",
        (Locale::Es, MsgId::No) => "no",

        // German
        (Locale::De, MsgId::MissingPdfHeader) => "PDF-Kopfzeile fehlt (%PDF-x.x)",
        (Locale::De, MsgId::MissingEof) => "%%EOF-Markierung am Dateiende fehlt",
        (Locale::De, MsgId::MissingStartxref) => "startxref-Zeiger fehlt",
        (Locale::De, MsgId::MissingTrailer) => "Trailer-Wörterbuch fehlt",
        (Locale::De, MsgId::MissingCatalog) => "Dokumentkatalog fehlt (/Type /Catalog)",
        (Locale::De, MsgId::MissingPagesTree) => "Seitenbaum fehlt (/Type /Pages)",
        (Locale::De, MsgId::NoPageObjects) => "Keine Seitenobjekte gefunden (/Type /Page)",
        (Locale::De, MsgId::NoPdfObjects) => "Keine PDF-Objekte gefunden",
        (Locale::De, MsgId::TrailerMissingRoot) => "Trailer ohne /Root-Referenz",
        (Locale::De, MsgId::FileNotFound) => "Datei nicht gefunden: {0}",
        (Locale::De, MsgId::CannotReadFile) => "Datei kann nicht gelesen werden: {0}",
        (Locale::De, MsgId::ValidationResultFor) => "Validierungsergebnis für {0}:",
        (Locale::De, MsgId::ValidLabel) => "Gültig",
        (Locale::De, MsgId::PagesLabel) => "Seiten",
        (Locale::De, MsgId::ObjectsLabel) => "Objekte",
        (Locale::De, MsgId::ErrorsLabel) => "Fehler",
        (Locale::De, MsgId::WarningsLabel) => "Warnungen",
        (Locale::De, MsgId::ErrorValidatingPdf) => "Fehler bei der PDF-Validierung: {0}",
        (Locale::De, MsgId::Yes) => "ja",
        (Locale::De, MsgId::No) => "nein",

        // French
        (Locale::Fr, MsgId::MissingPdfHeader) => "En-tête PDF manquant (%PDF-x.x)",
        (Locale::Fr, MsgId::MissingEof) => "Marqueur %%EOF manquant en fin de fichier",
        (Locale::Fr, MsgId::MissingStartxref) => "Pointeur startxref manquant",
        (Locale::Fr, MsgId::MissingTrailer) => "Dictionnaire trailer manquant",
        (Locale::Fr, MsgId::MissingCatalog) => "Catalogue du document manquant (/Type /Catalog)",
        (Locale::Fr, MsgId::MissingPagesTree) => "Arborescence des pages manquante (/Type /Pages)",
        (Locale::Fr, MsgId::NoPageObjects) => "Aucun objet page trouvé (/Type /Page)",
        (Locale::Fr, MsgId::NoPdfObjects) => "Aucun objet PDF trouvé",
        (Locale::Fr, MsgId::TrailerMissingRoot) => "Trailer sans référence /Root",
        (Locale::Fr, MsgId::FileNotFound) => "Fichier introuvable : {0}",
        (Locale::Fr, MsgId::CannotReadFile) => "Impossible de lire le fichier : {0}",
        (Locale::Fr, MsgId::ValidationResultFor) => "Résultat de validation pour {0} :",
        (Locale::Fr, MsgId::ValidLabel) => "Valide",
        (Locale::Fr, MsgId::PagesLabel) => "Pages",
        (Locale::Fr, MsgId::ObjectsLabel) => "Objets",
        (Locale::Fr, MsgId::ErrorsLabel) => "Erreurs",
        (Locale::Fr, MsgId::WarningsLabel) => "Avertissements",
        (Locale::Fr, MsgId::ErrorValidatingPdf) => "Erreur lors de la validation du PDF : {0}",
        (Locale::Fr, MsgId::Yes) => "oui",
        (Locale::Fr, MsgId::No) => "non",

        // Chinese (Simplified)
        (Locale::Zh, MsgId::MissingPdfHeader) => "缺少 PDF 文件头 (%PDF-x.x)",
        (Locale::Zh, MsgId::MissingEof) => "文件末尾缺少 %%EOF 标记",
        (Locale::Zh, MsgId::MissingStartxref) => "缺少 startxref 指针",
        (Locale::Zh, MsgId::MissingTrailer) => "缺少 trailer 字典",
        (Locale::Zh, MsgId::MissingCatalog) => "缺少文档目录 (/Type /Catalog)",
        (Locale::Zh, MsgId::MissingPagesTree) => "缺少页面树 (/Type /Pages)",
        (Locale::Zh, MsgId::NoPageObjects) => "未找到页面对象 (/Type /Page)",
        (Locale::Zh, MsgId::NoPdfObjects) => "未找到 PDF 对象",
        (Locale::Zh, MsgId::TrailerMissingRoot) => "trailer 缺少 /Root 引用",
        (Locale::Zh, MsgId::FileNotFound) => "找不到文件：{0}",
        (Locale::Zh, MsgId::CannotReadFile) => "无法读取文件：{0}",
        (Locale::Zh, MsgId::ValidationResultFor) => "{0} 的验证结果：",
        (Locale::Zh, MsgId::ValidLabel) => "有效",
        (Locale::Zh, MsgId::PagesLabel) => "页数",
        (Locale::Zh, MsgId::ObjectsLabel) => "对象数",
        (Locale::Zh, MsgId::ErrorsLabel) => "错误",
        (Locale::Zh, MsgId::WarningsLabel) => "警告",
        (Locale::Zh, MsgId::ErrorValidatingPdf) => "验证 PDF 时出错：{0}",
        (Locale::Zh, MsgId::Yes) => "是",
        (Locale::Zh, MsgId::No) => "否",

        // Hebrew
        (Locale::He, MsgId::MissingPdfHeader) => "חסרה כותרת PDF (%PDF-x.x)",
        (Locale::He, MsgId::MissingEof) => "חסר סימן %%EOF בסוף הקובץ",
        (Locale::He, MsgId::MissingStartxref) => "חסר מצביע startxref",
        (Locale::He, MsgId::MissingTrailer) => "חסר מילון trailer",
        (Locale::He, MsgId::MissingCatalog) => "חסר קטלוג מסמך (/Type /Catalog)",
        (Locale::He, MsgId::MissingPagesTree) => "חסר עץ עמודים (/Type /Pages)",
        (Locale::He, MsgId::NoPageObjects) => "לא נמצאו אובייקטי עמוד (/Type /Page)",
        (Locale::He, MsgId::NoPdfObjects) => "לא נמצאו אובייקטי PDF",
        (Locale::He, MsgId::TrailerMissingRoot) => "ב-trailer חסרה הפניה /Root",
        (Locale::He, MsgId::FileNotFound) => "הקובץ לא נמצא: {0}",
        (Locale::He, MsgId::CannotReadFile) => "לא ניתן לקרוא את הקובץ: {0}",
        (Locale::He, MsgId::ValidationResultFor) => "תוצאת אימות עבור {0}:",
        (Locale::He, MsgId::ValidLabel) => "תקין",
        (Locale::He, MsgId::PagesLabel) => "עמודים",
        (Locale::He, MsgId::ObjectsLabel) => "אובייקטים",
        (Locale::He, MsgId::ErrorsLabel) => "שגיאות",
        (Locale::He, MsgId::WarningsLabel) => "אזהרות",
        (Locale::He, MsgId::ErrorValidatingPdf) => "שגיאה באימות PDF: {0}",
        (Locale::He, MsgId::Yes) => "כן",
        (Locale::He, MsgId::No) => "לא",

        // Arabic
        (Locale::Ar, MsgId::MissingPdfHeader) => "رأس PDF مفقود (%PDF-x.x)",
        (Locale::Ar, MsgId::MissingEof) => "علامة %%EOF مفقودة في نهاية الملف",
        (Locale::Ar, MsgId::MissingStartxref) => "مؤشر startxref مفقود",
        (Locale::Ar, MsgId::MissingTrailer) => "قاموس trailer مفقود",
        (Locale::Ar, MsgId::MissingCatalog) => "كتالوج المستند مفقود (/Type /Catalog)",
        (Locale::Ar, MsgId::MissingPagesTree) => "شجرة الصفحات مفقودة (/Type /Pages)",
        (Locale::Ar, MsgId::NoPageObjects) => "لم يتم العثور على كائنات صفحة (/Type /Page)",
        (Locale::Ar, MsgId::NoPdfObjects) => "لم يتم العثور على كائنات PDF",
        (Locale::Ar, MsgId::TrailerMissingRoot) => "الـ trailer يفتقد مرجع /Root",
        (Locale::Ar, MsgId::FileNotFound) => "الملف غير موجود: {0}",
        (Locale::Ar, MsgId::CannotReadFile) => "تعذر قراءة الملف: {0}",
        (Locale::Ar, MsgId::ValidationResultFor) => "نتيجة التحقق لـ {0}:",
        (Locale::Ar, MsgId::ValidLabel) => "صالح",
        (Locale::Ar, MsgId::PagesLabel) => "الصفحات",
        (Locale::Ar, MsgId::ObjectsLabel) => "الكائنات",
        (Locale::Ar, MsgId::ErrorsLabel) => "أخطاء",
        (Locale::Ar, MsgId::WarningsLabel) => "تحذيرات",
        (Locale::Ar, MsgId::ErrorValidatingPdf) => "خطأ أثناء التحقق من PDF: {0}",
        (Locale::Ar, MsgId::Yes) => "نعم",
        (Locale::Ar, MsgId::No) => "لا",
    }
}

/// Format a catalog message, replacing `{0}`, `{1}`, … with `args`.
pub fn tf(locale: Locale, id: MsgId, args: &[&str]) -> String {
    let mut out = t(locale, id).to_string();
    for (i, arg) in args.iter().enumerate() {
        let placeholder = format!("{{{}}}", i);
        out = out.replace(&placeholder, arg);
    }
    out
}

/// Map a known English validation string to a [`MsgId`].
pub fn msg_id_for_english(english: &str) -> Option<MsgId> {
    match english {
        "Missing PDF header (%PDF-x.x)" => Some(MsgId::MissingPdfHeader),
        "Missing %%EOF marker at end of file" => Some(MsgId::MissingEof),
        "Missing startxref pointer" => Some(MsgId::MissingStartxref),
        "Missing trailer dictionary" => Some(MsgId::MissingTrailer),
        "Missing document catalog (/Type /Catalog)" => Some(MsgId::MissingCatalog),
        "Missing pages tree (/Type /Pages)" => Some(MsgId::MissingPagesTree),
        "No page objects found (/Type /Page)" => Some(MsgId::NoPageObjects),
        "No PDF objects found" => Some(MsgId::NoPdfObjects),
        "Trailer missing /Root reference" => Some(MsgId::TrailerMissingRoot),
        _ => None,
    }
}

/// Translate a known English message; unknown strings pass through unchanged.
pub fn localize_message(locale: Locale, english: &str) -> String {
    if locale == Locale::En {
        return english.to_string();
    }
    if let Some(id) = msg_id_for_english(english) {
        return t(locale, id).to_string();
    }
    english.to_string()
}

/// Return a copy of [`PdfValidation`] with localized error/warning strings.
pub fn localize_validation(locale: Locale, validation: &PdfValidation) -> PdfValidation {
    PdfValidation {
        valid: validation.valid,
        errors: validation
            .errors
            .iter()
            .map(|e| localize_message(locale, e))
            .collect(),
        warnings: validation
            .warnings
            .iter()
            .map(|w| localize_message(locale, w))
            .collect(),
        page_count: validation.page_count,
        object_count: validation.object_count,
    }
}

/// Format an integer with locale-appropriate thousands separators.
pub fn format_integer(locale: Locale, n: u64) -> String {
    let digits = n.to_string();
    let Some(sep) = locale.thousands_sep() else {
        return digits;
    };
    let mut out = String::new();
    for (i, ch) in digits.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(sep);
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

/// Format a floating-point value with locale decimal separator.
pub fn format_decimal(locale: Locale, n: f64, places: usize) -> String {
    let s = format!("{:.*}", places, n);
    if locale.decimal_sep() == '.' {
        return s;
    }
    s.replace('.', &locale.decimal_sep().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_locale_parse() {
        assert_eq!(Locale::parse("en"), Some(Locale::En));
        assert_eq!(Locale::parse("es-ES"), Some(Locale::Es));
        assert_eq!(Locale::parse("de_DE"), Some(Locale::De));
        assert_eq!(Locale::parse("zh-CN"), Some(Locale::Zh));
        assert_eq!(Locale::parse("he"), Some(Locale::He));
        assert_eq!(Locale::parse("xx"), None);
    }

    #[test]
    fn test_spanish_validation_message() {
        let msg = t(Locale::Es, MsgId::MissingPdfHeader);
        assert!(msg.contains("cabecera") || msg.contains("Falta"));
        assert_ne!(msg, t(Locale::En, MsgId::MissingPdfHeader));
    }

    #[test]
    fn test_tf_placeholder() {
        let msg = tf(Locale::En, MsgId::FileNotFound, &["a.pdf"]);
        assert_eq!(msg, "File not found: a.pdf");
        let es = tf(Locale::Es, MsgId::FileNotFound, &["a.pdf"]);
        assert!(es.contains("a.pdf"));
        assert!(es.contains("no encontrado") || es.contains("Archivo"));
    }

    #[test]
    fn test_localize_validation() {
        let v = PdfValidation {
            valid: false,
            errors: vec!["Missing PDF header (%PDF-x.x)".into()],
            warnings: vec![],
            page_count: 0,
            object_count: 0,
        };
        let de = localize_validation(Locale::De, &v);
        assert!(de.errors[0].contains("Kopfzeile") || de.errors[0].contains("fehlt"));
    }

    #[test]
    fn test_format_integer_grouping() {
        assert_eq!(format_integer(Locale::En, 1234567), "1,234,567");
        assert_eq!(format_integer(Locale::De, 1234567), "1.234.567");
        assert_eq!(format_integer(Locale::En, 42), "42");
    }

    #[test]
    fn test_format_decimal() {
        assert_eq!(format_decimal(Locale::En, 12.5, 1), "12.5");
        assert_eq!(format_decimal(Locale::Fr, 12.5, 1), "12,5");
    }

    #[test]
    fn test_all_locales_have_catalog_entries() {
        for locale in [
            Locale::En,
            Locale::Es,
            Locale::De,
            Locale::Fr,
            Locale::Zh,
            Locale::He,
            Locale::Ar,
        ] {
            let s = t(locale, MsgId::ValidLabel);
            assert!(!s.is_empty(), "empty ValidLabel for {:?}", locale);
        }
    }
}
