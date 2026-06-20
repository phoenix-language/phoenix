//! Parse `#[lang_item(name = "...", kind = "...")]` bracket attributes.
//!
//! Converts Phoenix bracket attributes on std items into a [`LangItemMarker`]. Used when
//! scanning resolved source during registry collection and when rehydrating markers from
//! dependency `.pxi` import metadata.
//!
//! Attribute shape (v1):
//!
//! ```text
//! #[lang_item(name = "Option", kind = "enum")]
//! ```
//!
//! Only the first attribute named `lang_item` whose `name` and `kind` string arguments both
//! parse successfully is returned; additional attributes on the same item are ignored.
//!
//! ## Pipeline position
//!
//! Called from [`super::collect::build_lang_item_registry`] while walking
//! [`ResolvedProgram`](crate::resolver::ResolvedProgram) modules. Parsing is intentionally
//! permissive — unknown `kind` strings cause that attribute to be skipped rather than
//! producing a diagnostic here. Callers validate the returned marker against the closed
//! registry via [`super::validate::is_known_lang_item`].
//!
//! ## Owning pass
//!
//! - **Type checking (setup)** — attribute extraction only; no registry mutation or
//!   duplicate detection happens in this module.
//!
//! ## In this module
//!
//! - [`lang_item_from_attrs`] — scan bracket attributes and return the first well-formed marker.

use phx_syntax::Interner;
use phx_syntax::ast::Node;
use phx_syntax::ast::attr::{AttrArg, AttrValue, Attribute};

use super::LangItemKind;
use super::LangItemMarker;

/// Parses a language item marker from bracket attributes, if present.
///
/// Scans `attrs` in source order and returns the first `lang_item` attribute whose `name`
/// and `kind` string arguments both parse successfully. Unknown `kind` strings cause that
/// attribute to be skipped rather than producing a diagnostic — callers in
/// [`super::collect`] validate the result against the closed registry via
/// [`super::validate::is_known_lang_item`].
///
/// Returns `None` when no attribute is named `lang_item`, when required arguments are
/// missing, or when every `lang_item` attribute has an unparseable `kind`.
///
/// # Panics
///
/// Never panics on malformed attribute syntax.
#[must_use]
pub fn lang_item_from_attrs(
    interner: &Interner,
    attrs: &[Node<Attribute>],
) -> Option<LangItemMarker> {
    for attr in attrs {
        if !crate::attrs::attr_named(interner, &attr.inner, "lang_item") {
            continue;
        }
        let mut name: Option<String> = None;
        let mut kind: Option<LangItemKind> = None;
        for arg in &attr.inner.args {
            let AttrArg::Named { name: key, value } = arg else {
                continue;
            };
            let Some(key_str) = interner.resolve(key.symbol) else {
                continue;
            };
            match (key_str, value) {
                ("name", AttrValue::Str(s)) => name = Some(s.clone()),
                ("kind", AttrValue::Str(s)) => kind = LangItemKind::parse(s),
                _ => {}
            }
        }
        if let (Some(name), Some(kind)) = (name, kind) {
            return Some(LangItemMarker { name, kind });
        }
    }
    None
}
