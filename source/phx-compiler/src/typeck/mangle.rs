//! Stable symbol mangling for monomorphized definitions and `.pxi` export ids.
//!
//! [`mangle_symbol`] and [`mangle_export_id`] produce deterministic names from template bases
//! and concrete type arguments. Lowering and the module linker use these strings for cross-crate
//! symbol resolution and interface (`.pxi`) export identity.
//!
//! # Mangling scheme
//!
//! Function specializations use `{base_name}${type_suffix}` where `type_suffix` is the
//! concatenation of each type argument's [`format_type`](super::display::format_type) spelling,
//! with non-ASCII-alphanumeric characters replaced by `_`. For example, `id` instantiated at
//! `s32` becomes `id$s32`; `pair` at `(s32, bool)` becomes `pair$s32_bool`.
//!
//! Export ids for monomorphized symbols follow
//! `{logical_module}::{mangled_name}::{kind}` (see [`mangle_export_id`]).

use super::display::format_type;
use super::types::{TypeId, TypeInterner};
use crate::resolver::{Def, DefId, ResolvedProgram};

/// Builds a mangled symbol name from a template base name and concrete type arguments.
///
/// Each entry in `args` is formatted with [`format_type`], sanitized to ASCII alphanumerics
/// (other characters become `_`), and concatenated without separators. The result is
/// `{base_name}${suffix}`.
///
/// # Examples
///
/// `mangle_symbol("id", &[s32], …)` yields `"id$s32"`.
#[must_use]
pub fn mangle_symbol(
    base_name: &str,
    args: &[TypeId],
    types: &TypeInterner,
    interner: &phx_syntax::Interner,
    defs: &[Def],
) -> String {
    let suffix: String = args
        .iter()
        .map(|a| format_type(types, interner, defs, *a))
        .map(|s| {
            s.chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect::<String>()
        })
        .collect();
    format!("{base_name}${suffix}")
}

/// Builds a stable `.pxi` / link `export_id` for a monomorphized export.
///
/// The id encodes the owning logical module path, the mangled symbol name, and an export
/// kind discriminator (for example `"fn"` or `"const"`) so the linker can distinguish
/// symbols that share a mangled name across kinds.
#[must_use]
pub fn mangle_export_id(logical_module: &str, kind: &str, mangled_name: &str) -> String {
    format!("{logical_module}::{mangled_name}::{kind}")
}

/// Returns the mangled symbol name for a specialized function definition.
///
/// Resolves `base`'s unmangled template name from `resolved`, then delegates to
/// [`mangle_symbol`]. Used when allocating specialized [`DefId`]s during monomorphization
/// and when matching call sites to existing specializations.
#[must_use]
pub fn mangle_symbol_for_specialization(
    resolved: &ResolvedProgram,
    base: DefId,
    args: &[TypeId],
    types: &TypeInterner,
) -> String {
    let base_def = &resolved.defs[base.index() as usize];
    let base_name = resolved.interner.resolve(base_def.name).unwrap_or("<?>");
    mangle_symbol(base_name, args, types, &resolved.interner, &resolved.defs)
}
