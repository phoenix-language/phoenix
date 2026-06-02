//! Phoenix compiler — resolve, type-check, lower, and codegen to bytecode.
//!
//! MVP pipeline wired today: [`compile_source`] runs [`phx_syntax::parse`] then [`resolver::resolve`].
//! Later passes (typeck, IR, bytecode) will consume [`CompilationUnit`].
//!
//! ## Modules
//!
//! - [`compile`] — [`compile_source`], [`check_file`], [`CompileError`].
//! - [`unit`] — [`CompilationUnit`] (owned source + [`ResolvedProgram`]).
//! - [`resolver`] — single-file name resolution and [`DefId`] tables.

mod compile;
mod resolver;
mod typeck;
mod unit;

pub use compile::{CompileError, check_file, compile_source};
pub use resolver::{Def, DefId, DefKind, ResolutionKey, ResolvedProgram, resolve};
pub use typeck::{ExprId, Ty, TypeId, TypeInterner, TypedProgram, type_check};
pub use unit::CompilationUnit;
