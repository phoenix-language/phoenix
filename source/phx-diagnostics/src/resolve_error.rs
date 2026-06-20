//! Name-resolution failure types for the Phoenix compiler.
//!
//! Collected in [`DiagnosticBag`] during [`phx_compiler::unstable::resolve`]. The resolve pass
//! binds identifiers and types, loads module files, validates imports and exports, and enforces
//! the MVP `main` entry contract. Errors are wrapped in [`LocatedError`] so multi-file crates
//! can attribute each failure to a module id before spans carry file paths in the CLI.
//!
//! ## Compiler pass
//!
//! Resolve follows parsing ([`ParseError`]) and precedes type checking ([`TypeCheckError`]).
//! Module loading may surface nested parse or I/O failures as [`ResolveError::ModuleParse`] or
//! [`ResolveError::ModuleIo`]; those variants embed the underlying message while keeping a
//! resolve-stage [`DiagnosticCode`].
//!
//! ## Diagnostic codes (E1001–E1024)
//!
//! | Code | Variant | Summary |
//! |------|---------|---------|
//! | E1001 | [`ResolveError::UnresolvedIdent`] | Value identifier not in scope |
//! | E1002 | [`ResolveError::UnresolvedType`] | Type name not in scope |
//! | E1003 | [`ResolveError::DuplicateDefinition`] | Name defined twice in the same scope |
//! | E1004 | [`ResolveError::ImportNotSupported`] | `#import` without a module root |
//! | E1005 | [`ResolveError::ModuleNotFound`] | Module file missing on disk |
//! | E1006 | [`ResolveError::ModuleIo`] | Failed to read a module file |
//! | E1007 | [`ResolveError::ModuleParse`] | Module source failed to parse |
//! | E1008 | [`ResolveError::CircularImport`] | Circular `#import` dependency |
//! | E1009 | [`ResolveError::ImportNotExported`] | Imported symbol is not `pub` |
//! | E1010 | [`ResolveError::ImportNotFound`] | Symbol missing from target module exports |
//! | E1011 | [`ResolveError::DuplicateImport`] | Conflicting import name |
//! | E1012 | [`ResolveError::MainNotInEntry`] | `main` defined outside the entry module |
//! | E1013 | [`ResolveError::MissingMain`] | No `main` in the compilation unit |
//! | E1014 | [`ResolveError::MainForbiddenInLib`] | `main` in a `lib` package |
//! | E1015 | [`ResolveError::InvalidMainSignature`] | `main` signature violates MVP contract |
//! | E1016 | [`ResolveError::GenericParamInValue`] | Generic type parameter used as a value |
//! | E1017 | [`ResolveError::DuplicateTraitImpl`] | Second `Type :: impl :: Trait` for same pair |
//! | E1018 | [`ResolveError::InvalidCfg`] | Malformed `#[cfg(...)]` attribute |
//! | E1019 | [`ResolveError::OrphanModuleFile`] | `.phx` file not registered via `mod` |
//! | E1020 | [`ResolveError::AmbiguousModuleEntry`] | Both `name.phx` and `name/mod.phx` exist |
//! | E1021 | [`ResolveError::MissingModuleEntry`] | Directory has modules but no entry file |
//! | E1022 | [`ResolveError::PrivateSubmodule`] | Import targets a private submodule |
//! | E1023 | [`ResolveError::ReexportRequiresPub`] | `reexport` without `pub` |
//! | E1024 | [`ResolveError::ProgramTooLarge`] | Definition table exceeded `u32::MAX` |
//!
//! ## Integration with [`crate::format`]
//!
//! - **Message text** — [`resolve_message`] resolves interned `symbol_index` fields through
//!   [`SymbolNames`] and mirrors user-facing prose for each variant.
//! - **Single error** — [`format_resolve_error`] / [`format_resolve_error_styled`] render
//!   Cargo-style output. [`ResolveError::DuplicateDefinition`] adds a secondary note at
//!   `first_span` via [`render_diagnostic_with_note`].
//! - **Module paths** — [`ResolveError::ModuleIo`] and [`ResolveError::ModuleParse`] override
//!   [`SpanContext::file_path`] with the filesystem path from the error.
//! - **Multiple errors** — [`DiagnosticBag`] errors are formatted individually and joined by
//!   callers (the CLI iterates [`DiagnosticBag::errors`]).
//!
//! [`SymbolNames`]: crate::SymbolNames
//! [`SpanContext::file_path`]: crate::render::SpanContext::file_path
//! [`render_diagnostic_with_note`]: crate::render::render_diagnostic_with_note

use core::fmt;

use crate::LocatedError;
use crate::Span;
use crate::code::DiagnosticCode;

/// Reason a `main` function fails the MVP entry contract.
///
/// Carried by [`ResolveError::InvalidMainSignature`]. Displayed as part of the invalid-signature
/// message via [`InvalidMainReason`]'s [`Display`] impl.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum InvalidMainReason {
    /// `main` has one or more parameters (MVP requires `fn main()`).
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
///
/// Each variant maps to a stable [`DiagnosticCode`] via [`ResolveError::code`] (E1001–E1024).
/// Identifier-bearing variants store a `symbol_index` into the compilation interner; formatters
/// resolve the index to a name through [`SymbolNames`] in [`resolve_message`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResolveError {
    /// Value identifier not found in the active scope chain.
    UnresolvedIdent {
        /// Interned name (display via [`SymbolNames`] in formatters).
        symbol_index: u32,
        /// Use site span.
        span: Span,
    },
    /// Type name not found in the active type scope.
    UnresolvedType {
        /// Interned name index.
        symbol_index: u32,
        /// Use site span.
        span: Span,
    },
    /// Name defined more than once in the same scope.
    ///
    /// Formatters attach a secondary note at `first_span` when rendering via
    /// [`crate::format::format_resolve_error_styled`].
    DuplicateDefinition {
        /// Interned name index.
        symbol_index: u32,
        /// Span of the earlier definition.
        first_span: Span,
        /// Span of the duplicate definition.
        span: Span,
    },
    /// `#import` used without a module root.
    ///
    /// Typical when running `phx check` on a single file without `--module-src` or a project
    /// `phoenix.toml` module layout.
    ImportNotSupported {
        /// Span of the import directive.
        span: Span,
    },
    /// Module file could not be found on disk for the given logical path.
    ModuleNotFound {
        /// Import or reference span in the requesting module.
        span: Span,
        /// Logical module path that was requested.
        path: String,
    },
    /// Failed to read a module file from the filesystem.
    ModuleIo {
        /// Related span (usually the import site).
        span: Span,
        /// Filesystem path that could not be read.
        path: String,
        /// OS error message from the read attempt.
        message: String,
    },
    /// Module source failed to parse after being loaded.
    ///
    /// The embedded `message` is the parse diagnostic text; the resolve code remains E1007.
    ModuleParse {
        /// Related span (usually the import site).
        span: Span,
        /// Filesystem path of the module that failed to parse.
        path: String,
        /// Parse error message from the nested parse pass.
        message: String,
    },
    /// Circular `#import` dependency detected while loading the module graph.
    CircularImport {
        /// Span of the import that closed the cycle (or the entry import).
        span: Span,
        /// Human-readable cycle description (for example `a -> b -> a`).
        cycle: String,
    },
    /// Imported symbol exists in the target module but is not exported (`pub`).
    ImportNotExported {
        /// Import span.
        span: Span,
        /// Symbol name as written in the import list.
        name: String,
    },
    /// Symbol not found among the target module's exports.
    ImportNotFound {
        /// Import span.
        span: Span,
        /// Symbol name as written in the import list.
        name: String,
        /// Target module logical path.
        module: String,
    },
    /// Import would introduce a name that already exists in the importer's scope.
    DuplicateImport {
        /// Import span.
        span: Span,
        /// Conflicting symbol name.
        name: String,
    },
    /// `main` is defined in a module other than the compilation entry module.
    MainNotInEntry {
        /// Span of the `main` function.
        span: Span,
        /// Module path where `main` was found.
        module: String,
    },
    /// No `main` function in the compilation unit (bin package entry contract).
    MissingMain {
        /// Hint span in the entry module (for example the first top-level item).
        span: Span,
    },
    /// `main` is not allowed in a library package (`phoenix.toml` `type = "lib"`).
    MainForbiddenInLib {
        /// Span of the `main` function definition.
        span: Span,
        /// Module path containing the forbidden `main`.
        module: String,
    },
    /// `main` exists but does not match the MVP signature (`fn main()` with unit return).
    InvalidMainSignature {
        /// Span of the `main` function name or signature.
        span: Span,
        /// Which part of the signature contract failed.
        reason: InvalidMainReason,
    },
    /// Generic type parameter used where a value identifier is required.
    GenericParamInValue {
        /// Interned generic parameter name.
        symbol_index: u32,
        /// Use site span.
        span: Span,
    },
    /// Second `Type :: impl :: Trait` block for the same type and trait pair.
    DuplicateTraitImpl {
        /// Span of the duplicate impl block.
        span: Span,
        /// Span of the first impl block for the same pair.
        first_span: Span,
    },
    /// Invalid `#[cfg(...)]` attribute expression or unsupported cfg key.
    InvalidCfg {
        /// Attribute span.
        span: Span,
        /// What failed (unsupported key, malformed syntax, etc.).
        message: String,
    },
    /// A `.phx` file on disk is not registered via `mod` in a parent module.
    OrphanModuleFile {
        /// Related span (often the parent's module root or import site).
        span: Span,
        /// Filesystem path of the orphan file.
        path: String,
        /// Suggested fix (for example `add mod name; in parent`).
        hint: String,
    },
    /// Both `name.phx` and `name/mod.phx` exist for the same logical module name.
    AmbiguousModuleEntry {
        /// Related span.
        span: Span,
        /// Path to the flat `name.phx` file.
        flat: String,
        /// Path to the directory-style `name/mod.phx` entry.
        module_dir: String,
    },
    /// A directory contains `.phx` submodule files but no `name.phx` or `name/mod.phx` entry.
    MissingModuleEntry {
        /// Related span.
        span: Span,
        /// Directory path missing an entry file.
        dir: String,
    },
    /// Import targets a submodule that was declared without `pub mod`.
    PrivateSubmodule {
        /// Import span.
        span: Span,
        /// Submodule logical path.
        path: String,
    },
    /// `reexport` declaration is missing a required `pub` modifier.
    ReexportRequiresPub {
        /// Declaration span.
        span: Span,
    },
    /// Definition or symbol table exceeded representable `u32` indices.
    ProgramTooLarge {
        /// Related source span when the limit was hit during insertion.
        span: Span,
    },
}

impl ResolveError {
    /// Stable diagnostic code for this error (E1001–E1024).
    ///
    /// # Panics
    ///
    /// Never panics.
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

    /// Returns the primary span for caret rendering, if any.
    ///
    /// For [`ResolveError::DuplicateDefinition`] and [`ResolveError::DuplicateTraitImpl`], this
    /// returns the duplicate site (`span`); the first-definition note uses `first_span` in
    /// [`crate::format::format_resolve_error_styled`].
    ///
    /// # Panics
    ///
    /// Never panics.
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
///
/// Success carries the resolved program; failure is a [`DiagnosticBag`] of located errors rather
/// than a single variant, so the resolver can report every binding and module issue in one pass.
pub type ResolveResult<T> = Result<T, DiagnosticBag>;

/// Collected resolve diagnostics; resolution may continue after non-fatal errors.
///
/// Each entry is a [`LocatedError`] pairing a module id with a [`ResolveError`]. Formatters iterate
/// [`DiagnosticBag::errors`] and call [`crate::format::format_resolve_error_styled`] per entry.
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
    ///
    /// The `module` id indexes into the compilation unit's module table so multi-file diagnostics
    /// can be attributed before file paths are attached in the CLI.
    pub fn push(&mut self, module: u32, error: ResolveError) {
        self.errors.push(LocatedError::new(module, error));
    }

    /// Records an already-located error (for example when merging sub-pass bags).
    pub fn push_located(&mut self, located: LocatedError<ResolveError>) {
        self.errors.push(located);
    }

    /// Returns `true` if any errors were recorded.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Borrows collected located errors in insertion order.
    #[must_use]
    pub fn errors(&self) -> &[LocatedError<ResolveError>] {
        &self.errors
    }

    /// Consumes the bag and returns the underlying located error vector.
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
