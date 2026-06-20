//! Phoenix interface files (`.pxi`) for separate compilation (M2).
//!
//! ## Pass role
//!
//! Each compiled module emits a JSON `.pxi` beside its `.phx0` artifact under `build/`.
//! Dependent modules read dependency `.pxi` files when interfaces are fresh
//! ([`PxiFile::source_is_fresh`]) instead of re-parsing bodies. The build driver calls
//! [`build_pxi_for_module`] after type-check; importers hydrate value types from v2
//! structured exports via [`PxiImportCtx`](import_types::PxiImportCtx).
//!
//! ## Submodules
//!
//! - [`format`] — parse and serialize [`PxiFile`], export records, stable export ids
//! - [`emit`] — construct a [`PxiFile`] from a typed module
//! - [`hash`] — content digests for `source_hash` and `pxi_hash` fields
//! - [`import_types`] — lower v2 [`PxiType`] trees into the type checker
//! - [`type_ast`] — structured type AST stored in format v2 exports
//!
//! ## Format versions
//!
//! | Version | Contents |
//! |---------|----------|
//! | `1` | `signature` string per export |
//! | `2` | Adds optional structured `type` per export (current emit) |
//!
//! Readers accept v1 and v2; writers emit v2. Field semantics and mangling rules are
//! documented in `docs/design/features/pxi-format.md`.

mod emit;
mod format;
mod hash;
mod import_types;
mod serialize_ty;
mod type_ast;

pub use emit::{build_pxi_for_module, module_dependencies};
pub use format::{
    PxiDependency, PxiError, PxiExport, PxiFile, PxiLangItem, def_kind_to_pxi, stable_export_id,
};
pub use hash::{digest_bytes, digest_file};
pub use import_types::{PxiImportCtx, build_named_def_paths, seed_value_type};
pub use type_ast::PxiType;
