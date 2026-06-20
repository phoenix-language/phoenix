//! Language item registry: compiler-known std definitions from `#[lang_item]` markers.
//!
//! Phoenix std marks canonical definitions (`Option`/`Result`, core traits, VM intrinsics)
//! with `#[lang_item(name = "...", kind = "...")]`. This module collects those markers
//! during type checking, validates them against a closed v1 registry, and exposes a
//! [`LangItemRegistry`] keyed by [`DefId`](crate::resolver::DefId) for later passes.
//!
//! # Pipeline position
//!
//! [`build_lang_item_registry`] runs at the start of type checking (see
//! `typeck::check::decl`) after name resolution. The registry is stored on
//! [`TypedProgram`](crate::typeck::TypedProgram) and consulted by expression checking,
//! monomorphization, lowering, and PXI export.
//!
//! # Sources
//!
//! Markers are collected from:
//!
//! - `#[lang_item]` attributes on items under the `std::` module tree (source)
//! - Dependency `.pxi` import metadata via
//!   [`ResolvedProgram::import_lang_items`](crate::resolver::ResolvedProgram)
//!
//! User modules may not declare language items — attempts produce
//! [`TypeCheckError::LangItemReserved`](crate::typeck::TypeCheckError::LangItemReserved).

mod collect;
mod parse;
mod registry;
mod validate;

pub use collect::build_lang_item_registry;
pub use parse::lang_item_from_attrs;
pub use registry::{LangItemKind, LangItemMarker, LangItemRegistry};
