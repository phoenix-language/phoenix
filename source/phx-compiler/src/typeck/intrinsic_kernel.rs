//! VM intrinsic definitions in std (`alloc_bytes`, `slice_from_raw_parts`, …).
//!
//! Records [`DefId`]s for compiler-known intrinsics lowered to dedicated opcodes — not ordinary
//! [`IrInst::Call`] targets.

use phx_syntax::Interner;

use crate::resolver::{DefId, DefKind, ResolvedProgram};

/// Logical module path for std heap allocation.
const STD_ALLOC_MODULE: &str = "std::core::alloc";
/// Logical module path for std slice construction.
const STD_SLICE_MODULE: &str = "std::core::slice";

/// Canonical intrinsic function ids discovered in linked std modules.
#[derive(Debug, Clone, Default)]
pub struct IntrinsicKernel {
    /// `std::core::alloc::alloc_bytes`
    pub alloc_bytes: Option<DefId>,
    /// `std::core::slice::slice_from_raw_parts`
    pub slice_from_raw_parts: Option<DefId>,
}

/// Lowering hint for a call to a compiler intrinsic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntrinsicSite {
    /// `alloc_bytes(size)` → `ALLOC` opcode.
    AllocBytes,
    /// `slice_from_raw_parts(ptr, len)` → `MAKE_SLICE_FROM_PTR` opcode.
    SliceFromRawParts,
}

impl IntrinsicKernel {
    /// Scans `resolved` for bundled std intrinsic definitions.
    #[must_use]
    pub fn build(resolved: &ResolvedProgram) -> Self {
        let mut kernel = Self::default();
        if let Some(mod_id) = module_id(resolved, STD_ALLOC_MODULE) {
            kernel.alloc_bytes = find_def(
                resolved,
                &resolved.interner,
                mod_id,
                "alloc_bytes",
                DefKind::Fn,
            );
        }
        if let Some(mod_id) = module_id(resolved, STD_SLICE_MODULE) {
            kernel.slice_from_raw_parts = find_def(
                resolved,
                &resolved.interner,
                mod_id,
                "slice_from_raw_parts",
                DefKind::Fn,
            );
        }
        kernel
    }

    /// Returns the intrinsic site for a direct call to `def`, if any.
    #[must_use]
    pub fn site_for_call(&self, def: DefId) -> Option<IntrinsicSite> {
        if self.alloc_bytes == Some(def) {
            Some(IntrinsicSite::AllocBytes)
        } else if self.slice_from_raw_parts == Some(def) {
            Some(IntrinsicSite::SliceFromRawParts)
        } else {
            None
        }
    }

    /// Returns `true` when `def` is an intrinsic template whose body must not be lowered.
    #[must_use]
    pub fn is_intrinsic_fn(&self, def: DefId) -> bool {
        self.alloc_bytes == Some(def) || self.slice_from_raw_parts == Some(def)
    }
}

fn module_id(resolved: &ResolvedProgram, logical_path: &str) -> Option<u32> {
    resolved
        .modules
        .iter()
        .find(|m| m.logical_path == logical_path)
        .map(|m| m.id)
}

fn find_def(
    resolved: &ResolvedProgram,
    interner: &Interner,
    module: u32,
    name: &str,
    kind: DefKind,
) -> Option<DefId> {
    for (i, def) in resolved.defs.iter().enumerate() {
        if def.module == module && def.kind == kind && interner.resolve(def.name) == name {
            return Some(DefId::from_raw(u32::try_from(i).ok()?));
        }
    }
    None
}
