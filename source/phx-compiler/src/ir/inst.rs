//! IR instructions and stack effects (MVP subset).
//!
//! Aligns with opcode families in `docs/design/features/vm-linear.md`; numeric opcodes are
//! assigned during codegen.

use crate::resolver::DefId;
use crate::typeck::TypeId;

/// Dense index of a function in an [`IrModule`](super::IrModule).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IrFunctionId(u32);

impl IrFunctionId {
    /// Creates an id from a raw index (lowering internal use).
    #[must_use]
    pub const fn from_raw(index: u32) -> Self {
        Self(index)
    }

    /// Returns the raw index.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// Operand local slot in a function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalSlot(u32);

impl LocalSlot {
    /// Creates a slot from a raw index.
    #[must_use]
    pub const fn from_raw(index: u32) -> Self {
        Self(index)
    }

    /// Returns the raw index.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// One IR instruction with documented stack effect (MVP subset).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum IrInst {
    /// Push constant pool index. Stack: `[] → [value]`
    Const {
        /// Constant index in module pool (future).
        index: u32,
        /// Result type.
        ty: TypeId,
    },
    /// Load local slot. Stack: `[] → [value]`
    LoadLocal {
        /// Slot index.
        slot: LocalSlot,
        /// Slot type.
        ty: TypeId,
    },
    /// Pop value and store to local. Stack: `[value] → []`
    StoreLocal {
        /// Slot index.
        slot: LocalSlot,
        /// Expected type.
        ty: TypeId,
    },
    /// Pop two values, push result. Stack: `[a, b] → [result]`
    BinOp {
        /// Opcode discriminant (lowering/codegen maps to bytecode).
        op: IrBinOp,
        /// Result type.
        result: TypeId,
    },
    /// Pop operands, call function. Stack: `[args…] → [ret]`
    Call {
        /// Callee definition.
        callee: DefId,
        /// Return type.
        ret: TypeId,
    },
    /// Pop value and return from function. Stack: `[value] → []` (terminator)
    Return {
        /// Returned type.
        ty: TypeId,
    },
    /// Unconditional branch. Stack: `[] → []` (terminator)
    Jump {
        /// Target block index.
        target: u32,
    },
    /// Pop condition, branch. Stack: `[bool] → []` (terminator)
    JumpIf {
        /// Target when true.
        then_block: u32,
        /// Target when false.
        else_block: u32,
    },
}

/// Binary operators mirrored from type-checked expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum IrBinOp {
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `==`
    Eq,
    /// `<`
    Lt,
}
