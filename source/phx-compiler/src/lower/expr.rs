//! Lower expressions to IR instructions.

use crate::ir::IrBasicBlock;
use crate::typeck::TypedProgram;
use phx_syntax::ast::ExprNode;

/// Lowers `expr` into `block` using type information from `typed` (not yet implemented).
#[expect(dead_code, reason = "scaffold for expression lowering")]
pub fn lower_expr(_typed: &TypedProgram, _expr: &ExprNode, _block: &mut IrBasicBlock) {}
