//! Format [`TypeId`] for diagnostics.

use phx_syntax::Interner;

use super::types::{Ty, TypeId, TypeInterner};
use crate::resolver::{Def, DefId, DefKind};

/// Formats `id` as a human-readable type string.
#[must_use]
pub fn format_type(interner: &TypeInterner, names: &Interner, defs: &[Def], id: TypeId) -> String {
    format_type_inner(interner, names, defs, id, &mut Vec::new(), false)
}

/// Formats `id` for user-facing type errors, prefixing user-defined types with their definition kind.
#[must_use]
pub fn format_type_diagnostic(
    interner: &TypeInterner,
    names: &Interner,
    defs: &[Def],
    id: TypeId,
) -> String {
    format_type_inner(interner, names, defs, id, &mut Vec::new(), true)
}

fn format_type_inner(
    types: &TypeInterner,
    names: &Interner,
    defs: &[Def],
    id: TypeId,
    depth: &mut Vec<TypeId>,
    diagnostic: bool,
) -> String {
    if depth.contains(&id) {
        return "<recursive>".to_owned();
    }
    depth.push(id);
    let s = match types.get(id) {
        Ty::Primitive(k) => format!("{k:?}"),
        Ty::Unit => "()".to_owned(),
        Ty::Error => "<error>".to_owned(),
        Ty::Named { def, args } => {
            let base = def_name(names, defs, *def);
            let base = if diagnostic {
                format_named_diagnostic(defs, *def, &base)
            } else {
                base
            };
            if args.is_empty() {
                base
            } else {
                let args_s: Vec<_> = args
                    .iter()
                    .map(|a| format_type_inner(types, names, defs, *a, depth, diagnostic))
                    .collect();
                format!("{base}<{}>", args_s.join(", "))
            }
        }
        Ty::Tuple(elems) => {
            let inner: Vec<_> = elems
                .iter()
                .map(|e| format_type_inner(types, names, defs, *e, depth, diagnostic))
                .collect();
            format!("({})", inner.join(", "))
        }
        Ty::Array { elem, len } => format!(
            "[{}; {len}]",
            format_type_inner(types, names, defs, *elem, depth, diagnostic)
        ),
        Ty::Slice(inner) => format!(
            "[{}]",
            format_type_inner(types, names, defs, *inner, depth, diagnostic)
        ),
        Ty::Str => "str".to_owned(),
        Ty::Ref { mut_, inner } => {
            let prefix = if *mut_ { "&mut " } else { "&" };
            format!(
                "{prefix}{}",
                format_type_inner(types, names, defs, *inner, depth, diagnostic)
            )
        }
        Ty::Ptr { mut_, inner } => {
            let prefix = if *mut_ { "*mut " } else { "*" };
            format!(
                "{prefix}{}",
                format_type_inner(types, names, defs, *inner, depth, diagnostic)
            )
        }
        Ty::Fn { params, ret } => {
            let ps: Vec<_> = params
                .iter()
                .map(|p| format_type_inner(types, names, defs, *p, depth, diagnostic))
                .collect();
            format!(
                ":: ({}) => {}",
                ps.join(", "),
                format_type_inner(types, names, defs, *ret, depth, diagnostic)
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

fn format_named_diagnostic(defs: &[Def], def: DefId, base: &str) -> String {
    let Some(record) = defs.get(def.index() as usize) else {
        return base.to_owned();
    };
    let Some(kind) = def_kind_diagnostic_label(record.kind) else {
        return base.to_owned();
    };
    format!("{kind} {base}")
}

const fn def_kind_diagnostic_label(kind: DefKind) -> Option<&'static str> {
    match kind {
        DefKind::Struct => Some("struct"),
        DefKind::Enum => Some("enum"),
        DefKind::TypeAlias => Some("type"),
        DefKind::Trait => Some("trait"),
        DefKind::Fn
        | DefKind::ExternFn
        | DefKind::Const
        | DefKind::Var
        | DefKind::Param
        | DefKind::Local
        | DefKind::EnumVariant
        | DefKind::StructField
        | DefKind::Impl
        | DefKind::GenericParam
        | DefKind::Closure
        | DefKind::TraitAssocType => None,
    }
}

#[cfg(test)]
mod tests {
    use phx_syntax::Interner;

    use super::*;
    use crate::resolver::{Def, DefKind};
    use crate::typeck::types::TypeInterner;

    #[test]
    fn diagnostic_format_prefixes_user_defined_types() {
        let mut types = TypeInterner::new();
        let mut names = Interner::new();
        let Ok(point) = names.intern("Point") else {
            panic!("intern Point");
        };
        let defs = vec![Def {
            kind: DefKind::Struct,
            name: point,
            span: phx_diagnostics::Span::new(0, 0),
            module: 0,
            exported: false,
            scope_depth: 0,
        }];
        let point_ty = types.intern(&Ty::Named {
            def: DefId::from_raw(0),
            args: vec![],
        });
        assert_eq!(
            format_type_diagnostic(&types, &names, &defs, point_ty),
            "struct Point"
        );
    }
}
