//! Closed registry of valid `(kind, name)` language items.

use super::LangItemKind;

/// Returns `true` when `name` is a known language item for `kind`.
#[must_use]
pub fn is_known_lang_item(kind: LangItemKind, name: &str) -> bool {
    match kind {
        LangItemKind::Intrinsic => matches!(
            name,
            "alloc_bytes" | "dealloc_bytes" | "slice_from_raw_parts" | "len" | "size_of"
        ),
        LangItemKind::Enum => matches!(name, "Option" | "Result"),
        LangItemKind::Variant => matches!(name, "Some" | "None" | "Ok" | "Err"),
        LangItemKind::Trait => matches!(
            name,
            "Copyable"
                | "Clone"
                | "Drop"
                | "PartialEq"
                | "Eq"
                | "Debug"
                | "Iterator"
                | "IntoIter"
                | "From"
        ),
    }
}

/// Variant names collected automatically for a marked std enum.
#[must_use]
pub fn auto_variants_for_enum(enum_name: &str) -> &'static [&'static str] {
    match enum_name {
        "Option" => &["Some", "None"],
        "Result" => &["Ok", "Err"],
        _ => &[],
    }
}
