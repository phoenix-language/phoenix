//! Stable symbol mangling for monomorphized definitions and `.pxi` export ids.

use super::display::format_type;
use super::types::{TypeId, TypeInterner};
use crate::resolver::{Def, DefId, ResolvedProgram};

/// Builds a mangled symbol name such as `id$s32` from a template base name and concrete type args.
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
#[must_use]
pub fn mangle_export_id(logical_module: &str, kind: &str, mangled_name: &str) -> String {
    format!("{logical_module}::{mangled_name}::{kind}")
}

/// Returns the mangled symbol name for a specialized function definition.
#[must_use]
pub fn mangle_symbol_for_specialization(
    resolved: &ResolvedProgram,
    base: DefId,
    args: &[TypeId],
    types: &TypeInterner,
) -> String {
    let base_def = &resolved.defs[base.index() as usize];
    let base_name = resolved.interner.resolve(base_def.name);
    mangle_symbol(base_name, args, types, &resolved.interner, &resolved.defs)
}
