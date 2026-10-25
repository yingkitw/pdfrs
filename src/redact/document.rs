//! Document-level helpers: object references, XObject resources and
//! annotation appearance streams.

use std::collections::HashMap;

use crate::pdf::{PdfDocument, PdfObject, PdfValue};

/// Whether any object in the document still references `id`.
pub(super) fn is_referenced(doc: &PdfDocument, id: u32) -> bool {
    fn value_references(v: &PdfValue, id: u32) -> bool {
        match v {
            PdfValue::Reference(r, _) => *r == id,
            PdfValue::Object(o) => object_references(o, id),
        }
    }
    fn object_references(o: &PdfObject, id: u32) -> bool {
        match o {
            PdfObject::Dictionary(d) => d.values().any(|v| value_references(v, id)),
            PdfObject::Stream { dictionary, .. } => {
                dictionary.values().any(|v| value_references(v, id))
            }
            PdfObject::Array(items) => items.iter().any(|v| value_references(v, id)),
            PdfObject::Reference(r, _) => *r == id,
            _ => false,
        }
    }
    doc.objects.values().any(|o| object_references(o, id))
}

/// Remove `names` from the page's `/Resources /XObject` dictionary (whether
/// the resources dict is inline or a separate object).
pub(super) fn remove_xobject_entries(doc: &mut PdfDocument, page_id: u32, names: &[String]) {
    // Find the object that owns the XObject dict: the page itself or the
    // object its /Resources points at.
    let owner_id = {
        let Some(dict) = crate::search::object_dict(doc, page_id) else {
            return;
        };
        match dict.get("Resources") {
            Some(PdfValue::Reference(id, _)) => *id,
            Some(PdfValue::Object(PdfObject::Dictionary(_))) => page_id,
            _ => return,
        }
    };
    let Some(owner) = doc.objects.get_mut(&owner_id) else {
        return;
    };
    let dict = match owner {
        PdfObject::Dictionary(d) | PdfObject::Stream { dictionary: d, .. } => d,
        _ => return,
    };
    if let Some(PdfValue::Object(PdfObject::Dictionary(res))) = dict.get_mut("Resources")
        && let Some(PdfValue::Object(PdfObject::Dictionary(xobj))) = res.get_mut("XObject")
    {
        xobj.retain(|k, _| !names.contains(k));
    }
}

/// Stream object ids of annotation appearance streams (`/AP /N /R /D`)
/// attached to a page's `/Annots`.
pub(super) fn collect_annotation_ap_streams(doc: &PdfDocument, page_id: u32) -> Vec<u32> {
    let mut result = Vec::new();
    let Some(dict) = crate::search::object_dict(doc, page_id) else {
        return result;
    };
    let annots = match dict.get("Annots") {
        Some(PdfValue::Object(PdfObject::Array(items))) => items.clone(),
        Some(PdfValue::Reference(id, _)) => match doc.objects.get(id) {
            Some(PdfObject::Array(items)) => items.clone(),
            _ => return result,
        },
        _ => return result,
    };
    for annot_val in annots {
        let annot_id = match annot_val {
            PdfValue::Reference(id, _) => id,
            PdfValue::Object(PdfObject::Dictionary(_)) => continue,
            _ => continue,
        };
        let Some(annot_dict) = crate::search::object_dict(doc, annot_id) else {
            continue;
        };
        if let Some(PdfValue::Object(PdfObject::Dictionary(ap))) = annot_dict.get("AP") {
            for entry in ap.values() {
                match entry {
                    PdfValue::Reference(id, _) => result.push(*id),
                    PdfValue::Object(PdfObject::Dictionary(states)) => {
                        for state in states.values() {
                            if let PdfValue::Reference(id, _) = state {
                                result.push(*id);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    result
}

// ----- Stream rewriting ---------------------------------------------------

/// An XObject entry from a page's `/Resources`.
#[derive(Debug, Clone)]
pub(super) struct XObjectInfo {
    /// Object number of the XObject.
    pub(super) id: u32,
    pub(super) is_image: bool,
    pub(super) is_form: bool,
}

pub(super) type XObjectMap = HashMap<String, XObjectInfo>;

/// Collect XObject entries (images and forms) from a page's /Resources.
pub(super) fn collect_xobjects(doc: &PdfDocument, page_id: u32) -> XObjectMap {
    let mut result = HashMap::new();
    let Some(dict) = crate::search::object_dict(doc, page_id) else {
        return result;
    };
    let resources_dict = match dict.get("Resources") {
        Some(PdfValue::Reference(id, _)) => crate::search::object_dict(doc, *id),
        Some(PdfValue::Object(PdfObject::Dictionary(d))) => Some(d),
        _ => None,
    };
    let Some(res_dict) = resources_dict else {
        return result;
    };
    let xobj_dict = match res_dict.get("XObject") {
        Some(PdfValue::Reference(id, _)) => crate::search::object_dict(doc, *id),
        Some(PdfValue::Object(PdfObject::Dictionary(d))) => Some(d),
        _ => None,
    };
    let Some(xobj_dict) = xobj_dict else {
        return result;
    };
    for (name, val) in xobj_dict {
        let PdfValue::Reference(id, _) = val else {
            continue;
        };
        let Some(obj_dict) = crate::search::object_dict(doc, *id) else {
            continue;
        };
        let subtype = obj_dict
            .get("Subtype")
            .and_then(|v| match v {
                PdfValue::Object(PdfObject::Name(s)) => Some(s.as_str()),
                _ => None,
            })
            .unwrap_or("");
        result.insert(
            name.clone(),
            XObjectInfo {
                id: *id,
                is_image: subtype == "Image",
                is_form: subtype == "Form",
            },
        );
    }
    result
}
