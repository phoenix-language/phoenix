//! Struct/enum layout tables for lowering and bytecode metadata.

use std::collections::HashMap;
use std::collections::HashSet;

use phx_syntax::Symbol;
use phx_syntax::token::Keyword;

use super::types::TypeId;
use crate::resolver::DefId;

/// Who implements a trait in [`TraitInstKey`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TraitImplementer {
    /// User-defined struct, enum, or alias.
    Type(DefId),
    /// Numeric or `bool` primitive (`s32 :: impl :: Trait`).
    Primitive(Keyword),
    /// Language `str` view (`str :: impl :: Trait`).
    Str,
}

/// Identifies a concrete trait implementation: `Target: From<Source>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TraitInstKey {
    /// Implementing type.
    pub implementer: TraitImplementer,
    /// Type arguments on the implementer (empty when monomorphic).
    pub implementer_args: Vec<TypeId>,
    /// Trait template definition.
    pub trait_def: DefId,
    /// Trait type arguments (e.g. `[Source]` for `From<Source>`).
    pub trait_args: Vec<TypeId>,
}

impl TraitInstKey {
    /// Builds a trait impl lookup key.
    #[must_use]
    pub fn new(
        implementer: TraitImplementer,
        implementer_args: Vec<TypeId>,
        trait_def: DefId,
        trait_args: Vec<TypeId>,
    ) -> Self {
        Self {
            implementer,
            implementer_args,
            trait_def,
            trait_args,
        }
    }

    /// Returns a key for a monomorphic named type impl.
    #[must_use]
    pub fn type_simple(implementer: DefId, trait_def: DefId) -> Self {
        Self::new(
            TraitImplementer::Type(implementer),
            Vec::new(),
            trait_def,
            Vec::new(),
        )
    }

    /// Returns a key for a primitive type impl (`s32 :: impl :: Trait`).
    #[must_use]
    pub fn primitive_simple(kw: Keyword, trait_def: DefId) -> Self {
        Self::new(
            TraitImplementer::Primitive(kw),
            Vec::new(),
            trait_def,
            Vec::new(),
        )
    }

    /// Returns a key for `str :: impl :: Trait`.
    #[must_use]
    pub fn str_simple(trait_def: DefId) -> Self {
        Self::new(TraitImplementer::Str, Vec::new(), trait_def, Vec::new())
    }
}

/// Key for a monomorphized struct, enum, or alias instantiation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeMonoKey {
    /// Generic template definition.
    pub base: DefId,
    /// Concrete type arguments in generic-parameter order.
    pub args: Vec<TypeId>,
}

impl TypeMonoKey {
    /// Builds a layout lookup key from a template def and concrete type arguments.
    #[must_use]
    pub fn new(base: DefId, args: Vec<TypeId>) -> Self {
        Self { base, args }
    }
}

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
    /// Tuple struct templates (`Name :: struct(T, …)`).
    pub tuple_structs: HashSet<DefId>,
    /// Enum layouts by enum def.
    pub enums: HashMap<DefId, EnumLayout>,
    /// Stable bytecode type id per struct/enum def.
    pub type_ids: HashMap<DefId, u32>,
    /// Enum variant ctor metadata.
    pub variants: HashMap<DefId, VariantMeta>,
    /// Inherent impl methods: `(type_def, method_name) → fn_def`.
    pub inherent_methods: HashMap<(DefId, Symbol), DefId>,
    /// Trait impl methods: `(TraitInstKey, method_name) → fn_def`.
    pub trait_methods: HashMap<(TraitInstKey, Symbol), DefId>,
    /// Concrete associated types: `(TraitInstKey, assoc_name) → TypeId`.
    pub trait_assoc_impls: HashMap<(TraitInstKey, Symbol), TypeId>,
    /// Types that implement a trait instantiation (including empty impl blocks).
    pub trait_impls: HashSet<TraitInstKey>,
    /// Monomorphized struct field layouts keyed by `(template, args)`.
    pub specialized_structs: HashMap<TypeMonoKey, StructLayout>,
    /// Monomorphized enum layouts keyed by `(template, args)`.
    pub specialized_enums: HashMap<TypeMonoKey, EnumLayout>,
    /// Bytecode `type_id` per monomorphized struct/enum key.
    pub specialized_type_ids: HashMap<TypeMonoKey, u32>,
    /// Bytecode `FnSig` type ids keyed by interned `Ty::Fn`.
    pub fn_sig_ids: HashMap<TypeId, u32>,
}

impl ProgramLayout {
    /// Returns bytecode `type_id` for `def`.
    #[must_use]
    pub fn type_id(&self, def: DefId) -> Option<u32> {
        self.type_ids.get(&def).copied()
    }

    /// Returns bytecode `type_id` for a named type, including monomorphized instantiations.
    #[must_use]
    pub fn type_id_for_named(&self, def: DefId, args: &[TypeId]) -> Option<u32> {
        if args.is_empty() {
            self.type_id(def)
        } else {
            self.specialized_type_ids
                .get(&TypeMonoKey::new(def, args.to_vec()))
                .copied()
        }
    }

    /// Returns struct field layout for a template or monomorphized instantiation.
    #[must_use]
    pub fn struct_layout(&self, def: DefId, args: &[TypeId]) -> Option<&StructLayout> {
        if args.is_empty() {
            self.structs.get(&def)
        } else {
            self.specialized_structs
                .get(&TypeMonoKey::new(def, args.to_vec()))
        }
    }

    /// Returns enum layout for a template or monomorphized instantiation.
    #[must_use]
    pub fn enum_layout(&self, def: DefId, args: &[TypeId]) -> Option<&EnumLayout> {
        if args.is_empty() {
            self.enums.get(&def)
        } else {
            self.specialized_enums
                .get(&TypeMonoKey::new(def, args.to_vec()))
        }
    }

    /// Returns field index for a struct field name.
    #[must_use]
    pub fn struct_field_index(&self, def: DefId, field: Symbol, args: &[TypeId]) -> Option<u32> {
        self.struct_layout(def, args).and_then(|s| {
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
