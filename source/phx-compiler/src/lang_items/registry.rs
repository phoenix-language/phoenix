//! [`LangItemRegistry`] — compiler-known std definitions keyed by [`DefId`](crate::resolver::DefId).
//!
//! Maps stable `(kind, name)` language item markers to resolved definition ids. Cached fields
//! on [`LangItemRegistry`] (`option_enum`, `alloc_bytes`, trait markers, …) provide fast
//! lookups for the type checker, monomorphizer, and lowering passes without scanning the
//! entry map on every query.
//!
//! Populated once per program by [`super::build_lang_item_registry`] at the start of type
//! checking and stored on [`TypedProgram`](crate::typeck::TypedProgram). Optional fields remain
//! `None` when std is not linked or a required marker is missing from the build.
//!
//! ## Pipeline position
//!
//! Read throughout type checking (intrinsic calls, `Option`/`Result` special cases, trait
//! bound rules), monomorphization, IR lowering (opcode selection for intrinsics), and PXI
//! export (re-emitting registered markers).
//!
//! ## Owning passes
//!
//! - **Type checking** — `is_std_option` / `is_std_result`, trait bound helpers, intrinsic
//!   call sites via [`LangItemRegistry::site_for_call`].
//! - **Lowering** — variant tags for `?` desugaring and pattern matching via
//!   [`LangItemRegistry::success_tag_for`] / [`LangItemRegistry::failure_tag_for`].
//! - **PXI export** — serializes the full `entries` map for dependency consumers.
//!
//! ## In this module
//!
//! - [`LangItemKind`] — closed item category (`intrinsic`, `enum`, `variant`, `trait`).
//! - [`LangItemMarker`] — one parsed `#[lang_item]` marker (`name` + `kind`).
//! - [`LangItemRegistry`] — per-program registry with cached std/VM/trait `DefId` fields and
//!   lookup helpers for intrinsics, `Option`/`Result` typing, and trait bounds.

use phx_syntax::Interner;
use phx_syntax::token::Keyword;

use crate::resolver::DefId;
use crate::typeck::{IntrinsicSite, ProgramLayout, Ty, TypeId, TypeInterner};

/// Closed language item kind (v1).
///
/// Each kind corresponds to a distinct role in the compiler pipeline: VM intrinsics
/// become dedicated opcodes, enum/variant markers drive `Option`/`Result` special
/// cases, and trait markers seed built-in bound checking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LangItemKind {
    /// VM intrinsic lowered to dedicated opcodes instead of a function body.
    Intrinsic,
    /// Std `Option` / `Result` enum template.
    Enum,
    /// Enum variant constructor (optional explicit marker; may be auto-linked).
    Variant,
    /// Core trait definition referenced by built-in bound rules.
    Trait,
}

impl LangItemKind {
    /// Parses a `kind` string from a `#[lang_item]` attribute or `.pxi` import record.
    ///
    /// Accepts `"intrinsic"`, `"enum"`, `"variant"`, and `"trait"`. Returns `None` for
    /// unknown strings — callers treat that as a skipped or invalid marker.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "intrinsic" => Some(Self::Intrinsic),
            "enum" => Some(Self::Enum),
            "variant" => Some(Self::Variant),
            "trait" => Some(Self::Trait),
            _ => None,
        }
    }

    /// Serializes this kind for `.pxi` export and duplicate-error messages.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Intrinsic => "intrinsic",
            Self::Enum => "enum",
            Self::Variant => "variant",
            Self::Trait => "trait",
        }
    }
}

/// One `#[lang_item]` marker parsed from source or rehydrated from `.pxi` import metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LangItemMarker {
    /// Stable language item name (`alloc_bytes`, `Option`, `Copyable`, …).
    pub name: String,
    /// Item category.
    pub kind: LangItemKind,
}

/// Canonical std / VM language item definitions for one program.
///
/// Populated by [`super::build_lang_item_registry`] at the start of type checking.
/// Optional fields remain `None` when std is not linked or a required marker is missing.
#[derive(Debug, Clone, Default)]
pub struct LangItemRegistry {
    /// `std::core::option::Option` enum template.
    pub option_enum: Option<DefId>,
    /// `std::core::result::Result` enum template.
    pub result_enum: Option<DefId>,
    /// `Some` variant ctor.
    pub some_variant: Option<DefId>,
    /// `None` variant ctor.
    pub none_variant: Option<DefId>,
    /// `Ok` variant ctor.
    pub ok_variant: Option<DefId>,
    /// `Err` variant ctor.
    pub err_variant: Option<DefId>,
    /// `alloc_bytes` intrinsic.
    pub alloc_bytes: Option<DefId>,
    /// `dealloc_bytes` intrinsic.
    pub dealloc_bytes: Option<DefId>,
    /// `slice_from_raw_parts` intrinsic.
    pub slice_from_raw_parts: Option<DefId>,
    /// `len` on slices intrinsic.
    pub slice_len: Option<DefId>,
    /// `size_of` intrinsic.
    pub size_of: Option<DefId>,
    /// `Copyable` trait.
    pub copyable_trait: Option<DefId>,
    /// `Clone` trait.
    pub clone_trait: Option<DefId>,
    /// `Drop` trait.
    pub drop_trait: Option<DefId>,
    /// `PartialEq` trait.
    pub partial_eq_trait: Option<DefId>,
    /// `Eq` trait.
    pub eq_trait: Option<DefId>,
    /// `Debug` trait.
    pub debug_trait: Option<DefId>,
    /// `Display` trait.
    pub display_trait: Option<DefId>,
    /// `Iterator` trait.
    pub iterator_trait: Option<DefId>,
    /// `IntoIter` trait.
    pub into_iter_trait: Option<DefId>,
    /// `From` trait.
    pub from_trait: Option<DefId>,
    /// All registered `(kind, name)` → `DefId` for PXI emit and duplicate detection.
    entries: std::collections::HashMap<(LangItemKind, String), DefId>,
}

impl LangItemRegistry {
    /// Returns `true` when a variant language item with `name` is already registered.
    #[must_use]
    pub fn variant_registered(&self, name: &str) -> bool {
        self.entries
            .contains_key(&(LangItemKind::Variant, name.to_owned()))
    }

    /// Returns the marker for `def`, if registered.
    ///
    /// Linear scan of the entry map — intended for diagnostics and PXI export, not hot paths.
    #[must_use]
    pub fn marker_for_def(&self, def: DefId) -> Option<LangItemMarker> {
        for ((kind, name), &id) in &self.entries {
            if id == def {
                return Some(LangItemMarker {
                    name: name.clone(),
                    kind: *kind,
                });
            }
        }
        None
    }

    /// Returns the intrinsic site for a direct call to `def`, if any.
    ///
    /// Used by the expression checker and lowering to emit VM opcodes instead of
    /// inlining an intrinsic function body.
    #[must_use]
    pub fn site_for_call(&self, def: DefId) -> Option<IntrinsicSite> {
        if self.alloc_bytes == Some(def) {
            Some(IntrinsicSite::AllocBytes)
        } else if self.dealloc_bytes == Some(def) {
            Some(IntrinsicSite::DeallocBytes)
        } else if self.slice_from_raw_parts == Some(def) {
            Some(IntrinsicSite::SliceFromRawParts)
        } else if self.slice_len == Some(def) {
            Some(IntrinsicSite::SliceLen)
        } else if self.size_of == Some(def) {
            Some(IntrinsicSite::SizeOf)
        } else {
            None
        }
    }

    /// Returns `true` when `def` is an intrinsic template whose body must not be lowered.
    #[must_use]
    pub fn is_intrinsic_fn(&self, def: DefId) -> bool {
        self.site_for_call(def).is_some()
    }

    /// Returns `true` when `ty` is a monomorphized std `Option<…>`.
    #[must_use]
    pub fn is_std_option(&self, types: &TypeInterner, ty: TypeId) -> bool {
        self.option_enum
            .is_some_and(|def| enum_template(types, ty) == Some(def))
    }

    /// Returns `true` when `ty` is a monomorphized std `Result<…, …>`.
    #[must_use]
    pub fn is_std_result(&self, types: &TypeInterner, ty: TypeId) -> bool {
        self.result_enum
            .is_some_and(|def| enum_template(types, ty) == Some(def))
    }

    /// Payload type `T` from `Option<T>`.
    ///
    /// Returns `None` when `ty` is not a std `Option` instance.
    #[must_use]
    pub fn option_payload_ty(&self, types: &TypeInterner, ty: TypeId) -> Option<TypeId> {
        if !self.is_std_option(types, ty) {
            return None;
        }
        let Ty::Named { args, .. } = types.get(ty) else {
            return None;
        };
        args.first().copied()
    }

    /// `(ok, err)` types from `Result<ok, err>`.
    ///
    /// Returns `None` when `ty` is not a std `Result` instance or lacks both type args.
    #[must_use]
    pub fn result_ok_err_tys(&self, types: &TypeInterner, ty: TypeId) -> Option<(TypeId, TypeId)> {
        if !self.is_std_result(types, ty) {
            return None;
        }
        let Ty::Named { args, .. } = types.get(ty) else {
            return None;
        };
        if args.len() < 2 {
            return None;
        }
        Some((args[0], args[1]))
    }

    /// Success variant tag (`Some` / `Ok`) for a std `Option` or `Result` scrutinee.
    ///
    /// Uses [`ProgramLayout`] variant metadata to map registered variant ctors to
    /// discriminant tags for pattern lowering and `?` desugaring.
    #[must_use]
    pub fn success_tag_for(
        &self,
        layout: &ProgramLayout,
        types: &TypeInterner,
        ty: TypeId,
    ) -> Option<u32> {
        let enum_def = enum_template(types, ty)?;
        if self.option_enum == Some(enum_def) {
            variant_tag(layout, enum_def, self.some_variant)
        } else if self.result_enum == Some(enum_def) {
            variant_tag(layout, enum_def, self.ok_variant)
        } else {
            None
        }
    }

    /// Failure variant tag (`None` / `Err`) for a std `Option` or `Result` scrutinee.
    #[must_use]
    pub fn failure_tag_for(
        &self,
        layout: &ProgramLayout,
        types: &TypeInterner,
        ty: TypeId,
    ) -> Option<u32> {
        let enum_def = enum_template(types, ty)?;
        if self.option_enum == Some(enum_def) {
            variant_tag(layout, enum_def, self.none_variant)
        } else if self.result_enum == Some(enum_def) {
            variant_tag(layout, enum_def, self.err_variant)
        } else {
            None
        }
    }

    /// Returns the std trait [`DefId`](crate::resolver::DefId) for a trait name, if linked.
    ///
    /// Matches the closed v1 trait language items (`Copyable`, `Clone`, `Drop`, …). Returns
    /// `None` for user traits or when std is not linked.
    #[must_use]
    pub fn trait_def_for_name(&self, _interner: &Interner, name: &str) -> Option<DefId> {
        match name {
            "Copyable" => self.copyable_trait,
            "Clone" => self.clone_trait,
            "Drop" => self.drop_trait,
            "PartialEq" => self.partial_eq_trait,
            "Eq" => self.eq_trait,
            "Debug" => self.debug_trait,
            "Display" => self.display_trait,
            "Iterator" => self.iterator_trait,
            "IntoIter" => self.into_iter_trait,
            "From" => self.from_trait,
            _ => None,
        }
    }

    /// Returns the std trait [`DefId`](crate::resolver::DefId) for an interned trait symbol, if linked.
    #[must_use]
    pub fn trait_def_for_symbol(
        &self,
        interner: &Interner,
        trait_symbol: phx_syntax::Symbol,
    ) -> Option<DefId> {
        let name = interner.resolve(trait_symbol)?;
        self.trait_def_for_name(interner, name)
    }

    /// Returns `true` when `trait_def` is the std `Copyable` marker.
    #[must_use]
    pub fn is_copyable_trait(&self, trait_def: DefId) -> bool {
        self.copyable_trait == Some(trait_def)
    }

    /// Returns `true` when `trait_def` is the std `Drop` trait.
    #[must_use]
    pub fn is_drop_trait(&self, trait_def: DefId) -> bool {
        self.drop_trait == Some(trait_def)
    }

    /// Returns `true` when `trait_def` is the std `Iterator` trait.
    #[must_use]
    pub fn is_iterator_trait(&self, trait_def: DefId) -> bool {
        self.iterator_trait == Some(trait_def)
    }

    /// Returns `true` when `trait_def` is the std `IntoIter` trait.
    #[must_use]
    pub fn is_into_iter_trait(&self, trait_def: DefId) -> bool {
        self.into_iter_trait == Some(trait_def)
    }

    /// Returns `true` when `trait_def` is the std `From` trait.
    #[must_use]
    pub fn is_from_trait(&self, trait_def: DefId) -> bool {
        self.from_trait == Some(trait_def)
    }

    /// Returns whether a primitive keyword type satisfies a std trait bound.
    ///
    /// Used when checking trait bounds on numeric and boolean primitives before
    /// user impls are considered.
    #[must_use]
    pub fn primitive_satisfies(&self, kw: Keyword, trait_def: DefId) -> bool {
        if self.copyable_trait == Some(trait_def) {
            return is_copyable_primitive(kw);
        }
        if self.clone_trait == Some(trait_def) {
            return is_copyable_primitive(kw);
        }
        if self.partial_eq_trait == Some(trait_def) {
            return is_partial_eq_primitive(kw);
        }
        if self.eq_trait == Some(trait_def) {
            return is_eq_primitive(kw);
        }
        if self.debug_trait == Some(trait_def) {
            return is_debug_primitive(kw);
        }
        if self.display_trait == Some(trait_def) {
            return is_display_primitive(kw);
        }
        false
    }

    /// Inserts `marker` → `def_id`, updating cached fields and the entry map.
    ///
    /// Called by [`super::collect::build_lang_item_registry`] during collection. Known
    /// `(kind, name)` pairs also populate the typed cache fields on [`LangItemRegistry`]
    /// (`option_enum`, `alloc_bytes`, trait markers, …).
    ///
    /// Returns the previous `DefId` for the same `(kind, name)` key, if any.
    pub(crate) fn insert_entry(&mut self, marker: &LangItemMarker, def_id: DefId) -> Option<DefId> {
        self.assign_cached(marker.kind, &marker.name, def_id);
        self.entries
            .insert((marker.kind, marker.name.clone()), def_id)
    }

    fn assign_cached(&mut self, kind: LangItemKind, name: &str, def_id: DefId) {
        match (kind, name) {
            (LangItemKind::Intrinsic, "alloc_bytes") => self.alloc_bytes = Some(def_id),
            (LangItemKind::Intrinsic, "dealloc_bytes") => self.dealloc_bytes = Some(def_id),
            (LangItemKind::Intrinsic, "slice_from_raw_parts") => {
                self.slice_from_raw_parts = Some(def_id);
            }
            (LangItemKind::Intrinsic, "len") => self.slice_len = Some(def_id),
            (LangItemKind::Intrinsic, "size_of") => self.size_of = Some(def_id),
            (LangItemKind::Enum, "Option") => self.option_enum = Some(def_id),
            (LangItemKind::Enum, "Result") => self.result_enum = Some(def_id),
            (LangItemKind::Variant, "Some") => self.some_variant = Some(def_id),
            (LangItemKind::Variant, "None") => self.none_variant = Some(def_id),
            (LangItemKind::Variant, "Ok") => self.ok_variant = Some(def_id),
            (LangItemKind::Variant, "Err") => self.err_variant = Some(def_id),
            (LangItemKind::Trait, "Copyable") => self.copyable_trait = Some(def_id),
            (LangItemKind::Trait, "Clone") => self.clone_trait = Some(def_id),
            (LangItemKind::Trait, "Drop") => self.drop_trait = Some(def_id),
            (LangItemKind::Trait, "PartialEq") => self.partial_eq_trait = Some(def_id),
            (LangItemKind::Trait, "Eq") => self.eq_trait = Some(def_id),
            (LangItemKind::Trait, "Debug") => self.debug_trait = Some(def_id),
            (LangItemKind::Trait, "Display") => self.display_trait = Some(def_id),
            (LangItemKind::Trait, "Iterator") => self.iterator_trait = Some(def_id),
            (LangItemKind::Trait, "IntoIter") => self.into_iter_trait = Some(def_id),
            (LangItemKind::Trait, "From") => self.from_trait = Some(def_id),
            _ => {}
        }
    }
}

/// Extracts the enum template `DefId` from a named type instance.
fn enum_template(types: &TypeInterner, ty: TypeId) -> Option<DefId> {
    match types.get(ty) {
        Ty::Named { def, .. } => Some(*def),
        _ => None,
    }
}

/// Maps a variant ctor `DefId` to its discriminant tag within `enum_def`.
fn variant_tag(layout: &ProgramLayout, enum_def: DefId, variant: Option<DefId>) -> Option<u32> {
    let variant = variant?;
    layout.variants.get(&variant).and_then(|meta| {
        if meta.enum_def == enum_def {
            Some(meta.tag)
        } else {
            None
        }
    })
}

fn is_copyable_primitive(kw: Keyword) -> bool {
    matches!(
        kw,
        Keyword::S8
            | Keyword::S16
            | Keyword::S32
            | Keyword::S64
            | Keyword::U8
            | Keyword::U16
            | Keyword::U32
            | Keyword::U64
            | Keyword::F32
            | Keyword::F64
            | Keyword::Bool
    )
}

fn is_partial_eq_primitive(kw: Keyword) -> bool {
    is_copyable_primitive(kw)
}

fn is_eq_primitive(kw: Keyword) -> bool {
    matches!(
        kw,
        Keyword::S8
            | Keyword::S16
            | Keyword::S32
            | Keyword::S64
            | Keyword::U8
            | Keyword::U16
            | Keyword::U32
            | Keyword::U64
            | Keyword::Bool
    )
}

fn is_debug_primitive(kw: Keyword) -> bool {
    is_copyable_primitive(kw)
}

fn is_display_primitive(kw: Keyword) -> bool {
    is_copyable_primitive(kw)
}
