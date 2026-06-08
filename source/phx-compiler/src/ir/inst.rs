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
        /// Wire [`phx_bytecode::PrimitiveKind`] for scalar payloads.
        prim_kind: u8,
    },
    /// Load local slot. Stack: `[] → [value]`
    LoadLocal {
        /// Slot index.
        slot: LocalSlot,
        /// Slot type.
        ty: TypeId,
        /// Wire primitive kind when scalar.
        prim_kind: u8,
    },
    /// Pop value and store to local. Stack: `[value] → []`
    StoreLocal {
        /// Slot index.
        slot: LocalSlot,
        /// Expected type.
        ty: TypeId,
        /// Wire primitive kind when scalar.
        prim_kind: u8,
    },
    /// Pop two values, push result. Stack: `[a, b] → [result]`
    BinOp {
        /// Opcode discriminant (lowering/codegen maps to bytecode).
        op: IrBinOp,
        /// Result type.
        result: TypeId,
        /// Operand primitive kind.
        prim_kind: u8,
    },
    /// Pop operands, call function. Stack: `[args…] → [ret]`
    Call {
        /// Callee definition.
        callee: DefId,
        /// Return type.
        ret: TypeId,
    },
    /// Call `Drop::drop` for an owned local. Stack: `[] → []` (loads, calls, discards `()`).
    DropLocal {
        /// Local holding the value to drop.
        slot: LocalSlot,
        /// Type of the local.
        ty: TypeId,
        /// Resolved `Drop::drop` function.
        drop_fn: DefId,
        /// Wire primitive kind when scalar.
        prim_kind: u8,
    },
    /// Materialize function pointer. Stack: `[] → [fn_ptr]`
    MakeFnPtr {
        /// `0` = Phoenix function id; `1` = foreign stub id.
        target_kind: u32,
        /// Callee or stub id.
        target_id: u32,
        /// Result fn pointer type.
        ty: TypeId,
    },
    /// Call through fn pointer. Stack: `[fn_ptr, args…] → [ret]`
    CallIndirect {
        /// Type-table `FnSig` id for verifier contract.
        sig_type_id: u32,
        /// Argument count (must match signature).
        expected_arity: u32,
        /// Return type.
        ret: TypeId,
        /// `true` when callee is a foreign stub (`target_kind = 1`).
        foreign: bool,
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
    /// Discard stack top. Stack: `[value] → []`
    Pop,
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
        /// Operand primitive kind.
        prim_kind: u8,
    },
    /// Logical not. Stack: `[bool] → [bool]`
    Not {
        /// Result type.
        result: TypeId,
        /// Operand primitive kind (`bool`).
        prim_kind: u8,
    },
    /// Bitwise not. Stack: `[a] → [~a]`
    BitNot {
        /// Result type.
        result: TypeId,
        /// Operand primitive kind.
        prim_kind: u8,
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
    /// Runtime trap for non-exhaustive `match` failure.
    TrapGivenMismatch,
    /// Load primitive through raw address. Stack: `[addr] → [value]`
    PtrLoad {
        /// Result primitive wire kind.
        prim_kind: u8,
        /// `1` = signed integer load, `0` = unsigned/float.
        signed: u8,
        /// Result type.
        result: TypeId,
    },
    /// Push address of local slot. Stack: `[] → [ptr]`
    AddressOfLocal {
        /// Local slot index.
        slot: LocalSlot,
    },
    /// Resolve aggregate through a local pointer (caller frame). Stack: `[local_ptr] → [agg]`
    LoadAggViaLocalPtr,
    /// Build slice from array aggregate. Stack: `[array] → [slice]`
    MakeSlice {
        /// Element primitive wire kind (or `0xFF` for aggregates).
        elem_kind: u8,
    },
    /// Build UTF-8 `str` view from constant pool. Stack: `[] → [str]`
    MakeStr {
        /// Constant pool index (`ConstTag::Bytes`).
        pool_index: u32,
    },
    /// Convert `str` to `[u8]` slice view. Stack: `[str] → [slice]`
    StrAsSlice,
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
