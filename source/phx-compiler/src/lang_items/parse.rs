//! Parse `#[lang_item(name = "...", kind = "...")]` from bracket attributes.
//!
//! Used when scanning std source items and when rehydrating markers from `.pxi`
//! metadata. Only the first well-formed `lang_item` attribute on an item is returned.

use phx_syntax::Interner;
use phx_syntax::ast::Node;
use phx_syntax::ast::attr::{AttrArg, AttrValue, Attribute};

use super::LangItemKind;
use super::LangItemMarker;

/// Parses a language item marker from bracket attributes, if present.
///
/// Scans `attrs` in order and returns the first `lang_item` attribute whose `name`
/// and `kind` string arguments both parse successfully. Unknown `kind` strings
/// cause that attribute to be skipped rather than producing a diagnostic — callers
/// in [`super::collect`] validate the result against the closed registry.
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
