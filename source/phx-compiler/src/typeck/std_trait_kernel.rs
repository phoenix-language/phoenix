//! Canonical `std::core::*` trait definitions for bound checking and primitive dispatch.

use phx_syntax::Interner;
use phx_syntax::token::Keyword;

use super::layout::ProgramLayout;
use crate::resolver::{DefId, DefKind, ResolvedProgram};

const STD_COPYABLE_MODULE: &str = "std::core::copyable";
const STD_CLONE_MODULE: &str = "std::core::clone";
const STD_DROP_MODULE: &str = "std::core::drop";
const STD_CMP_MODULE: &str = "std::core::cmp";
const STD_FMT_MODULE: &str = "std::core::fmt";
const STD_ITER_MODULE: &str = "std::core::iter";
const STD_CONVERT_MODULE: &str = "std::core::convert";

/// Canonical std core trait definition ids.
#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, Default)]
pub struct StdTraitKernel {
    /// `std::core::copyable::Copyable`.
    pub copyable_trait: Option<DefId>,
    /// `std::core::clone::Clone`.
    pub clone_trait: Option<DefId>,
    /// `std::core::drop::Drop`.
    pub drop_trait: Option<DefId>,
    /// `std::core::cmp::PartialEq`.
    pub partial_eq_trait: Option<DefId>,
    /// `std::core::cmp::Eq`.
    pub eq_trait: Option<DefId>,
    /// `std::core::fmt::Debug`.
    pub debug_trait: Option<DefId>,
    /// `std::core::iter::Iterator`.
    pub iterator_trait: Option<DefId>,
    /// `std::core::iter::IntoIter`.
    pub into_iter_trait: Option<DefId>,
    /// `std::core::convert::From`.
    pub from_trait: Option<DefId>,
}

impl StdTraitKernel {
    /// Scans `resolved` for bundled std core trait definitions.
    #[must_use]
    pub fn build(resolved: &ResolvedProgram, _layout: &ProgramLayout) -> Self {
        let interner = &resolved.interner;
        Self {
            copyable_trait: find_trait(resolved, interner, STD_COPYABLE_MODULE, "Copyable"),
            clone_trait: find_trait(resolved, interner, STD_CLONE_MODULE, "Clone"),
            drop_trait: find_trait(resolved, interner, STD_DROP_MODULE, "Drop"),
            partial_eq_trait: find_trait(resolved, interner, STD_CMP_MODULE, "PartialEq"),
            eq_trait: find_trait(resolved, interner, STD_CMP_MODULE, "Eq"),
            debug_trait: find_trait(resolved, interner, STD_FMT_MODULE, "Debug"),
            iterator_trait: find_trait(resolved, interner, STD_ITER_MODULE, "Iterator"),
            into_iter_trait: find_trait(resolved, interner, STD_ITER_MODULE, "IntoIter"),
            from_trait: find_trait(resolved, interner, STD_CONVERT_MODULE, "From"),
        }
    }

    /// Returns the std trait `DefId` for an interned trait name, if linked.
    #[must_use]
    pub fn trait_def_for_name(&self, _interner: &Interner, name: &str) -> Option<DefId> {
        match name {
            "Copyable" => self.copyable_trait,
            "Clone" => self.clone_trait,
            "Drop" => self.drop_trait,
            "PartialEq" => self.partial_eq_trait,
            "Eq" => self.eq_trait,
            "Debug" => self.debug_trait,
            "Iterator" => self.iterator_trait,
            "IntoIter" => self.into_iter_trait,
            "From" => self.from_trait,
            _ => None,
        }
    }

    /// Returns the std trait `DefId` for an interned trait symbol, if linked.
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

    /// Returns whether a primitive satisfies a std trait bound.
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
        false
    }
}

fn module_id(resolved: &ResolvedProgram, logical_path: &str) -> Option<u32> {
    resolved
        .modules
        .iter()
        .find(|m| m.logical_path == logical_path)
        .map(|m| m.id)
}

fn find_trait(
    resolved: &ResolvedProgram,
    interner: &Interner,
    module_path: &str,
    name: &str,
) -> Option<DefId> {
    let mod_id = module_id(resolved, module_path)?;
    for (i, def) in resolved.defs.iter().enumerate() {
        if def.module == mod_id
            && def.kind == DefKind::Trait
            && interner.resolves_to(def.name, name)
        {
            return Some(DefId::from_raw(u32::try_from(i).ok()?));
        }
    }
    None
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
