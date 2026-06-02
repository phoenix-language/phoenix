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
    /// `given` pattern did not match scrutinee.
    GivenMismatch,
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
            Self::GivenMismatch => write!(f, "given pattern did not match"),
        }
    }
}

impl std::error::Error for VmError {}
