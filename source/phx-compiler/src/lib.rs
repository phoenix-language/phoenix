//! Phoenix compiler — resolve, type-check, lower, and codegen to bytecode.
//!
//! This crate implements the Phoenix compilation pipeline from source text to verified-ready
//! [`BytecodeModule`] images. The `phx` CLI wraps these APIs; verify and VM execution live in
//! `phx-bytecode` and `phx-vm`.
//!
//! ## Pipeline overview
//!
//! ```text
//! source / path → parse → cfg + derive → resolve → typeck → [lint] → lower → codegen → PHX0
//! ```
//!
//! - **Check-only:** [`compile_source`], [`check_file`], [`facade::check_file`] — stop after type-check.
//! - **Full compile:** [`compile_to_module`], [`facade::compile_to_module`], [`build_project`] — emit bytecode and (for projects) link artifacts under `build/`.
//!
//! ## API stability tiers
//!
//! **Tier 1 — stable (embedders):** [`facade`] ([`facade::CheckOutput`], [`facade::CompileOutput`],
//! [`facade::check_file`], [`facade::compile_to_module`]), [`CompileError`], [`build_project`],
//! [`ProjectConfig`], [`BytecodeModule`].
//!
//! Prefer [`facade::check_file`] / [`facade::compile_to_module`] for LSP, SDK, and other external
//! tools — they hide [`CompilationUnit`](unstable::CompilationUnit) and internal side tables.
//!
//! **Tier 2 — driver (CLI / in-repo):** [`compile_source`], [`check_file`], [`compile_to_module`],
//! [`DiagnosticContext`], [`modules`], [`standalone`]. May evolve; does not expose internal graph
//! layout at the crate root.
//!
//! **Tier 3 — unstable:** [`unstable`] — [`unstable::ResolvedProgram`], [`unstable::TypedProgram`], IR, and pass
//! entry points for tests and contributor tooling. Not semver-stable.
//!
//! ## Module map
//!
//! - [`compile`] (re-exported at root) — single-file and multi-file compile/check drivers
//! - [`facade`] — stable embedder entry points
//! - [`build`] — `phx build`, manifest, path-dependency prebuild, link
//! - [`project`] — `phoenix.toml` discovery and layout
//! - [`unstable`] — internal graphs and pass hooks for tests

#![allow(clippy::result_large_err)] // `CompileError::TypeCheck` carries full `DiagnosticContext`.

mod attrs;
mod build;
mod byte_size;
mod cfg;
mod codegen;
mod compile;
mod derive;
#[cfg(test)]
mod embed;
pub mod facade;
mod ir;
mod lang_items;
mod link;
mod lint;
mod lower;
mod modules;
mod project;
mod pxi;
mod resolver;
mod standalone;
mod typeck;
mod unit;
pub mod unstable;

pub use build::{
    BuildError, BuildOptions, BuildProfile, BuildResult, LoadOptions, build_project,
    emit_interfaces_from_compiled, load_project_binary, load_project_binary_with_options,
};
pub use byte_size::{ByteSizeError, parse_byte_size};
pub use cfg::{CompileCfg, strip_cfg};
pub use compile::{
    CompileError, DiagnosticContext, check_file, check_file_with_module_path, check_project_file,
    compile_compilation_unit, compile_source, compile_source_with_module_root, compile_to_module,
    compile_to_module_with_module_path, format_lints, lint_checked,
};
pub use derive::{DeriveError, expand_derives};
pub use facade::{CheckOutput, CompileOutput};
pub use lang_items::{
    LangItemKind, LangItemMarker, LangItemRegistry, build_lang_item_registry, lang_item_from_attrs,
};
pub use link::{LinkError, LinkInput, link_modules};
pub use modules::{
    LoadedModule, LoadedProgram, ProgramLoadContext, load_program_with_context,
    resolve_loaded_program,
};
pub use phx_bytecode::BytecodeModule;
pub use project::{
    BuildLayout, PackageType, ProjectConfig, ProjectError, discover_project, resolve_project,
};
pub use pxi::{PxiExport, PxiFile, PxiLangItem, PxiType, digest_bytes, digest_file};
pub use standalone::{
    StandaloneOptions, check_standalone_unit_with_context, check_standalone_with_context,
    compile_standalone_with_context,
};
