//! VM intrinsic definitions in std (`alloc_bytes`, …).
//!
//! Records [`DefId`]s for compiler-known intrinsics lowered to dedicated opcodes — not ordinary
//! [`IrInst::Call`] targets.

use phx_syntax::Interner;

use crate::resolver::{DefId, DefKind, ResolvedProgram};

/// Logical module path for std heap allocation.
const STD_ALLOC_MODULE: &str = "std::core::alloc";

/// Canonical intrinsic function ids discovered in linked std modules.
#[derive(Debug, Clone, Default)]
pub struct IntrinsicKernel {
    /// `std::core::alloc::alloc_bytes`
    pub alloc_bytes: Option<DefId>,
}

/// Lowering hint for a call to a compiler intrinsic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntrinsicSite {
    /// `alloc_bytes(size)` → `ALLOC` opcode.
    AllocBytes,
}

impl IntrinsicKernel {
    /// Scans `resolved` for bundled std intrinsic definitions.
    #[must_use]
    pub fn build(resolved: &ResolvedProgram) -> Self {
        let mut kernel = Self::default();
        let Some(mod_id) = module_id(resolved, STD_ALLOC_MODULE) else {
            return kernel;
        };
        kernel.alloc_bytes = find_def(
            resolved,
            &resolved.interner,
            mod_id,
            "alloc_bytes",
            DefKind::Fn,
        );
        kernel
    }

    /// Returns the intrinsic site for a direct call to `def`, if any.
    #[must_use]
    pub fn site_for_call(&self, def: DefId) -> Option<IntrinsicSite> {
        if self.alloc_bytes == Some(def) {
            Some(IntrinsicSite::AllocBytes)
        } else {
            None
        }
    }

    /// Returns `true` when `def` is an intrinsic template whose body must not be lowered.
    #[must_use]
    pub fn is_intrinsic_fn(&self, def: DefId) -> bool {
        self.alloc_bytes == Some(def)
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
