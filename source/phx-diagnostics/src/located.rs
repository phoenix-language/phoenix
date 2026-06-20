//! Module-scoped errors for multi-file Phoenix crates.
//!
//! Phoenix programs may contain many source modules (path dependencies, `mod` trees). Most error
//! enums carry a [`crate::Span`] into **one** module's UTF-8 buffer but not **which** module that
//! buffer belongs to. [`LocatedError`] is the bridge: it pairs a dense module index with the
//! underlying pass error until spans carry file identity natively.
//!
//! ## Role in the pipeline
//!
//! Resolve, type-check, lower, and IR passes push into `*Bag` collections as
//! `LocatedError { module, error }`:
//!
//! 1. **Resolver** — [`crate::DiagnosticBag`] stores [`LocatedError<ResolveError>`].
//! 2. **Type-check** — [`crate::TypeCheckBag`] stores [`LocatedError<TypeCheckError>`].
//! 3. **Lower / IR** — [`crate::LowerBag`] and [`crate::IrBag`] follow the same pattern.
//!
//! When rendering, the compiler looks up `modules[module].source` and passes that buffer with the
//! error's inner span to [`crate::render_diagnostic_enriched`]. The `module` field matches
//! `SourceModule::id` in the compiler resolver (dense `u32`, not a path string).
//!
//! Single-file compilations may still use bare error enums; bags and [`LocatedError`] become
//! mandatory once multiple modules are loaded.

/// A diagnostic tied to the module that produced it.
///
/// Generic over the pass-specific error type (`ResolveError`, `TypeCheckError`, …). Formatters
/// pattern-match on `error` for message text and codes, then use `module` to select source text
/// for caret rendering.
///
/// # Examples
///
/// ```
/// use phx_diagnostics::{LocatedError, ResolveError, Span};
///
/// let located = LocatedError::new(
///     0,
///     ResolveError::UnresolvedIdent {
///         symbol_index: 42,
///         span: Span::new(10, 14),
///     },
/// );
/// assert_eq!(located.module, 0);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedError<E> {
    /// Dense module index for the owning source file.
    ///
    /// Matches the compiler resolver's `SourceModule::id` for the buffer that contains the inner
    /// error's spans.
    pub module: u32,
    /// The pass-specific error (lex, resolve, type-check, lower, or IR).
    pub error: E,
}

impl<E> LocatedError<E> {
    /// Pairs `module` with `error` for bag collection or immediate return.
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub const fn new(module: u32, error: E) -> Self {
        Self { module, error }
    }
}
