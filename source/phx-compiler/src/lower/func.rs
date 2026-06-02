//! Lower function definitions to [`IrFunction`](crate::ir::IrFunction).

use crate::ir::IrFunction;
use crate::typeck::TypedProgram;

/// Lowers all functions in `typed` (not yet implemented).
#[must_use]
pub fn lower_functions(_typed: &TypedProgram) -> Vec<IrFunction> {
    Vec::new()
}
