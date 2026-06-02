//! Type-checking for `Option` / `Result` enum constructor expressions.

use phx_diagnostics::{Span, TypeCheckBag, TypeCheckError};
use phx_syntax::Interner;
use phx_syntax::token::Keyword;

use super::builtins::{option_type, result_type};
use super::display::format_type;
use super::types::{Ty, TypeId, TypeInterner};
use crate::resolver::Def;

/// Context for checking an enum constructor when the expected type is known.
pub struct EnumCtorContext<'a> {
    /// Type interner.
    pub types: &'a mut TypeInterner,
    /// Error bag.
    pub bag: &'a mut TypeCheckBag,
    /// Name interner for diagnostics.
    pub interner: &'a Interner,
    /// Definition table for diagnostics.
    pub defs: &'a [Def],
    /// Expected type from context (`const x: Option<s32> = …`), if any.
    pub expected: Option<TypeId>,
}

/// Checks `Some`/`None`/`Ok`/`Err` when the inner expression was already type-checked.
#[must_use]
pub fn check_enum_ctor_with_inner_ty(
    ctx: &mut EnumCtorContext<'_>,
    variant: Keyword,
    inner_ty: Option<TypeId>,
    span: Span,
) -> TypeId {
    match variant {
        Keyword::None => check_none_ctor(ctx, span),
        Keyword::Some => {
            let Some(payload) = inner_ty else {
                ctx.bag.push(TypeCheckError::ArityMismatch {
                    expected: 1,
                    found: 0,
                    span,
                });
                return ctx.types.intern(&Ty::Unit);
            };
            let option = option_type(ctx.types, payload);
            if let Some(expected) = ctx.expected
                && !option_matches_expected(ctx.types, expected, option)
            {
                ctx.bag.push(TypeCheckError::Mismatch {
                    expected: format_type(ctx.types, ctx.interner, ctx.defs, expected),
                    found: format_type(ctx.types, ctx.interner, ctx.defs, option),
                    span,
                });
            }
            option
        }
        Keyword::Ok => check_result_arm(ctx, inner_ty, true, span),
        Keyword::Err => check_result_arm(ctx, inner_ty, false, span),
        _ => {
            ctx.bag.push(TypeCheckError::InvalidOperator {
                op: "enum ctor",
                span,
            });
            ctx.types.intern(&Ty::Unit)
        }
    }
}

fn check_none_ctor(ctx: &mut EnumCtorContext<'_>, span: Span) -> TypeId {
    let Some(expected) = ctx.expected else {
        ctx.bag.push(TypeCheckError::Mismatch {
            expected: "Option<T> (annotation required)".to_owned(),
            found: "None".to_owned(),
            span,
        });
        return ctx.types.intern(&Ty::Unit);
    };
    let Ty::Option(inner) = ctx.types.get(expected) else {
        ctx.bag.push(TypeCheckError::Mismatch {
            expected: format_type(ctx.types, ctx.interner, ctx.defs, expected),
            found: "None".to_owned(),
            span,
        });
        return expected;
    };
    option_type(ctx.types, *inner)
}

fn check_result_arm(
    ctx: &mut EnumCtorContext<'_>,
    inner_ty: Option<TypeId>,
    is_ok: bool,
    span: Span,
) -> TypeId {
    let Some(payload) = inner_ty else {
        ctx.bag.push(TypeCheckError::ArityMismatch {
            expected: 1,
            found: 0,
            span,
        });
        return ctx.types.intern(&Ty::Unit);
    };
    let (ok_ty, err_ty) = if let Some(expected) = ctx.expected {
        if let Ty::Result { ok, err } = ctx.types.get(expected) {
            (*ok, *err)
        } else {
            ctx.bag.push(TypeCheckError::Mismatch {
                expected: format_type(ctx.types, ctx.interner, ctx.defs, expected),
                found: format_type(ctx.types, ctx.interner, ctx.defs, payload),
                span,
            });
            (payload, ctx.types.intern(&Ty::Unit))
        }
    } else {
        (payload, ctx.types.intern(&Ty::Unit))
    };
    if is_ok {
        if ctx.expected.is_some() && payload != ok_ty {
            ctx.bag.push(TypeCheckError::Mismatch {
                expected: format_type(ctx.types, ctx.interner, ctx.defs, ok_ty),
                found: format_type(ctx.types, ctx.interner, ctx.defs, payload),
                span,
            });
        }
        result_type(ctx.types, ok_ty, err_ty)
    } else {
        if ctx.expected.is_some() && payload != err_ty {
            ctx.bag.push(TypeCheckError::Mismatch {
                expected: format_type(ctx.types, ctx.interner, ctx.defs, err_ty),
                found: format_type(ctx.types, ctx.interner, ctx.defs, payload),
                span,
            });
        }
        result_type(ctx.types, ok_ty, err_ty)
    }
}

fn option_matches_expected(types: &TypeInterner, expected: TypeId, option: TypeId) -> bool {
    matches!(
        (types.get(expected), types.get(option)),
        (Ty::Option(e), Ty::Option(o)) if e == o
    )
}
