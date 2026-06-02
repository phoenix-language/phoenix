//! Format [`TypeId`] for diagnostics.

use phx_syntax::Interner;

use super::types::{Ty, TypeId, TypeInterner};
use crate::resolver::{Def, DefId};

/// Formats `id` as a human-readable type string.
#[must_use]
pub fn format_type(interner: &TypeInterner, names: &Interner, defs: &[Def], id: TypeId) -> String {
    format_type_inner(interner, names, defs, id, &mut Vec::new())
}

fn format_type_inner(
    types: &TypeInterner,
    names: &Interner,
    defs: &[Def],
    id: TypeId,
    depth: &mut Vec<TypeId>,
) -> String {
    if depth.contains(&id) {
        return "<recursive>".to_owned();
    }
    depth.push(id);
    let s = match types.get(id) {
        Ty::Primitive(k) => format!("{k:?}"),
        Ty::Unit => "()".to_owned(),
        Ty::Named { def, args } => {
            let base = def_name(names, defs, *def);
            if args.is_empty() {
                base
            } else {
                let args_s: Vec<_> = args
                    .iter()
                    .map(|a| format_type_inner(types, names, defs, *a, depth))
                    .collect();
                format!("{base}<{}>", args_s.join(", "))
            }
        }
        Ty::Tuple(elems) => {
            let inner: Vec<_> = elems
                .iter()
                .map(|e| format_type_inner(types, names, defs, *e, depth))
                .collect();
            format!("({})", inner.join(", "))
        }
        Ty::Array { elem, len } => format!(
            "[{}; {len}]",
            format_type_inner(types, names, defs, *elem, depth)
        ),
        Ty::Slice(inner) => format!("[{}]", format_type_inner(types, names, defs, *inner, depth)),
        Ty::Ref { mut_, inner } => {
            let prefix = if *mut_ { "&mut " } else { "&" };
            format!(
                "{prefix}{}",
                format_type_inner(types, names, defs, *inner, depth)
            )
        }
        Ty::Ptr { mut_, inner } => {
            let prefix = if *mut_ { "*mut " } else { "*" };
            format!(
                "{prefix}{}",
                format_type_inner(types, names, defs, *inner, depth)
            )
        }
        Ty::Fn { params, ret } => {
            let ps: Vec<_> = params
                .iter()
                .map(|p| format_type_inner(types, names, defs, *p, depth))
                .collect();
            format!(
                ":: ({}) => {}",
                ps.join(", "),
                format_type_inner(types, names, defs, *ret, depth)
            )
        }
        Ty::Var(n) => format!("?{n}"),
    };
    depth.pop();
    s
}

fn def_name(names: &Interner, defs: &[Def], def: DefId) -> String {
    defs.get(def.index() as usize).map_or_else(
        || format!("def#{}", def.index()),
        |d| names.resolve(d.name).to_owned(),
    )
}
