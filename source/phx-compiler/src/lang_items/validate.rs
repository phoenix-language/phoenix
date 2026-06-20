//! Closed registry of valid `(kind, name)` language items.
//!
//! v1 accepts only the names listed here. [`super::collect::build_lang_item_registry`]
//! rejects unknown pairs with [`TypeCheckError::LangItemInvalid`](crate::typeck::TypeCheckError::LangItemInvalid)
//! and auto-links enum variants when explicit `#[lang_item]` markers are omitted.

use super::LangItemKind;

/// Returns `true` when `name` is a known language item for `kind`.
///
/// The closed set covers std VM intrinsics, `Option`/`Result` templates and variants,
/// and the core trait markers the type checker treats specially.
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
                | "Display"
                | "Iterator"
                | "IntoIter"
                | "From"
        ),
    }
}

/// Variant names collected automatically for a marked std enum.
///
/// When `Option` or `Result` is registered as an enum language item, sibling variant
/// definitions in the same module are linked without requiring explicit
/// `#[lang_item(kind = "variant", …)]` on each ctor.
#[must_use]
pub fn auto_variants_for_enum(enum_name: &str) -> &'static [&'static str] {
    match enum_name {
        "Option" => &["Some", "None"],
        "Result" => &["Ok", "Err"],
        _ => &[],
    }
}
