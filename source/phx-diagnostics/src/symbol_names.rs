//! Interned symbol resolution for human-readable diagnostics.
//!
//! Compiler passes store identifiers as dense `u32` indices into an interner, not spellings.
//! Error enums such as [`crate::TypeCheckError`] and [`crate::ResolveError`] reference names by
//! index; formatters need the original text for messages like `undefined name 'foo'`.
//!
//! ## Role in the pipeline
//!
//! `phx-diagnostics` stays independent of the compiler's interner implementation. Instead,
//! [`SymbolNames`] is a small trait implemented by the compiler (typically wrapping
//! `phx_compiler::Interner`) and threaded into enriched formatters:
//!
//! 1. **Error creation** — passes record `name: u32` (or similar) at the error site.
//! 2. **Formatting** — `format_*_styled` helpers accept `&dyn SymbolNames` (or a generic
//!    implementor) and call [`SymbolNames::symbol_name`] when building prose.
//! 3. **Fallback** — when an index is stale or out of range, formatters use a placeholder rather
//!    than panicking.
//!
//! This keeps diagnostic message logic in `phx-diagnostics` while spellings remain owned by the
//! compiler front end.
//!
//! [`crate::TypeCheckError`]: crate::TypeCheckError
//! [`crate::ResolveError`]: crate::ResolveError

/// Resolves interned symbol indices to source spellings for diagnostic messages.
///
/// Implemented by the compiler's interner wrapper and passed into formatters that print
/// identifier names. The trait is object-safe so callers can use `&dyn SymbolNames` at API
/// boundaries without exposing interner types from `phx-compiler`.
///
/// # Examples
///
/// ```
/// use phx_diagnostics::SymbolNames;
///
/// struct Fixture(&'static [(&'static str, u32)]);
///
/// impl SymbolNames for Fixture {
///     fn symbol_name(&self, symbol_index: u32) -> Option<&str> {
///         self.0
///             .iter()
///             .find(|(_, id)| *id == symbol_index)
///             .map(|(name, _)| *name)
///     }
/// }
///
/// let names = Fixture(&[("main", 1)]);
/// assert_eq!(names.symbol_name(1), Some("main"));
/// assert_eq!(names.symbol_name(99), None);
/// ```
pub trait SymbolNames {
    /// Returns the UTF-8 spelling for `symbol_index` when it is a valid interned identifier.
    ///
    /// Returns `None` when the index is unknown, out of range, or no longer present in the
    /// interner (for example after a failed partial compile). Callers must handle `None` and
    /// must not panic.
    fn symbol_name(&self, symbol_index: u32) -> Option<&str>;
}
