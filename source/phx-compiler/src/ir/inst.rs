//! IR instructions and stack effects (MVP subset).
//!
//! Aligns with opcode families in `docs/design/features/vm-linear.md`; numeric opcodes are
//! assigned during codegen.

use crate::resolver::DefId;
pub use crate::typeck::LocalSlot;
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

// `LocalSlot` is defined in typeck (`FunctionLayout`); IR uses the same indices.

/// One IR instruction with documented stack effect (MVP subset).
///
/// ## Opcode mapping (MVP)
///
/// | [`IrInst`] | [`phx_bytecode::Opcode`] |
/// |---|---|
/// | `Const` | `Const` |
/// | `LoadLocal` | `LoadLocal` |
/// | `StoreLocal` | `StoreLocal` |
/// | `BinOp::Add` etc. | `Add`, `Sub`, `Mul`, `Div`, `Eq`, `Lt` |
/// | `Call` | `Call` |
/// | `Return` | `Return` |
/// | `Jump` | `Jump` |
/// | `JumpIf` | `JumpIfTrue` / `JumpIfFalse` |
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
    /// Build struct aggregate. Stack: `[fields…] → [agg]`
    MakeStruct {
        /// Bytecode type id.
        type_id: u32,
        /// Number of fields (stack operands).
        field_count: u32,
    },
    /// Build enum variant. Stack: `[payload…] → [agg]`
    MakeEnum {
        /// Bytecode type id.
        type_id: u32,
        /// Variant tag.
        variant_tag: u32,
        /// Payload count.
        payload_count: u32,
    },
    /// Read field or enum payload slot. Stack: `[agg] → [value]`
    GetField {
        /// Bytecode type id.
        type_id: u32,
        /// Field index.
        field_index: u32,
        /// Result type.
        result: TypeId,
    },
    /// Write struct field in-place. Stack: `[agg, value] → [agg]`
    SetField {
        /// Bytecode type id.
        type_id: u32,
        /// Field index.
        field_index: u32,
    },
    /// Compare enum tag. Stack: `[agg] → [bool]`
    MatchTag {
        /// Bytecode type id.
        type_id: u32,
        /// Expected variant tag.
        variant_tag: u32,
    },
    /// Explicit primitive cast. Stack: `[value] → [value]`
    Cast {
        /// Source primitive kind wire byte.
        from_kind: u8,
        /// Target primitive kind wire byte.
        to_kind: u8,
    },
    /// Unary negate. Stack: `[a] → [-a]`
    Neg {
        /// Result type.
        result: TypeId,
    },
    /// Logical not. Stack: `[bool] → [bool]`
    Not {
        /// Result type.
        result: TypeId,
    },
    /// Bitwise not. Stack: `[a] → [~a]`
    BitNot {
        /// Result type.
        result: TypeId,
    },
    /// Build tuple. Stack: `[elems…] → [agg]`
    MakeTuple {
        /// Element count.
        arity: u32,
    },
    /// Build fixed array. Stack: `[elems…] → [agg]`
    MakeArray {
        /// Element count.
        len: u32,
    },
    /// Index tuple or array. Stack: `[agg, index] → [elem]`
    Index {
        /// Element result type.
        result: TypeId,
    },
    /// Runtime trap for non-exhaustive `given` / match failure.
    TrapGivenMismatch,
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
    /// `!=`
    Ne,
    /// `<=`
    Le,
    /// `>=`
    Ge,
    /// `%`
    Mod,
    /// `**`
    Pow,
    /// `&`
    BitAnd,
    /// `|`
    BitOr,
    /// `^`
    BitXor,
    /// `<<`
    Shl,
    /// `>>`
    Shr,
}
