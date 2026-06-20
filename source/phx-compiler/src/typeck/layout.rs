//! Struct, enum, and trait layout tables for lowering and bytecode metadata.
//!
//! During type checking, the layout pass walks type definitions and impl items to populate
//! [`ProgramLayout`]: field order, enum variant tags, monomorphized instantiations, trait impl
//! keys, and stable bytecode type-table ids. Lowering, codegen, PXI serialization, and
//! [`super::bounds`] read these tables — they do not re-derive layout from the AST.
//!
//! # Role in type checking
//!
//! Built incrementally in [`super::check`] (and related layout collection) alongside expression
//! typing. Generic templates live in `structs` / `enums`; concrete instantiations additionally
//! land in `specialized_*` maps keyed by [`TypeMonoKey`]. Trait impl discovery fills
//! [`ProgramLayout::trait_impls`], [`ProgramLayout::trait_methods`], and
//! [`ProgramLayout::trait_assoc_impls`] for dispatch and bound checking.
//!
//! # [`ProgramLayout`] tables
//!
//! | Field | Purpose |
//! | --- | --- |
//! | `structs` / `enums` | Monomorphic template layouts |
//! | `specialized_structs` / `specialized_enums` | Generic types at concrete args |
//! | `type_ids` / `specialized_type_ids` | Bytecode `type_id` indices |
//! | `variants` | Enum ctor def → parent enum + tag + payload |
//! | `inherent_methods` | `(type_def, method) → fn_def` |
//! | `trait_methods` / `trait_impls` | Trait dispatch and bound proofs |
//! | `trait_assoc_impls` | Associated type assignments per impl |
//! | `tuple_structs` | Marks tuple-struct templates |
//! | `fn_sig_ids` | Bytecode ids for interned function types |
//!
//! # Trait impl keys
//!
//! [`TraitInstKey`] identifies `Implementer: Trait<TraitArgs>` including generic arguments on
//! both sides (for example `Result<T,E>: From<E>`). [`TraitImplementer`] distinguishes named types,
//! primitives, and `str` so builtin and user impls share one lookup shape.
//!
//! # Consumers
//!
//! - [`crate::lower`] — field indices, variant tags, method [`DefId`] resolution
//! - [`super::bounds`] — generic bound satisfaction
//! - [`super::builtins`] — Copyable / Drop eligibility
//! - [`crate::pxi::serialize_ty`] — external type metadata

use std::collections::HashMap;
use std::collections::HashSet;

use phx_syntax::Symbol;
use phx_syntax::token::Keyword;

use super::types::TypeId;
use crate::resolver::DefId;

/// Who implements a trait in [`TraitInstKey`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TraitImplementer {
    /// User-defined struct, enum, or alias (`MyType: Trait`).
    Type(DefId),
    /// Numeric or `bool` primitive (`s32: Trait`).
    Primitive(Keyword),
    /// Language `str` view (`str: Trait`).
    Str,
}

/// Identifies a concrete trait implementation: `Implementer<Args>: Trait<TraitArgs>`.
///
/// Used as a hash key in [`ProgramLayout::trait_impls`] and as the prefix of
/// [`ProgramLayout::trait_methods`] keys. Both implementer and trait type arguments must match
/// exactly for lookup — blanket impls are represented with the concrete args recorded at collect
/// time.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TraitInstKey {
    /// Implementing type (named def, primitive keyword, or `str`).
    pub implementer: TraitImplementer,
    /// Type arguments on the implementer (empty when monomorphic).
    pub implementer_args: Vec<TypeId>,
    /// Trait template definition ([`DefId`] of the trait).
    pub trait_def: DefId,
    /// Trait type arguments (e.g. `[Source]` for `From<Source>`).
    pub trait_args: Vec<TypeId>,
}

impl TraitInstKey {
    /// Builds a trait impl lookup key with full implementer and trait type arguments.
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

    /// Returns a key for a monomorphic named type impl (`Type: Trait` with no type args).
    #[must_use]
    pub fn type_simple(implementer: DefId, trait_def: DefId) -> Self {
        Self::new(
            TraitImplementer::Type(implementer),
            Vec::new(),
            trait_def,
            Vec::new(),
        )
    }

    /// Returns a key for a primitive type impl (`s32: Trait`).
    #[must_use]
    pub fn primitive_simple(kw: Keyword, trait_def: DefId) -> Self {
        Self::new(
            TraitImplementer::Primitive(kw),
            Vec::new(),
            trait_def,
            Vec::new(),
        )
    }

    /// Returns a key for `str: Trait`.
    #[must_use]
    pub fn str_simple(trait_def: DefId) -> Self {
        Self::new(TraitImplementer::Str, Vec::new(), trait_def, Vec::new())
    }
}

/// Key for a monomorphized struct, enum, or alias instantiation.
///
/// Pairs a generic template [`DefId`] with concrete type arguments in generic-parameter order.
/// Used to look up specialized field layouts and bytecode type ids.
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

/// Ordered struct fields for codegen field indices.
///
/// Fields appear in source declaration order; [`ProgramLayout::struct_field_index`] maps names to
/// indices for get/set lowering.
#[derive(Debug, Clone)]
pub struct StructLayout {
    /// Fields in declaration order `(name, type)`.
    pub fields: Vec<(Symbol, TypeId)>,
}

/// One enum variant layout entry within an [`EnumLayout`].
#[derive(Debug, Clone)]
pub struct VariantLayout {
    /// Variant definition id (constructor def).
    pub def: DefId,
    /// Variant name symbol.
    pub name: Symbol,
    /// Runtime discriminant tag (declaration order, starting at 0).
    pub tag: u32,
    /// Variant payload shape.
    pub kind: VariantKind,
}

/// Enum variant payload shape.
#[derive(Debug, Clone)]
pub enum VariantKind {
    /// Unit variant — no payload slots.
    Unit,
    /// Tuple variant fields (positional payload).
    Tuple(Vec<TypeId>),
    /// Struct variant fields in declaration order.
    Struct(Vec<(Symbol, TypeId)>),
}

impl VariantKind {
    /// Payload slot count for enum construction and match binding.
    ///
    /// Unit variants return `0`; tuple and struct variants return their field counts.
    #[must_use]
    pub fn payload_len(&self) -> usize {
        match self {
            Self::Unit => 0,
            Self::Tuple(ts) => ts.len(),
            Self::Struct(fs) => fs.len(),
        }
    }
}

/// Enum type layout with variants in tag order.
#[derive(Debug, Clone)]
pub struct EnumLayout {
    /// Parent enum definition id.
    pub enum_def: DefId,
    /// Variants sorted by runtime tag.
    pub variants: Vec<VariantLayout>,
}

/// Maps variant constructor defs to parent enum metadata.
///
/// Lets lowering jump from a ctor [`DefId`] to the parent enum, tag, and payload layout without
/// scanning all enums.
#[derive(Debug, Clone)]
pub struct VariantMeta {
    /// Parent enum definition.
    pub enum_def: DefId,
    /// Runtime discriminant tag for this ctor.
    pub tag: u32,
    /// Payload field types (empty for unit variants).
    pub payload: VariantKind,
}

/// Layout tables produced by type checking and carried on [`TypedProgram`](crate::typeck::TypedProgram).
///
/// Default-constructs to empty maps for tests and incremental builds. Population happens during
/// the type-check walk; readers treat missing keys as "not found" rather than inferring layout.
#[derive(Debug, Clone, Default)]
pub struct ProgramLayout {
    /// Struct field order by monomorphic struct def.
    pub structs: HashMap<DefId, StructLayout>,
    /// Tuple struct templates (`Name :: struct(T, …)`).
    pub tuple_structs: HashSet<DefId>,
    /// Enum layouts by monomorphic enum def.
    pub enums: HashMap<DefId, EnumLayout>,
    /// Stable bytecode type id per monomorphic struct/enum def.
    pub type_ids: HashMap<DefId, u32>,
    /// Enum variant ctor metadata keyed by ctor def.
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
    /// Returns bytecode `type_id` for a monomorphic struct or enum def.
    ///
    /// Returns `None` when the def was not assigned a type-table entry during layout collection.
    #[must_use]
    pub fn type_id(&self, def: DefId) -> Option<u32> {
        self.type_ids.get(&def).copied()
    }

    /// Returns bytecode `type_id` for a named type, including monomorphized instantiations.
    ///
    /// Empty `args` delegates to [`ProgramLayout::type_id`]; non-empty args consult
    /// `specialized_type_ids`.
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

    /// Returns zero-based field index for a struct field name.
    ///
    /// Returns `None` when the struct layout is missing or the field name is not declared.
    #[must_use]
    pub fn struct_field_index(&self, def: DefId, field: Symbol, args: &[TypeId]) -> Option<u32> {
        self.struct_layout(def, args).and_then(|s| {
            s.fields
                .iter()
                .position(|(name, _)| *name == field)
                .map(|i| u32::try_from(i).unwrap_or(u32::MAX))
        })
    }

    /// Finds an enum variant by ctor name across all registered enums.
    ///
    /// Returns the parent enum def and a clone of the matching [`VariantLayout`]. Used when
    /// resolution has a variant name but not yet the parent enum def.
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
