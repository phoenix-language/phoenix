//! MVP bytecode opcodes (stable discriminants per format version).

/// MVP instruction opcode (wire: single `u8`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Opcode {
    /// Push constant pool index. Stack: `[] → [value]`
    Const = 0,
    /// Load local slot. Stack: `[] → [value]`
    LoadLocal = 1,
    /// Pop and store to local. Stack: `[value] → []`
    StoreLocal = 2,
    /// Pop one value. Stack: `[a] → []`
    ///
    /// MVP codegen does not emit `Pop`; reserved for verifier/tests and future stmt-expr forms.
    Pop = 3,
    /// Add. Stack: `[a, b] → [sum]`
    Add = 4,
    /// Subtract. Stack: `[a, b] → [diff]`
    Sub = 5,
    /// Multiply. Stack: `[a, b] → [product]`
    Mul = 6,
    /// Divide. Stack: `[a, b] → [quotient]`
    Div = 7,
    /// Equality. Stack: `[a, b] → [bool]`
    Eq = 8,
    /// Less-than. Stack: `[a, b] → [bool]`
    Lt = 9,
    /// Unconditional jump. Stack: `[] → []`
    Jump = 10,
    /// Jump if top is true. Stack: `[bool] → []`
    JumpIfTrue = 11,
    /// Jump if top is false. Stack: `[bool] → []`
    JumpIfFalse = 12,
    /// Return from function. Stack: `[value] → []`
    Return = 13,
    /// Call function by id. Stack: `[args…] → [ret]`
    Call = 14,
    /// Build struct from field values. Stack: `[fields…] → [agg]`
    MakeStruct = 15,
    /// Build enum variant. Stack: `[payload…] → [agg]`
    MakeEnum = 16,
    /// Read struct field or enum payload slot. Stack: `[agg] → [value]`
    GetField = 17,
    /// Write struct field in-place. Stack: `[agg, value] → [agg]`
    SetField = 18,
    /// Compare enum tag. Stack: `[agg] → [bool]`
    MatchTag = 19,
    /// Explicit primitive cast. Stack: `[value] → [value]` — operands: `from_kind`, `to_kind`.
    Cast = 20,
    /// Modulo. Stack: `[a, b] → [rem]`
    Mod = 21,
    /// Integer power. Stack: `[a, b] → [pow]`
    Pow = 22,
    /// Negate. Stack: `[a] → [-a]`
    Neg = 23,
    /// Logical not. Stack: `[bool] → [bool]`
    Not = 24,
    /// Bitwise not. Stack: `[a] → [~a]`
    BitNot = 25,
    /// Bitwise and. Stack: `[a, b] → [a & b]`
    BitAnd = 26,
    /// Bitwise or. Stack: `[a, b] → [a | b]`
    BitOr = 27,
    /// Bitwise xor. Stack: `[a, b] → [a ^ b]`
    BitXor = 28,
    /// Shift left. Stack: `[a, b] → [a << b]`
    Shl = 29,
    /// Shift right. Stack: `[a, b] → [a >> b]`
    Shr = 30,
    /// Not equal. Stack: `[a, b] → [bool]`
    Ne = 31,
    /// Less or equal. Stack: `[a, b] → [bool]`
    Le = 32,
    /// Greater or equal. Stack: `[a, b] → [bool]`
    Ge = 33,
    /// Build tuple. Stack: `[elems…] → [agg]`
    MakeTuple = 34,
    /// Build fixed array. Stack: `[elems…] → [agg]`
    MakeArray = 35,
    /// Index tuple/array. Stack: `[agg, index] → [elem]`
    Index = 36,
    /// Abort execution. Stack: `[] → []` (terminator)
    Trap = 37,
    /// Allocate `size` bytes on the VM heap. Stack: `[] → [addr]` — operand: byte size.
    ///
    /// VM-ready; MVP compiler does not emit until alloc intrinsic spelling is defined in design.
    Alloc = 38,
    /// Load primitive at address. Stack: `[addr] → [value]` — operands: `size`, `signed` (0/1).
    PtrLoad = 39,
    /// Store primitive at address. Stack: `[addr, value] → []` — operands: `size`, `signed`.
    ///
    /// VM-ready; not emitted by MVP codegen (see `Alloc`).
    PtrStore = 40,
    /// Build slice from array aggregate. Stack: `[array] → [slice]` — operand: element `prim_kind`.
    MakeSlice = 41,
    /// Push address of local slot. Stack: `[] → [ptr]` — operand: slot index.
    AddressOfLocal = 42,
    /// Build UTF-8 `str` view from constant pool bytes. Stack: `[] → [str]` — operand: pool index.
    MakeStr = 43,
    /// Convert `str` aggregate to `[u8]` slice view. Stack: `[str] → [slice]`
    StrAsSlice = 44,
    /// Materialize function pointer. Stack: `[] → [fn_ptr]` — operands: `target_kind`, `target_id`.
    MakeFnPtr = 45,
    /// Call through fn pointer. Stack: `[fn_ptr, args…] → [ret]` — operands: `expected_arity`, `sig_type_id`.
    CallIndirect = 46,
}

impl Opcode {
    /// Decodes an opcode byte.
    ///
    /// # Errors
    ///
    /// Returns [`OpcodeError::Unknown`] for unrecognized values.
    pub fn from_u8(byte: u8) -> Result<Self, OpcodeError> {
        match byte {
            0 => Ok(Self::Const),
            1 => Ok(Self::LoadLocal),
            2 => Ok(Self::StoreLocal),
            3 => Ok(Self::Pop),
            4 => Ok(Self::Add),
            5 => Ok(Self::Sub),
            6 => Ok(Self::Mul),
            7 => Ok(Self::Div),
            8 => Ok(Self::Eq),
            9 => Ok(Self::Lt),
            10 => Ok(Self::Jump),
            11 => Ok(Self::JumpIfTrue),
            12 => Ok(Self::JumpIfFalse),
            13 => Ok(Self::Return),
            14 => Ok(Self::Call),
            15 => Ok(Self::MakeStruct),
            16 => Ok(Self::MakeEnum),
            17 => Ok(Self::GetField),
            18 => Ok(Self::SetField),
            19 => Ok(Self::MatchTag),
            20 => Ok(Self::Cast),
            21 => Ok(Self::Mod),
            22 => Ok(Self::Pow),
            23 => Ok(Self::Neg),
            24 => Ok(Self::Not),
            25 => Ok(Self::BitNot),
            26 => Ok(Self::BitAnd),
            27 => Ok(Self::BitOr),
            28 => Ok(Self::BitXor),
            29 => Ok(Self::Shl),
            30 => Ok(Self::Shr),
            31 => Ok(Self::Ne),
            32 => Ok(Self::Le),
            33 => Ok(Self::Ge),
            34 => Ok(Self::MakeTuple),
            35 => Ok(Self::MakeArray),
            36 => Ok(Self::Index),
            37 => Ok(Self::Trap),
            38 => Ok(Self::Alloc),
            39 => Ok(Self::PtrLoad),
            40 => Ok(Self::PtrStore),
            41 => Ok(Self::MakeSlice),
            42 => Ok(Self::AddressOfLocal),
            43 => Ok(Self::MakeStr),
            44 => Ok(Self::StrAsSlice),
            45 => Ok(Self::MakeFnPtr),
            46 => Ok(Self::CallIndirect),
            _ => Err(OpcodeError::Unknown(byte)),
        }
    }

    /// Returns the wire discriminant.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Opcode decode failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpcodeError {
    /// Unrecognized opcode byte.
    Unknown(u8),
}
