//! IR function records.

use super::block::IrBasicBlock;
use super::inst::IrFunctionId;
use crate::resolver::DefId;
use crate::typeck::TypeId;

/// One lowered function body.
#[derive(Debug, Clone)]
pub struct IrFunction {
    /// Stable id within the module.
    pub id: IrFunctionId,
    /// Resolved definition (name, kind).
    pub def: DefId,
    /// Parameter types in order.
    pub params: Vec<TypeId>,
    /// Declared return type.
    pub return_type: TypeId,
    /// Local slot count (parameters + locals).
    pub local_count: u32,
    /// Basic blocks (block 0 is entry).
    pub blocks: Vec<IrBasicBlock>,
}
