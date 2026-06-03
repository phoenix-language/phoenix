//! Struct/enum layout tables for lowering and bytecode metadata.

use std::collections::HashMap;

use phx_syntax::Symbol;

use super::types::TypeId;
use crate::resolver::DefId;

/// Ordered struct fields for codegen indices.
#[derive(Debug, Clone)]
pub struct StructLayout {
    /// Fields in declaration order.
    pub fields: Vec<(Symbol, TypeId)>,
}

/// One enum variant layout entry.
#[derive(Debug, Clone)]
pub struct VariantLayout {
    /// Variant definition id.
    pub def: DefId,
    /// Variant name.
    pub name: Symbol,
    /// Runtime tag (declaration order).
    pub tag: u32,
    /// Variant shape.
    pub kind: VariantKind,
}

/// Enum variant payload shape.
#[derive(Debug, Clone)]
pub enum VariantKind {
    /// Unit variant — no payload.
    Unit,
    /// Tuple variant fields.
    Tuple(Vec<TypeId>),
    /// Struct variant fields (declaration order).
    Struct(Vec<(Symbol, TypeId)>),
}

impl VariantKind {
    /// Payload slot count for enum construction / match bind.
    #[must_use]
    pub fn payload_len(&self) -> usize {
        match self {
            Self::Unit => 0,
            Self::Tuple(ts) => ts.len(),
            Self::Struct(fs) => fs.len(),
        }
    }
}

/// Enum type layout.
#[derive(Debug, Clone)]
pub struct EnumLayout {
    /// Parent enum definition id.
    pub enum_def: DefId,
    /// Variants in tag order.
    pub variants: Vec<VariantLayout>,
}

/// Maps variant ctor defs to parent enum + tag.
#[derive(Debug, Clone)]
pub struct VariantMeta {
    /// Parent enum definition.
    pub enum_def: DefId,
    /// Runtime tag.
    pub tag: u32,
    /// Payload field types (empty for unit).
    pub payload: VariantKind,
}

/// Layout tables produced by type checking.
#[derive(Debug, Clone, Default)]
pub struct ProgramLayout {
    /// Struct field order by struct def.
    pub structs: HashMap<DefId, StructLayout>,
    /// Enum layouts by enum def.
    pub enums: HashMap<DefId, EnumLayout>,
    /// Stable bytecode type id per struct/enum def.
    pub type_ids: HashMap<DefId, u32>,
    /// Enum variant ctor metadata.
    pub variants: HashMap<DefId, VariantMeta>,
    /// Inherent impl methods: `(type_def, method_name) → fn_def`.
    pub inherent_methods: HashMap<(DefId, Symbol), DefId>,
    /// Trait impl methods: `(type_def, trait_def, method_name) → fn_def`.
    pub trait_methods: HashMap<(DefId, DefId, Symbol), DefId>,
}

impl ProgramLayout {
    /// Returns bytecode `type_id` for `def`.
    #[must_use]
    pub fn type_id(&self, def: DefId) -> Option<u32> {
        self.type_ids.get(&def).copied()
    }

    /// Returns field index for a struct field name.
    #[must_use]
    pub fn struct_field_index(&self, def: DefId, field: Symbol) -> Option<u32> {
        self.structs.get(&def).and_then(|s| {
            s.fields
                .iter()
                .position(|(name, _)| *name == field)
                .map(|i| u32::try_from(i).unwrap_or(u32::MAX))
        })
    }

    /// Finds an enum variant by ctor name across all enums.
    #[must_use]
    pub fn enum_variant_by_name(&self, name: Symbol) -> Option<(DefId, VariantLayout)> {
        for el in self.enums.values() {
            if let Some(v) = el.variants.iter().find(|v| v.name == name) {
                return Some((el.enum_def, v.clone()));
            }
        }
        None
    }
}
