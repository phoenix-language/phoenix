//! Type-checked compilation unit (driver output).
//!
//! ## Pass role
//!
//! A [`CompilationUnit`] is the handoff between the **check** stages (parse → cfg → derive →
//! resolve → type-check) and **codegen** ([`crate::compile_compilation_unit`]). Drivers in
//! [`crate::compile`] produce it; lowering reads [`CompilationUnit::typed`] only.
//!
//! Re-exported as [`crate::unstable::CompilationUnit`] for in-repo tests and tooling. External
//! embedders should use [`crate::facade::check_file`] or [`crate::facade::compile_to_module`],
//! which do not expose this type.
//!
//! ## Lifetime
//!
//! [`CompilationUnit::source`] is an owned [`String`] so the unit can outlive file I/O buffers and
//! match diagnostic spans when reporting errors from disk. Check drivers copy entry text into the
//! unit — do not store borrowed source alongside it.
//!
//! ## Public API
//!
//! | Item | Role |
//! |---|---|
//! | [`CompilationUnit`] | Entry path, owned source, and [`TypedProgram`] after check |
//! | [`CompilationUnit::path`] | Filesystem path when loaded from disk (`None` for buffer-only compiles) |
//! | [`CompilationUnit::source`] | Owned entry-module source text |
//! | [`CompilationUnit::typed`] | Full type-checked program (defs, expr types, ownership) |

use std::path::PathBuf;

use crate::typeck::TypedProgram;

/// Type-checked Phoenix source ready for lowering and codegen.
///
/// Holds the entry file's path and owned source text together with the full [`TypedProgram`] side
/// tables (definitions, expression types, ownership state). Construct via [`crate::check_file`],
/// [`crate::compile_source`], or related drivers in [`crate::compile`]; manual construction is
/// reserved for unit tests.
#[derive(Debug, Clone)]
pub struct CompilationUnit {
    /// Source path when loaded from disk; `None` when compiled from an in-memory buffer only.
    pub path: Option<PathBuf>,
    /// Owned copy of the Phoenix source (required after `read_to_string` / for stable lifetimes).
    pub source: String,
    /// Type-checked program (resolved AST, defs, interned types, expression types).
    pub typed: TypedProgram,
}
