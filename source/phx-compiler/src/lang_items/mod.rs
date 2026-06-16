//! Language item registry: compiler-known std definitions from `#[lang_item]` markers.

mod collect;
mod parse;
mod registry;
mod validate;

pub use collect::build_lang_item_registry;
pub use parse::lang_item_from_attrs;
pub use registry::{LangItemKind, LangItemMarker, LangItemRegistry};
