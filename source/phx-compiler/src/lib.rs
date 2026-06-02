//! Phoenix compiler — resolve, type-check, lower, and codegen to bytecode.
//!
//! MVP pipeline wired today: [`compile_source`] runs parse → resolve → typeck.
//! [`lower::lower`] and [`codegen::codegen`] produce bytecode; VM execution is not wired in `compile_source` yet.
//!
//! ## Modules
//!
//! - [`compile`] — [`compile_source`], [`check_file`], [`CompileError`].
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

pub use codegen::codegen;
pub use compile::{CompileError, check_file, compile_source};
pub use phx_bytecode::BytecodeModule;
pub use ir::{IrBasicBlock, IrBinOp, IrFunction, IrFunctionId, IrInst, IrModule, LocalSlot};
pub use lower::lower;
pub use resolver::{Def, DefId, DefKind, ResolutionKey, ResolvedProgram, resolve};
pub use typeck::{
    Binding, BindingKind, ExprId, FunctionLayout, Ty, TypeId, TypeInterner, TypedProgram,
    type_check,
};
pub use unit::CompilationUnit;
