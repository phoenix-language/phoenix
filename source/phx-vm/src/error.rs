//! VM runtime errors.

/// Failure during bytecode execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmError {
    /// `entry_function_id` not found in the function table.
    MissingEntry,
    /// Entry function must have zero arity for MVP `main`.
    EntryArityNotZero,
    /// Unknown function id in `Call`.
    InvalidFunctionId(u32),
    /// Local index out of range.
    InvalidLocalSlot(u32),
    /// Constant pool index out of range.
    InvalidConstIndex(u32),
    /// Constant payload could not be decoded.
    InvalidConstPayload,
    /// Operand stack underflow.
    StackUnderflow,
    /// Division by zero.
    DivisionByZero,
    /// Instruction stream ended unexpectedly.
    TruncatedCode,
    /// Opcode not implemented in MVP interpreter.
    UnsupportedOpcode(u8),
    /// Arithmetic/compare expected a scalar operand.
    ExpectedScalar,
    /// Aggregate handle invalid or wrong shape for operation.
    InvalidAggregate,
    /// Field or payload index out of range.
    FieldOutOfRange,
    /// `match` pattern did not match scrutinee.
    GivenMismatch,
    /// Pointer access outside the VM heap.
    HeapOutOfBounds,
    /// `Free` on an address that is not a live heap allocation.
    DoubleFree,
    /// `Free` with invalid pointer, size mismatch, or out-of-bounds range.
    InvalidFree,
    /// `ConstTag::Bytes` is not loadable in MVP (use `MakeArray` lowering).
    UnsupportedConst,
    /// Header `entry_function_id` is `ENTRY_NONE` (library object).
    NoEntryPoint,
    /// Function pointer value is not tagged as `PTR_FN_TAG`.
    InvalidFnPtr,
    /// Foreign stub id was not registered.
    InvalidForeignStub(u32),
    /// Arithmetic operator is not defined for this primitive kind (e.g. `**` cut from v0).
    UnsupportedArithOp,
    /// Heap allocation would exceed the configured cap.
    OutOfMemory,
}

impl std::fmt::Display for VmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingEntry => write!(f, "missing entry function"),
            Self::EntryArityNotZero => write!(f, "entry function must have arity 0"),
            Self::InvalidFunctionId(id) => write!(f, "invalid function id {id}"),
            Self::InvalidLocalSlot(slot) => write!(f, "invalid local slot {slot}"),
            Self::InvalidConstIndex(idx) => write!(f, "invalid constant index {idx}"),
            Self::InvalidConstPayload => write!(f, "invalid constant payload"),
            Self::StackUnderflow => write!(f, "stack underflow"),
            Self::DivisionByZero => write!(f, "division by zero"),
            Self::TruncatedCode => write!(f, "truncated code stream"),
            Self::UnsupportedOpcode(op) => write!(f, "unsupported opcode {op}"),
            Self::ExpectedScalar => write!(f, "expected scalar value"),
            Self::InvalidAggregate => write!(f, "invalid aggregate value"),
            Self::FieldOutOfRange => write!(f, "field index out of range"),
            Self::GivenMismatch => write!(f, "match pattern did not match"),
            Self::HeapOutOfBounds => write!(f, "heap access out of bounds"),
            Self::DoubleFree => write!(f, "double free of heap block"),
            Self::InvalidFree => write!(f, "invalid heap deallocation"),
            Self::UnsupportedConst => {
                write!(f, "byte constant pool entries are not loadable in MVP")
            }
            Self::NoEntryPoint => write!(f, "module has no entry function"),
            Self::InvalidFnPtr => write!(f, "invalid function pointer value"),
            Self::InvalidForeignStub(id) => write!(f, "invalid foreign stub id {id}"),
            Self::UnsupportedArithOp => {
                write!(f, "unsupported arithmetic operator for primitive kind")
            }
            Self::OutOfMemory => write!(f, "heap allocation exceeded cap"),
        }
    }
}

impl std::error::Error for VmError {}
