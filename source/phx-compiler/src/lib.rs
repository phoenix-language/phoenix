//! Phoenix compiler — resolve, type-check, lower, and codegen to bytecode.
//!
//! [`compile_source`] and [`check_file`] run parse → resolve → typeck.
//! [`compile_to_module`] continues through [`lower::lower`] and [`codegen::codegen`].
//! Verify and VM execution are orchestrated by the `phx` CLI, not this crate.
//!
//! ## Modules
//!
//! - [`compile`] — [`compile_source`], [`check_file`], [`compile_to_module`], [`CompileError`].
//! - [`unit`] — [`CompilationUnit`] (owned source + [`TypedProgram`]).
//! - [`resolver`] — single-file name resolution and [`DefId`] tables.
//! - [`typeck`] — type checking → [`TypedProgram`].
//! - [`ir`] — intermediate representation.
//! - [`lower`] — [`TypedProgram`] → [`IrModule`].
//! - [`codegen`] — [`IrModule`] → [`phx_bytecode::BytecodeModule`].
//!
//! ## API stability
//!
//! External tools should use [`facade`] ([`check_file`], [`compile_to_module`], [`CompileOutput`],
//! [`CheckOutput`]). [`ResolvedProgram`], [`TypedProgram`], and [`CompilationUnit`] are internal
//! compiler graphs for the CLI and tests — their field layout is not stable.
//!
//! See `docs/finished-review/10-rust-code-quality.md`.

mod build;
mod codegen;
mod compile;
pub mod facade;
mod ir;
mod link;
mod lower;
mod modules;
mod project;
mod pxi;
mod resolver;
mod standalone;
mod typeck;
mod unit;

pub use build::{
    BuildError, BuildOptions, BuildResult, build_project, emit_interfaces_from_compiled,
    load_project_binary,
};
pub use codegen::{build_type_table, codegen, codegen_module};
pub use compile::{
    CompileError, DiagnosticContext, check_file, check_file_with_module_path, check_project_file,
    compile_source, compile_source_with_module_root, compile_to_module,
    compile_to_module_with_module_path,
};
pub use facade::{CheckOutput, CompileOutput};
pub use ir::{IrBasicBlock, IrBinOp, IrFunction, IrFunctionId, IrInst, IrModule, LocalSlot};
pub use link::{LinkError, LinkInput, link_modules};
pub use lower::lower;
pub use modules::{
    CrateLoadContext, LoadedCrate, LoadedModule, load_crate_with_context, resolve_crate,
};
pub use phx_bytecode::BytecodeModule;
pub use project::{
    BuildLayout, PackageType, ProjectConfig, ProjectError, discover_project, resolve_project,
};
pub use pxi::{PxiExport, PxiFile, PxiType, digest_bytes, digest_file};
#[doc(hidden)]
pub use resolver::ResolvedProgram;
pub use resolver::{
    ClosureInfo, ClosureUpvar, Def, DefId, DefKind, ResolutionKey, SourceModule, resolve,
};
pub use standalone::{
    StandaloneOptions, check_standalone_unit_with_context, check_standalone_with_context,
    compile_standalone_with_context,
};
#[doc(hidden)]
pub use typeck::TypedProgram;
pub use typeck::{
    Binding, BindingKind, ExprId, FunctionLayout, Ty, TypeId, TypeInterner, type_check,
};
#[doc(hidden)]
pub use unit::CompilationUnit;
