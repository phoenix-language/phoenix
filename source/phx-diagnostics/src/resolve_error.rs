//! Name-resolution failure types.
//!
//! Collected in [`DiagnosticBag`] during [`phx_compiler::unstable::resolve`] (imports, duplicates, `main`).

use core::fmt;

use crate::LocatedError;
use crate::Span;
use crate::code::DiagnosticCode;

/// Reason a `main` function fails the MVP entry contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum InvalidMainReason {
    /// `main` has one or more parameters.
    HasParameters,
    /// Return type is present and is not unit `()`.
    NonUnitReturn,
}

impl fmt::Display for InvalidMainReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HasParameters => f.write_str("main must have no parameters"),
            Self::NonUnitReturn => f.write_str("main must return unit type `()`"),
        }
    }
}

/// A resolve error produced while binding names in the AST.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResolveError {
    /// Value identifier not found in scope.
    UnresolvedIdent {
        /// Interned name (display via interner).
        symbol_index: u32,
        /// Use site span.
        span: Span,
    },
    /// Type name not found in scope.
    UnresolvedType {
        /// Interned name index.
        symbol_index: u32,
        /// Use site span.
        span: Span,
    },
    /// Name defined more than once in the same scope.
    DuplicateDefinition {
        /// Interned name index.
        symbol_index: u32,
        /// Span of the earlier definition.
        first_span: Span,
        /// Span of the duplicate.
        span: Span,
    },
    /// `#import` used without a module root (single-file `compile_source` / `phx check` with no `--module-src`).
    ImportNotSupported {
        /// Span of the import directive.
        span: Span,
    },
    /// Module file could not be found on disk.
    ModuleNotFound {
        /// Import or reference span.
        span: Span,
        /// Logical module path.
        path: String,
    },
    /// Failed to read a module file.
    ModuleIo {
        /// Related span.
        span: Span,
        /// Filesystem path.
        path: String,
        /// OS error message.
        message: String,
    },
    /// Module source failed to parse.
    ModuleParse {
        /// Related span.
        span: Span,
        /// Filesystem path.
        path: String,
        /// Parse error message.
        message: String,
    },
    /// Circular `#import` dependency.
    CircularImport {
        /// Span (entry or import).
        span: Span,
        /// Human-readable cycle description.
        cycle: String,
    },
    /// Imported symbol is not `pub`.
    ImportNotExported {
        /// Import span.
        span: Span,
        /// Symbol name.
        name: String,
    },
    /// Symbol not found in target module exports.
    ImportNotFound {
        /// Import span.
        span: Span,
        /// Symbol name.
        name: String,
        /// Target module path.
        module: String,
    },
    /// Duplicate name introduced by imports.
    DuplicateImport {
        /// Import span.
        span: Span,
        /// Conflicting symbol.
        name: String,
    },
    /// `main` defined outside the entry module.
    MainNotInEntry {
        /// `main` span.
        span: Span,
        /// Module where `main` was found.
        module: String,
    },
    /// No `main` function in the compilation unit.
    MissingMain {
        /// Hint span in the entry module (e.g. first top-level item).
        span: Span,
    },
    /// `main` is not allowed in a `lib` package.
    MainForbiddenInLib {
        /// `main` definition span.
        span: Span,
        /// Module path.
        module: String,
    },
    /// `main` exists but does not match the MVP signature.
    InvalidMainSignature {
        /// Span of the `main` function name or signature.
        span: Span,
        /// What failed.
        reason: InvalidMainReason,
    },
    /// Generic type parameter used as a value identifier.
    GenericParamInValue {
        /// Interned parameter name.
        symbol_index: u32,
        /// Use site span.
        span: Span,
    },
    /// Second `Type :: impl :: Trait` for the same type and trait.
    DuplicateTraitImpl {
        /// Span of the duplicate impl.
        span: Span,
        /// Span of the first impl.
        first_span: Span,
    },
    /// Invalid `#[cfg(...)]` attribute.
    InvalidCfg {
        /// Attribute span.
        span: Span,
        /// What failed.
        message: String,
    },
    /// `.phx` file on disk is not registered via `mod` in a parent module.
    OrphanModuleFile {
        /// Related span.
        span: Span,
        /// Filesystem path.
        path: String,
        /// How to fix.
        hint: String,
    },
    /// Both `name.phx` and `name/mod.phx` exist for the same module.
    AmbiguousModuleEntry {
        /// Related span.
        span: Span,
        /// Flat file path.
        flat: String,
        /// Directory `mod.phx` path.
        module_dir: String,
    },
    /// Directory contains `.phx` files but no `name.phx` / `name/mod.phx` entry.
    MissingModuleEntry {
        /// Related span.
        span: Span,
        /// Directory path.
        dir: String,
    },
    /// Import targets a private submodule.
    PrivateSubmodule {
        /// Import span.
        span: Span,
        /// Submodule logical path.
        path: String,
    },
    /// `reexport` without `pub`.
    ReexportRequiresPub {
        /// Declaration span.
        span: Span,
    },
    /// Definition table exceeded `u32::MAX` entries.
    ProgramTooLarge {
        /// Related source span.
        span: Span,
    },
}

impl ResolveError {
    /// Stable diagnostic code for this error.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        match self {
            Self::UnresolvedIdent { .. } => DiagnosticCode::new("E1001"),
            Self::UnresolvedType { .. } => DiagnosticCode::new("E1002"),
            Self::DuplicateDefinition { .. } => DiagnosticCode::new("E1003"),
            Self::ImportNotSupported { .. } => DiagnosticCode::new("E1004"),
            Self::ModuleNotFound { .. } => DiagnosticCode::new("E1005"),
            Self::ModuleIo { .. } => DiagnosticCode::new("E1006"),
            Self::ModuleParse { .. } => DiagnosticCode::new("E1007"),
            Self::CircularImport { .. } => DiagnosticCode::new("E1008"),
            Self::ImportNotExported { .. } => DiagnosticCode::new("E1009"),
            Self::ImportNotFound { .. } => DiagnosticCode::new("E1010"),
            Self::DuplicateImport { .. } => DiagnosticCode::new("E1011"),
            Self::MainNotInEntry { .. } => DiagnosticCode::new("E1012"),
            Self::MissingMain { .. } => DiagnosticCode::new("E1013"),
            Self::MainForbiddenInLib { .. } => DiagnosticCode::new("E1014"),
            Self::InvalidMainSignature { .. } => DiagnosticCode::new("E1015"),
            Self::GenericParamInValue { .. } => DiagnosticCode::new("E1016"),
            Self::DuplicateTraitImpl { .. } => DiagnosticCode::new("E1017"),
            Self::InvalidCfg { .. } => DiagnosticCode::new("E1018"),
            Self::OrphanModuleFile { .. } => DiagnosticCode::new("E1019"),
            Self::AmbiguousModuleEntry { .. } => DiagnosticCode::new("E1020"),
            Self::MissingModuleEntry { .. } => DiagnosticCode::new("E1021"),
            Self::PrivateSubmodule { .. } => DiagnosticCode::new("E1022"),
            Self::ReexportRequiresPub { .. } => DiagnosticCode::new("E1023"),
            Self::ProgramTooLarge { .. } => DiagnosticCode::new("E1024"),
        }
    }

    /// Returns the primary span for this error, if any.
    #[must_use]
    pub const fn span(&self) -> Option<Span> {
        match self {
            Self::UnresolvedIdent { span, .. }
            | Self::UnresolvedType { span, .. }
            | Self::DuplicateDefinition { span, .. }
            | Self::ImportNotSupported { span }
            | Self::ModuleNotFound { span, .. }
            | Self::ModuleIo { span, .. }
            | Self::ModuleParse { span, .. }
            | Self::CircularImport { span, .. }
            | Self::ImportNotExported { span, .. }
            | Self::ImportNotFound { span, .. }
            | Self::DuplicateImport { span, .. }
            | Self::MainNotInEntry { span, .. }
            | Self::InvalidMainSignature { span, .. }
            | Self::MainForbiddenInLib { span, .. }
            | Self::MissingMain { span, .. }
            | Self::GenericParamInValue { span, .. }
            | Self::DuplicateTraitImpl { span, .. }
            | Self::InvalidCfg { span, .. }
            | Self::OrphanModuleFile { span, .. }
            | Self::AmbiguousModuleEntry { span, .. }
            | Self::MissingModuleEntry { span, .. }
            | Self::PrivateSubmodule { span, .. }
            | Self::ReexportRequiresPub { span, .. }
            | Self::ProgramTooLarge { span, .. } => Some(*span),
        }
    }
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnresolvedIdent { symbol_index, .. } => {
                write!(f, "unresolved identifier (sym#{symbol_index})")
            }
            Self::UnresolvedType { symbol_index, .. } => {
                write!(f, "unresolved type (sym#{symbol_index})")
            }
            Self::DuplicateDefinition { symbol_index, .. } => {
                write!(f, "duplicate definition of sym#{symbol_index}")
            }
            Self::ImportNotSupported { .. } => {
                f.write_str(
                    "#import requires a module root: pass `--module-src <dir>` with `phx check`/`phx run`, use `phx check path/to/file.phx` (parent directory is the default module root), or run `phx build` from a project with `phoenix.toml`",
                )
            }
            Self::ModuleNotFound { path, .. } => write!(f, "module not found: `{path}`"),
            Self::ModuleIo { path, message, .. } => {
                write!(f, "failed to read module `{path}`: {message}")
            }
            Self::ModuleParse { path, message, .. } => {
                write!(f, "failed to parse module `{path}`: {message}")
            }
            Self::CircularImport { cycle, .. } => write!(f, "circular module import: {cycle}"),
            Self::ImportNotExported { name, .. } => {
                write!(
                    f,
                    "`{name}` is not exported (add `pub` or import something else)"
                )
            }
            Self::ImportNotFound { name, module, .. } => {
                write!(f, "symbol `{name}` not found in module `{module}`")
            }
            Self::DuplicateImport { name, .. } => {
                write!(f, "duplicate import: `{name}`")
            }
            Self::MainNotInEntry { module, .. } => {
                write!(
                    f,
                    "`main` must be defined in the entry module, not in `{module}`"
                )
            }
            Self::MissingMain { .. } => f.write_str("missing entry function `main`"),
            Self::MainForbiddenInLib { module, .. } => {
                write!(
                    f,
                    "`main` is not allowed in library package module `{module}`"
                )
            }
            Self::InvalidMainSignature { reason, .. } => {
                write!(f, "invalid `main` signature: {reason}")
            }
            Self::GenericParamInValue { symbol_index, .. } => {
                write!(f, "generic type parameter used as value (sym#{symbol_index})")
            }
            Self::DuplicateTraitImpl { .. } => {
                f.write_str("duplicate trait implementation for the same type and trait")
            }
            Self::InvalidCfg { message, .. } => write!(f, "invalid `#[cfg]`: {message}"),
            Self::OrphanModuleFile { path, hint, .. } => {
                write!(f, "orphan module file `{path}`: {hint}")
            }
            Self::AmbiguousModuleEntry { flat, module_dir, .. } => {
                write!(
                    f,
                    "ambiguous module entry: both `{flat}` and `{module_dir}` exist"
                )
            }
            Self::MissingModuleEntry { dir, .. } => {
                write!(
                    f,
                    "directory `{dir}` contains modules but has no `mod.phx` or sibling `.phx` entry"
                )
            }
            Self::PrivateSubmodule { path, .. } => {
                write!(f, "module `{path}` is private (use `pub mod` to export it)")
            }
            Self::ReexportRequiresPub { .. } => {
                f.write_str("`reexport` requires `pub`")
            }
            Self::ProgramTooLarge { .. } => {
                f.write_str("program too large (definition table exceeds limit)")
            }
        }
    }
}

impl std::error::Error for ResolveError {}

/// Result of a resolve pass that may collect multiple errors.
pub type ResolveResult<T> = Result<T, DiagnosticBag>;

/// Collected resolve diagnostics; resolution may continue after non-fatal errors.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DiagnosticBag {
    errors: Vec<LocatedError<ResolveError>>,
}

impl DiagnosticBag {
    /// Creates an empty bag.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records an error for `module`.
    pub fn push(&mut self, module: u32, error: ResolveError) {
        self.errors.push(LocatedError::new(module, error));
    }

    /// Records an already-located error (e.g. when merging sub-pass bags).
    pub fn push_located(&mut self, located: LocatedError<ResolveError>) {
        self.errors.push(located);
    }

    /// Returns `true` if any errors were recorded.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Borrows collected located errors.
    #[must_use]
    pub fn errors(&self) -> &[LocatedError<ResolveError>] {
        &self.errors
    }

    /// Consumes the bag and returns located errors.
    #[must_use]
    pub fn into_errors(self) -> Vec<LocatedError<ResolveError>> {
        self.errors
    }
}

impl fmt::Display for DiagnosticBag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, located) in self.errors.iter().enumerate() {
            if i > 0 {
                f.write_str("\n")?;
            }
            write!(f, "{}", located.error)?;
        }
        Ok(())
    }
}

impl std::error::Error for DiagnosticBag {}
