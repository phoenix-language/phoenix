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

mod build;
mod codegen;
mod compile;
mod ir;
mod link;
mod lower;
mod modules;
mod project;
mod pxi;
mod resolver;
mod typeck;
mod unit;

pub use build::{BuildError, BuildResult, build_project, load_project_binary};
pub use codegen::{build_type_table, codegen, codegen_module};
pub use compile::{
    CompileError, check_file, check_file_with_module_path, compile_source, compile_to_module,
    compile_to_module_with_module_path,
};
pub use ir::{IrBasicBlock, IrBinOp, IrFunction, IrFunctionId, IrInst, IrModule, LocalSlot};
pub use link::{LinkError, LinkInput, link_modules};
pub use lower::lower;
pub use phx_bytecode::BytecodeModule;
pub use project::{
    BuildLayout, PackageType, ProjectConfig, ProjectError, discover_project, resolve_project,
};
pub use resolver::{Def, DefId, DefKind, ResolutionKey, ResolvedProgram, SourceModule, resolve};
pub use typeck::{
    Binding, BindingKind, ExprId, FunctionLayout, Ty, TypeId, TypeInterner, TypedProgram,
    type_check,
};
pub use unit::CompilationUnit;
