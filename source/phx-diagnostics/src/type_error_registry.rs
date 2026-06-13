//! Generated [`TypeCheckError`] metadata (`code`, `span`) from a single variant table.
//!
//! Add new variants here once; `code()` / `span()` and explain drift tests stay in sync.

use crate::Span;
use crate::TypeCheckError;
use crate::code::DiagnosticCode;

macro_rules! typecheck_error_registry {
    ($($variant:ident => $code:literal),* $(,)?) => {
        impl TypeCheckError {
            /// Stable diagnostic code for this error.
            #[must_use]
            pub const fn code(&self) -> DiagnosticCode {
                match self {
                    $( Self::$variant { .. } => DiagnosticCode::new($code), )*
                }
            }

            /// Returns the primary span for this error, if any.
            #[must_use]
            pub const fn span(&self) -> Option<Span> {
                match self {
                    $( Self::$variant { span, .. } => Some(*span), )*
                }
            }
        }

        #[cfg(test)]
        pub(crate) const TYPECHECK_ERROR_TABLE: &[(&str, &str)] =
            &[$( ($code, stringify!($variant)), )*];
    };
}

typecheck_error_registry! {
    Mismatch => "E2001",
    UnknownType => "E2002",
    ArityMismatch => "E2003",
    NotCallable => "E2004",
    UnresolvedMethod => "E2005",
    AmbiguousMethod => "E2006",
    NonUnifyingBranches => "E2007",
    NonExhaustiveMatch => "E2008",
    UnreachableMatchArm => "E2009",
    UnknownStructField => "E2010",
    MissingStructField => "E2011",
    UnknownEnumVariantField => "E2012",
    MissingEnumVariantField => "E2013",
    InvalidCast => "E2014",
    InvalidOperator => "E2015",
    UnsupportedFeature => "E2016",
    UseAfterMove => "E2017",
    MovedAssignTarget => "E2018",
    UnresolvedValue => "E2019",
    LoopControlOutsideLoop => "E2020",
    RecursiveTypeAlias => "E2021",
    ReturnEscapesLocal => "E2022",
    TraitNotSatisfied => "E2023",
    UnknownTraitBound => "E2030",
    InferenceFailed => "E2024",
    InferenceAmbiguous => "E2025",
    MissingTraitMethod => "E2026",
    MissingAssociatedType => "E2027",
    TryOutsideFunction => "E2028",
    InvalidTryOperand => "E2029",
    TryErrorFromMissing => "E2031",
    ExternCallRequiresUnsafe => "E2032",
    CopyableDropConflict => "E2033",
    IntrinsicRequiresUnsafe => "E2034",
    UnsafeFnCallRequiresUnsafe => "E2035",
    UnsafeTraitRequiresUnsafeImpl => "E2036",
    RedundantUnsafeInUnsafeTrait => "E2037",
    UnsafeImplOfSafeTrait => "E2038",
}

#[cfg(test)]
mod registry_tests {
    use super::TYPECHECK_ERROR_TABLE;
    use crate::render::explain_code;

    #[test]
    fn every_typecheck_code_has_explain_entry() {
        for (code, variant) in TYPECHECK_ERROR_TABLE {
            assert!(
                explain_code(code).is_some(),
                "missing phx explain entry for {code} ({variant})"
            );
        }
    }
}
