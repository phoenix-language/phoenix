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

mod codegen;
mod compile;
mod ir;
mod lower;
mod resolver;
mod typeck;
mod unit;

pub use codegen::{build_type_table, codegen};
pub use compile::{CompileError, check_file, compile_source, compile_to_module};
pub use ir::{IrBasicBlock, IrBinOp, IrFunction, IrFunctionId, IrInst, IrModule, LocalSlot};
pub use lower::lower;
pub use phx_bytecode::BytecodeModule;
pub use resolver::{Def, DefId, DefKind, ResolutionKey, ResolvedProgram, resolve};
pub use typeck::{
    Binding, BindingKind, ExprId, FunctionLayout, Ty, TypeId, TypeInterner, TypedProgram,
    type_check,
};
pub use unit::CompilationUnit;
