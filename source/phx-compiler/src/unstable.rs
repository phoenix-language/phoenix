//! Internal compiler graphs and pipeline passes — **not a stable API**.
//!
//! For in-repo CLI wiring, compiler integration tests, and contributor tooling only.
//! Field layout and symbol paths may change without notice. External embedders should use
//! [`crate::facade`] ([`crate::facade::check_file`], [`crate::facade::compile_to_module`]).

pub use crate::codegen::{build_type_table, codegen, codegen_module};
pub use crate::ir::{IrBasicBlock, IrBinOp, IrFunction, IrFunctionId, IrInst, IrModule, LocalSlot};
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
