//! Secondary notes and help text for type-check diagnostics.
//!
//! Variant codes live in [`crate::type_error_registry`]; add a match arm here when introducing
//! a new [`TypeCheckError`] variant that needs notes or help text.

use crate::Span;
use crate::SymbolNames;
use crate::TypeCheckError;

/// Secondary note with optional source span (rendered with a caret when present).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeCheckNote {
    /// Note body (shown after `= note:`).
    pub text: String,
    /// Optional related source span.
    pub span: Option<Span>,
}

/// Notes and suggestions appended after the primary diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TypeCheckAncillary {
    /// Context notes (binding site, move site, etc.).
    pub notes: Vec<TypeCheckNote>,
    /// Actionable suggestions (shown after `= help:`).
    pub helps: Vec<String>,
}

/// Builds notes and help text for `err`.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn typecheck_ancillary(names: &impl SymbolNames, err: &TypeCheckError) -> TypeCheckAncillary {
    let mut out = TypeCheckAncillary::default();
    match err {
        TypeCheckError::Mismatch {
            expected,
            found,
            kind,
            ..
        } => mismatch_ancillary(expected, found, kind, &mut out),
        TypeCheckError::InvalidCast { from, to, .. } => {
            out.helps.push(format!(
                "explicit casts between `{from}` and `{to}` are not allowed in MVP; \
                 use a supported cast target (numeric primitives, array→slice, str→[u8])"
            ));
        }
        TypeCheckError::ArityMismatch {
            expected, found, ..
        } => {
            if found < expected {
                out.helps.push(format!(
                    "pass {expected} argument(s); this call provides {found}"
                ));
            } else {
                out.helps.push(format!(
                    "remove extra argument(s); this function accepts {expected}"
                ));
            }
        }
        TypeCheckError::NotCallable { found, .. } => {
            out.helps.push(format!(
                "`{found}` is not a function type; check the callee expression or use method call syntax (`receiver.method()`)"
            ));
        }
        TypeCheckError::UnresolvedMethod {
            receiver,
            method_index,
            ..
        } => {
            let method = names.symbol_name(*method_index).unwrap_or("<?>");
            out.helps.push(format!(
                "implement `fn {method}(...)` on `{receiver}` or check the method name"
            ));
        }
        TypeCheckError::AmbiguousMethod {
            receiver,
            method_index,
            ..
        } => {
            let method = names.symbol_name(*method_index).unwrap_or("<?>");
            out.helps.push(format!(
                "disambiguate by using only one trait impl that provides `{method}` on `{receiver}`, or call the function directly"
            ));
        }
        TypeCheckError::NonUnifyingBranches { .. } => {
            out.helps.push(
                "make every branch return the same type, or add an explicit `expr as Type` cast on mismatched branches"
                    .to_owned(),
            );
        }
        TypeCheckError::NonExhaustiveMatch { missing, .. } => {
            if missing.is_empty() {
                out.helps.push(
                    "add a match arm for each enum variant, or use `_` to match any remaining cases"
                        .to_owned(),
                );
            } else {
                out.helps.push(format!(
                    "add match arm(s) for: {}, or use `_` to match any remaining cases",
                    missing.join(", ")
                ));
            }
        }
        TypeCheckError::UnreachableMatchArm { .. } => {
            out.helps
                .push("remove or reorder this arm so it can match before a broader arm".to_owned());
        }
        TypeCheckError::UnknownStructField { name, .. } => {
            out.helps.push(format!(
                "remove field `{name}` or check the struct definition for the correct field name"
            ));
        }
        TypeCheckError::MissingStructField { name, .. } => {
            out.helps
                .push(format!("add initializer for struct field `{name}`"));
        }
        TypeCheckError::UnknownEnumVariantField { name, .. } => {
            out.helps.push(format!(
                "remove field `{name}` or check the enum variant definition"
            ));
        }
        TypeCheckError::MissingEnumVariantField { name, .. } => {
            out.helps
                .push(format!("add initializer for enum variant field `{name}`"));
        }
        TypeCheckError::InvalidOperator { op, .. } => {
            out.helps.push(format!(
                "operator `{op}` requires compatible primitive numeric or `bool` operands in MVP"
            ));
        }
        TypeCheckError::UnsupportedFeature { feature, .. } => {
            out.helps.push(format!(
                "`{feature}` is not implemented in the MVP compiler yet"
            ));
        }
        TypeCheckError::CopyableDropConflict { .. } => {
            out.helps.push(
                "remove the `Copyable` impl or the `Drop` impl — types with custom cleanup cannot be bitwise-copied"
                    .to_owned(),
            );
        }
        TypeCheckError::UseAfterMove {
            name, move_span, ..
        } => {
            out.notes.push(TypeCheckNote {
                text: format!("value `{name}` was moved here"),
                span: Some(*move_span),
            });
            out.helps.push(format!(
                "use `{name}` only before it is moved, or bind a new value after the move"
            ));
        }
        TypeCheckError::MovedAssignTarget {
            name, move_span, ..
        } => {
            out.notes.push(TypeCheckNote {
                text: format!("value `{name}` was moved here"),
                span: Some(*move_span),
            });
            out.helps.push(
                "assign to a binding that has not been moved yet, or introduce a new `var` binding"
                    .to_owned(),
            );
        }
        TypeCheckError::UnresolvedValue { symbol_index, .. } => {
            let name = names.symbol_name(*symbol_index).unwrap_or("<?>");
            out.helps.push(format!(
                "declare `{name}` before use, or import it with `#import` if it lives in another module"
            ));
        }
        TypeCheckError::UnknownType { symbol_index, .. } => {
            let name = names.symbol_name(*symbol_index).unwrap_or("<?>");
            out.helps.push(format!(
                "define type `{name}` in this module, or `#import` the module that exports it"
            ));
        }
        TypeCheckError::LoopControlOutsideLoop { keyword, .. } => {
            out.helps.push(format!(
                "place `{keyword}` inside a `while`, `for`, or `loop` block"
            ));
        }
        TypeCheckError::RecursiveTypeAlias { .. } => {
            out.helps.push(
                "break the cycle by introducing a named struct or enum, or remove the circular alias chain"
                    .to_owned(),
            );
        }
        TypeCheckError::ReturnEscapesLocal { borrow_span, .. } => {
            out.notes.push(TypeCheckNote {
                text: "borrow of local created here".to_owned(),
                span: Some(*borrow_span),
            });
            out.helps.push(
                "return an owned value instead of a borrow, slice view, or `str` view of a local binding"
                    .to_owned(),
            );
        }
        TypeCheckError::TraitNotSatisfied {
            type_name,
            trait_name,
            ..
        } => {
            out.helps.push(format!(
                "implement `{trait_name}` for `{type_name}` with `Type :: impl :: {trait_name} {{ ... }}`"
            ));
        }
        TypeCheckError::UnknownTraitBound { trait_name, .. } => {
            out.helps.push(format!(
                "import the trait definition, e.g. `#import std::core::clone::{trait_name};`"
            ));
        }
        TypeCheckError::InferenceFailed { .. } => {
            out.helps.push(
                "provide explicit type arguments at the call site, e.g. `name :: <Type> (...)`"
                    .to_owned(),
            );
        }
        TypeCheckError::InferenceAmbiguous { .. } => {
            out.helps.push(
                "disambiguate by writing explicit type arguments: `name :: <Type> (...)`"
                    .to_owned(),
            );
        }
        TypeCheckError::MissingTraitMethod {
            type_name,
            trait_name,
            method_name,
            ..
        } => {
            out.helps.push(format!(
                "add `fn {method_name}(...)` to `{type_name} :: impl :: {trait_name}`"
            ));
        }
        TypeCheckError::MissingAssociatedType {
            type_name,
            trait_name,
            assoc_name,
            ..
        } => {
            out.helps.push(format!(
                "add `type {assoc_name} = ...;` to `{type_name} :: impl :: {trait_name}`"
            ));
        }
        TypeCheckError::TryOutsideFunction { .. } => {
            out.helps.push(
                "use `match` or `if` in `main`, or call a helper function that returns `Option` or `Result`"
                    .to_owned(),
            );
        }
        TypeCheckError::InvalidTryOperand { .. } => {
            out.helps.push(
                "`?` requires a std `Option` or `Result` value matching the enclosing return type"
                    .to_owned(),
            );
        }
        TypeCheckError::TryErrorFromMissing {
            err_in, err_out, ..
        } => {
            out.helps
                .push(format!("implement `From<{err_in}>` for `{err_out}`"));
        }
        TypeCheckError::ExternCallRequiresUnsafe { .. }
        | TypeCheckError::IntrinsicRequiresUnsafe { .. }
        | TypeCheckError::UnsafeFnCallRequiresUnsafe { .. } => {
            out.helps.push(String::from(
                "wrap the call in `unsafe { ... }` or declare the enclosing function as `unsafe`",
            ));
        }
        TypeCheckError::UnsafeTraitRequiresUnsafeImpl { .. } => {
            out.helps.push(String::from(
                "use `Type :: unsafe impl :: Trait { ... }` to implement an `unsafe trait`",
            ));
        }
        TypeCheckError::RedundantUnsafeInUnsafeTrait { .. } => {
            out.helps.push(String::from(
                "remove `unsafe` from the method — all methods in an `unsafe trait` are implicitly unsafe",
            ));
        }
        TypeCheckError::UnsafeImplOfSafeTrait { .. } => {
            out.helps.push(String::from(
                "use a normal `impl` block, or mark the trait as `unsafe trait` if all implementers must be unsafe",
            ));
        }
        TypeCheckError::DiscardedStdResult { .. } | TypeCheckError::DiscardedStdOption { .. } => {
            out.helps.push(String::from(
                "handle the value with `match`, `if const` / `if var`, or `?` inside a compatible return type",
            ));
        }
        TypeCheckError::LangItemReserved { .. } => {
            out.helps.push(String::from(
                "remove `#[lang_item(...)]`; only `std::` modules may declare language items",
            ));
        }
        TypeCheckError::LangItemDuplicate { previous_span, .. } => {
            out.notes.push(TypeCheckNote {
                text: "previous language item declaration".to_owned(),
                span: Some(*previous_span),
            });
        }
        TypeCheckError::LangItemInvalid { .. } => {
            out.helps.push(String::from(
                "use `#[lang_item(name = \"...\", kind = \"intrinsic\"|\"enum\"|\"trait\")]` on std definitions",
            ));
        }
        TypeCheckError::InternalError { .. } | TypeCheckError::ProgramTooLarge { .. } => {}
    }
    out
}

#[allow(clippy::too_many_lines)]
fn mismatch_ancillary(
    expected: &str,
    found: &str,
    kind: &crate::MismatchKind,
    out: &mut TypeCheckAncillary,
) {
    match kind {
        crate::MismatchKind::ConstBinding {
            name,
            annotation_span,
        } => {
            out.notes.push(TypeCheckNote {
                text: format!(
                    "expected {} due to type annotation on `const {name}`",
                    type_expectation_phrase(expected)
                ),
                span: Some(*annotation_span),
            });
            push_mismatch_binding_helps(out, expected, found, "const");
        }
        crate::MismatchKind::VarBinding {
            name,
            annotation_span,
        } => {
            out.notes.push(TypeCheckNote {
                text: format!(
                    "expected {} due to type annotation on `var {name}`",
                    type_expectation_phrase(expected)
                ),
                span: Some(*annotation_span),
            });
            push_mismatch_binding_helps(out, expected, found, "var");
        }
        crate::MismatchKind::Return => {
            out.notes.push(TypeCheckNote {
                text: format!(
                    "expected `{expected}` because of the enclosing function return type"
                ),
                span: None,
            });
            push_mismatch_expr_helps(out, expected, found);
        }
        crate::MismatchKind::FunctionBody => {
            out.notes.push(TypeCheckNote {
                text: format!(
                    "function body must produce `{expected}` to match the declared return type"
                ),
                span: None,
            });
            push_mismatch_expr_helps(out, expected, found);
        }
        crate::MismatchKind::Argument { index } => {
            let ordinal = index + 1;
            out.notes.push(TypeCheckNote {
                text: format!("argument {ordinal} must be `{expected}`"),
                span: None,
            });
            push_mismatch_expr_helps(out, expected, found);
        }
        crate::MismatchKind::Assign { name } => {
            if let Some(name) = name {
                out.notes.push(TypeCheckNote {
                    text: format!("cannot assign `{found}` to local `{name}` of type `{expected}`"),
                    span: None,
                });
            }
            push_mismatch_expr_helps(out, expected, found);
        }
        crate::MismatchKind::StructField { name } => {
            out.notes.push(TypeCheckNote {
                text: format!("struct field `{name}` has type `{expected}`"),
                span: None,
            });
            push_mismatch_expr_helps(out, expected, found);
        }
        crate::MismatchKind::EnumVariantField { name } => {
            out.notes.push(TypeCheckNote {
                text: format!("enum variant field `{name}` has type `{expected}`"),
                span: None,
            });
            push_mismatch_expr_helps(out, expected, found);
        }
        crate::MismatchKind::Condition => {
            out.notes.push(TypeCheckNote {
                text: "conditions must have type `Bool`".to_owned(),
                span: None,
            });
            out.helps.push(format!(
                "use a boolean expression, or compare with `==` / `!=` instead of `{found}`"
            ));
        }
        crate::MismatchKind::Expression => {
            if expected == "specialized receiver type" {
                out.notes.push(TypeCheckNote {
                    text: "generic method call requires a fully instantiated receiver type (e.g. `Box :: <s32>`)"
                        .to_owned(),
                    span: None,
                });
                out.helps.push(
                    "specialize the receiver with `:: <...>` before calling the method".to_owned(),
                );
            } else if expected == "enum" {
                out.helps.push(
                    "match only enum variants when the scrutinee has an enum type".to_owned(),
                );
            } else {
                push_mismatch_expr_helps(out, expected, found);
            }
        }
    }
}

fn push_mismatch_binding_helps(
    out: &mut TypeCheckAncillary,
    expected: &str,
    found: &str,
    binding: &str,
) {
    if numeric_cast_might_help(found, expected) {
        out.helps.push(format!(
            "use an explicit cast on the initializer (`... as {expected}`), or change the `{binding}` type to `{found}`"
        ));
    } else {
        out.helps.push(format!(
            "change the `{binding}` type annotation to `{found}`, or change the initializer to produce `{expected}`"
        ));
    }
    if binding == "const" {
        out.helps.push(
            "or remove the type annotation and let the initializer determine the type".to_owned(),
        );
    }
}

fn push_mismatch_expr_helps(out: &mut TypeCheckAncillary, expected: &str, found: &str) {
    if numeric_cast_might_help(found, expected) {
        out.helps
            .push(format!("use an explicit cast: `... as {expected}`"));
    } else if found != expected {
        out.helps.push(format!(
            "change the expression to produce `{expected}`, or adjust the expected type to `{found}`"
        ));
    }
}

fn numeric_cast_might_help(found: &str, expected: &str) -> bool {
    is_numeric_primitive_name(found) && is_numeric_primitive_name(expected) && found != expected
}

fn type_expectation_phrase(expected: &str) -> String {
    if expected.starts_with("struct ")
        || expected.starts_with("enum ")
        || expected.starts_with("type ")
        || expected.starts_with("trait ")
    {
        format!("`{expected}`")
    } else {
        format!("type `{expected}`")
    }
}

fn is_numeric_primitive_name(name: &str) -> bool {
    matches!(
        name,
        "S8" | "S16"
            | "S32"
            | "S64"
            | "S128"
            | "U8"
            | "U16"
            | "U32"
            | "U64"
            | "U128"
            | "F32"
            | "F64"
    )
}
