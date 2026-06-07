//! Types section records (metadata for verifier / debug).

/// Primitive type kind for MVP metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TypeKind {
    /// Unit `()`.
    Unit = 0,
    /// Signed integer width.
    SignedInt = 1,
    /// Unsigned integer width.
    UnsignedInt = 2,
    /// Floating point width.
    Float = 3,
    /// Boolean.
    Bool = 4,
    /// User struct (aux: field metadata).
    Struct = 5,
    /// User enum (aux: variant metadata).
    Enum = 6,
}

/// One type table record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRecord {
    /// Stable local type id.
    pub type_id: u32,
    /// Record kind.
    pub kind: TypeKind,
    /// Optional aux bytes (width, etc.).
    pub aux: Vec<u8>,
}

/// Types section body.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TypeTable {
    /// All type records.
    pub records: Vec<TypeRecord>,
}

impl TypeTable {
    /// Encodes the types section payload.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let count = u32::try_from(self.records.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&count.to_le_bytes());
        for rec in &self.records {
            let aux_len = u16::try_from(rec.aux.len()).unwrap_or(u16::MAX);
            out.extend_from_slice(&rec.type_id.to_le_bytes());
            out.push(rec.kind as u8);
            out.push(0);
            out.extend_from_slice(&aux_len.to_le_bytes());
            out.extend_from_slice(&rec.aux);
        }
        out
    }

    /// Decodes a types section payload.
    ///
    /// # Errors
    ///
    /// Returns `TypeTableError::Truncated` when bytes are incomplete.
    pub fn decode(bytes: &[u8]) -> Result<Self, TypeTableError> {
        if bytes.len() < 4 {
            return Err(TypeTableError::Truncated);
        }
        let count = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        let mut records = Vec::with_capacity(count);
        let mut pos = 4;
        for _ in 0..count {
            if pos + 8 > bytes.len() {
                return Err(TypeTableError::Truncated);
            }
            let type_id =
                u32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]);
            let kind_byte = bytes[pos + 4];
            let aux_len = u16::from_le_bytes([bytes[pos + 6], bytes[pos + 7]]) as usize;
            pos += 8;
            let end = pos.saturating_add(aux_len);
            if end > bytes.len() {
                return Err(TypeTableError::Truncated);
            }
            let kind = match kind_byte {
                0 => TypeKind::Unit,
                1 => TypeKind::SignedInt,
                2 => TypeKind::UnsignedInt,
                3 => TypeKind::Float,
                4 => TypeKind::Bool,
                5 => TypeKind::Struct,
                6 => TypeKind::Enum,
                _ => return Err(TypeTableError::UnknownKind(kind_byte)),
            };
            records.push(TypeRecord {
                type_id,
                kind,
                aux: bytes[pos..end].to_vec(),
            });
            pos = end;
        }
        Ok(Self { records })
    }
}

/// Type table decode errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeTableError {
    /// Unexpected end of payload.
    Truncated,
    /// Unknown type kind byte.
    UnknownKind(u8),
}
