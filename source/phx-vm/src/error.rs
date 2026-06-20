//! VM runtime errors surfaced by the stack interpreter.
//!
//! Failures are classified by [`VmErrorKind`]. [`VmError`] optionally records a bytecode site
//! `(function_id, pc)` for dispatch-time faults. `Display` on [`VmError`] renders the kind and
//! appends ` (function {function_id}, pc {pc})` when both fields are present.
//!
//! ## Site attribution
//!
//! - **With site** — instruction dispatch failures ([`VmErrorKind::StackUnderflow`],
//!   [`VmErrorKind::InvalidLocalSlot`], heap faults, and similar) carry the function id and PC of
//!   the faulting instruction (before PC advance).
//! - **Without site** — module-level setup failures ([`VmErrorKind::NoEntryPoint`],
//!   [`VmErrorKind::MissingEntry`], [`VmErrorKind::EntryArityNotZero`]) use
//!   [`VmError::without_site`].
//!
//! ## Kind taxonomy
//!
//! | Category | Examples |
//! | --- | --- |
//! | Module / entry | [`VmErrorKind::NoEntryPoint`], [`VmErrorKind::MissingEntry`], [`VmErrorKind::EntryArityNotZero`] |
//! | Operand stack | [`VmErrorKind::StackUnderflow`], [`VmErrorKind::ExpectedScalar`] |
//! | Indices / ids | [`VmErrorKind::InvalidFunctionId`], [`VmErrorKind::InvalidLocalSlot`], [`VmErrorKind::InvalidConstIndex`] |
//! | Control flow | [`VmErrorKind::TruncatedCode`], [`VmErrorKind::GivenMismatch`] (`Trap` / `match`) |
//! | Heap | [`VmErrorKind::OutOfMemory`], [`VmErrorKind::HeapOutOfBounds`], [`VmErrorKind::UseAfterFree`], [`VmErrorKind::DoubleFree`], [`VmErrorKind::InvalidFree`] |
//! | MVP limits | [`VmErrorKind::UnsupportedOpcode`], [`VmErrorKind::UnsupportedConst`], [`VmErrorKind::UnsupportedArithOp`] |

/// Failure kind during bytecode execution (no bytecode site).
///
/// See the module docs for a grouped taxonomy. Variants map to user-facing `Display` strings on
/// [`VmError`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmErrorKind {
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
    /// Load or store through a pointer to freed heap memory.
    UseAfterFree,
}

impl std::fmt::Display for VmErrorKind {
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
            Self::UseAfterFree => write!(f, "use after free of heap memory"),
        }
    }
}

/// Failure during bytecode execution with optional coarse bytecode site.
///
/// Construct with [`Self::at`] when the faulting instruction is known, or [`Self::without_site`]
/// for module-level failures. `Display` formats as `{kind}` or `{kind} (function {id}, pc {pc})`.
///
/// # Examples
///
/// ```
/// use phx_vm::{VmError, VmErrorKind};
///
/// let err = VmError::at(0, 12, VmErrorKind::StackUnderflow);
/// assert_eq!(err.to_string(), "stack underflow (function 0, pc 12)");
///
/// let err = VmError::without_site(VmErrorKind::NoEntryPoint);
/// assert_eq!(err.to_string(), "module has no entry function");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmError {
    /// Underlying failure kind.
    pub kind: VmErrorKind,
    /// Function id at the faulting instruction, when known.
    pub function_id: Option<u32>,
    /// Byte offset of the faulting instruction in that function's code, when known.
    pub pc: Option<u32>,
}

impl VmError {
    /// Builds an error attributed to `function_id` and `pc`.
    ///
    /// `pc` is the byte offset of the faulting instruction in that function's code section (before
    /// PC advance).
    #[must_use]
    pub const fn at(function_id: u32, pc: u32, kind: VmErrorKind) -> Self {
        Self {
            kind,
            function_id: Some(function_id),
            pc: Some(pc),
        }
    }

    /// Builds an error with no bytecode site (module-level or non-dispatch failures).
    ///
    /// Used for [`VmErrorKind::NoEntryPoint`], [`VmErrorKind::MissingEntry`], and
    /// [`VmErrorKind::EntryArityNotZero`].
    #[must_use]
    pub const fn without_site(kind: VmErrorKind) -> Self {
        Self {
            kind,
            function_id: None,
            pc: None,
        }
    }
}

impl std::fmt::Display for VmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)?;
        if let (Some(function_id), Some(pc)) = (self.function_id, self.pc) {
            write!(f, " (function {function_id}, pc {pc})")?;
        }
        Ok(())
    }
}

impl std::error::Error for VmErrorKind {}

impl std::error::Error for VmError {}
