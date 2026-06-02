//! Lower statements to IR control flow.

use crate::ir::IrBasicBlock;
use crate::typeck::TypedProgram;
use phx_syntax::ast::stmt::Block;

/// Lowers `block` into IR (not yet implemented).
#[expect(dead_code, reason = "scaffold for statement lowering")]
pub fn lower_block(_typed: &TypedProgram, _block: &Block, _out: &mut IrBasicBlock) {}
