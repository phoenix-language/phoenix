//! Parse `#[lang_item(name = "...", kind = "...")]` from bracket attributes.

use phx_syntax::Interner;
use phx_syntax::ast::Node;
use phx_syntax::ast::attr::{AttrArg, AttrValue, Attribute};

use super::LangItemKind;
use super::LangItemMarker;

/// Parses a language item marker from bracket attributes, if present.
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
