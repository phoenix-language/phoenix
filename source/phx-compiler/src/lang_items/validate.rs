//! Closed v1 registry of valid `(kind, name)` language items.
//!
//! Phoenix std marks canonical definitions with `#[lang_item]`, but the compiler accepts only
//! the fixed set of names listed in [`is_known_lang_item`]. Unknown pairs are rejected during
//! collection with
//! [`TypeCheckError::LangItemInvalid`](crate::typeck::TypeCheckError::LangItemInvalid).
//!
//! ## Pipeline position
//!
//! Consulted by [`super::collect::build_lang_item_registry`] while scanning std source markers.
//! [`auto_variants_for_enum`] runs after enum templates are registered to link sibling variant
//! ctors (`Some`/`None`, `Ok`/`Err`) without requiring explicit `#[lang_item(kind = "variant")]`
//! on each ctor.
//!
//! ## Owning pass
//!
//! - **Type checking (setup)** — read-only validation tables; no AST or registry mutation.
//!
//! ## v1 closed set (summary)
//!
//! | [`LangItemKind`] | Names |
//! | --- | --- |
//! | `Intrinsic` | `alloc_bytes`, `dealloc_bytes`, `slice_from_raw_parts`, `len`, `size_of` |
//! | `Enum` | `Option`, `Result` |
//! | `Variant` | `Some`, `None`, `Ok`, `Err` |
//! | `Trait` | `Copyable`, `Clone`, `Drop`, `PartialEq`, `Eq`, `Debug`, `Display`, `Iterator`, `IntoIter`, `From` |
//!
//! ## In this module
//!
//! - [`is_known_lang_item`] — membership test for the closed registry.
//! - [`auto_variants_for_enum`] — variant names auto-linked for a registered enum template.

use super::LangItemKind;

/// Returns `true` when `name` is a known language item for `kind`.
///
/// The closed v1 set covers std VM intrinsics, `Option`/`Result` enum templates and
/// variant ctors, and the core trait markers the type checker treats specially. Unknown
/// pairs are rejected by [`super::collect::build_lang_item_registry`] with
/// [`TypeCheckError::LangItemInvalid`](crate::typeck::TypeCheckError::LangItemInvalid).
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

/// Variant names auto-linked for a registered std enum template.
///
/// When `Option` or `Result` is registered as an enum language item,
/// [`super::collect::build_lang_item_registry`] searches the same module for sibling
/// variant definitions and registers them without requiring explicit
/// `#[lang_item(kind = "variant", …)]` on each ctor.
///
/// Returns an empty slice for unknown `enum_name` values.
#[must_use]
pub fn auto_variants_for_enum(enum_name: &str) -> &'static [&'static str] {
    match enum_name {
        "Option" => &["Some", "None"],
        "Result" => &["Ok", "Err"],
        _ => &[],
    }
}
