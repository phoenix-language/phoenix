//! Phoenix compiler — resolve, type-check, lower, and codegen to bytecode.
//!
//! [`compile_source`] and [`check_file`] run parse → resolve → typeck.
//! [`compile_to_module`] continues through lowering and codegen.
//! Verify and VM execution are orchestrated by the `phx` CLI, not this crate.
//!
//! ## API stability tiers
//!
//! **Tier 1 — stable (embedders):** [`facade`] ([`facade::CheckOutput`], [`facade::CompileOutput`],
//! [`facade::check_file`], [`facade::compile_to_module`]), [`CompileError`], [`build_project`],
//! [`ProjectConfig`], [`BytecodeModule`].
//!
//! **Tier 2 — driver (CLI / in-repo):** [`compile_source`], [`check_file`], `modules`, `standalone`,
//! [`DiagnosticContext`]. May evolve; does not expose internal graph layout.
//!
//! **Tier 3 — unstable:** [`unstable`] — [`unstable::ResolvedProgram`], [`unstable::TypedProgram`], IR, and pass
//! entry points for tests and contributor tooling. Not semver-stable.

#![allow(clippy::result_large_err)] // `CompileError::TypeCheck` carries full `DiagnosticContext`.

mod attrs;
mod build;
mod cfg;
mod codegen;
mod compile;
mod derive;
pub mod facade;
mod ir;
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
    BuildError, BuildOptions, BuildResult, build_project, emit_interfaces_from_compiled,
    load_project_binary,
};
pub use cfg::{CompileCfg, strip_cfg};
pub use compile::{
    CompileError, DiagnosticContext, check_file, check_file_with_module_path, check_project_file,
    compile_source, compile_source_with_module_root, compile_to_module,
    compile_to_module_with_module_path, format_lints, lint_checked,
};
pub use derive::{DeriveError, expand_derives};
pub use facade::{CheckOutput, CompileOutput};
pub use link::{LinkError, LinkInput, link_modules};
pub use modules::{
    LoadedModule, LoadedProgram, ProgramLoadContext, load_program_with_context,
    resolve_loaded_program,
};
pub use phx_bytecode::BytecodeModule;
pub use project::{
    BuildLayout, PackageType, ProjectConfig, ProjectError, discover_project, resolve_project,
};
pub use pxi::{PxiExport, PxiFile, PxiType, digest_bytes, digest_file};
pub use standalone::{
    StandaloneOptions, check_standalone_unit_with_context, check_standalone_with_context,
    compile_standalone_with_context,
};
