//! Internal compiler graphs and pipeline pass entry points — **not a stable API**.
//!
//! Re-exports types and functions used by in-repo CLI wiring, compiler integration tests, lint,
//! `.pxi` export, and contributor tooling. External embedders should use [`crate::facade`]
//! ([`crate::facade::check_file`], [`crate::facade::compile_to_module`]) instead.
//!
//! Field layout, symbol paths, and the re-export set may change without notice.
//!
//! ## Pass role
//!
//! Each submodule owns one pipeline stage; this module exposes their **crate-internal** outputs so
//! tests and tools can run passes in isolation:
//!
//! ```text
//! parse → resolve → typeck → lower → codegen
//!          │          │        │        │
//!          ▼          ▼        ▼        ▼
//!   ResolvedProgram  TypedProgram  IrModule  PHX0 (via codegen helpers)
//! ```
//!
//! [`CompilationUnit`] bundles entry source path/text with a [`TypedProgram`] after
//! [`crate::check_file`] or [`crate::compile_source`]. Pass it to
//! [`crate::compile_compilation_unit`] to lower and emit bytecode without re-running type-check.
//!
//! ## Re-export map
//!
//! | Category | Key types | Entry point |
//! |---|---|---|
//! | Resolve | [`ResolvedProgram`], [`DefId`], [`DefKind`], [`ClosureInfo`] | [`resolve`] |
//! | Type-check | [`TypedProgram`], [`TypeId`], [`Ty`], [`Binding`], [`ExprId`] | [`type_check`] |
//! | Lower | [`IrModule`], [`IrFunction`], [`IrInst`], [`IrBinOp`] | [`lower`] |
//! | IR validation | — | [`validate_ir`], [`validate_function`], [`validation_enabled`] |
//! | Codegen | — | [`codegen`], [`codegen_module`], [`build_type_table`] |
//! | Driver output | [`CompilationUnit`] | [`crate::check_file`], [`crate::compile_source`] |
//!
//! ## When to use this module
//!
//! - **Integration tests** — assert on [`TypedProgram`] types, [`IrInst`] lowering, or resolver
//!   [`DefKind`] without going through the full CLI.
//! - **Contributor tooling** — lint, incremental build, or `.pxi` export that needs internal side
//!   tables after check.
//! - **Pass debugging** — call [`resolve`], [`type_check`], [`lower`], or [`codegen`] directly on
//!   fixture sources.
//!
//! Prefer [`crate::facade`] when the caller only needs success/failure or bytecode bytes.

pub use crate::codegen::{build_type_table, codegen, codegen_module};
pub use crate::ir::{
    IrBasicBlock, IrBinOp, IrFunction, IrFunctionId, IrInst, IrModule, LocalSlot, SpannedInst,
    validate_function, validate_ir, validation_enabled,
};
pub use crate::lower::lower;
pub use crate::resolver::{
    ClosureInfo, ClosureUpvar, Def, DefId, DefKind, ResolutionKey, ResolvedProgram, SourceModule,
    resolve,
};
pub use crate::typeck::{
    Binding, BindingKind, ExprId, FunctionLayout, TryFailureMode, TrySiteMeta, Ty, TypeId,
    TypeInterner, TypedProgram, format_type, type_check,
};
pub use crate::unit::CompilationUnit;
