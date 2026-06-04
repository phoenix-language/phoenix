//! Errors tagged with the module id where they were emitted.
//!
//! Spans are byte offsets into that module's source buffer until `Span` carries a file id.

/// A diagnostic tied to a dense module index in the crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedError<E> {
    /// Owning module id (matches [`phx_compiler::resolver::SourceModule::id`]).
    pub module: u32,
    /// The underlying error.
    pub error: E,
}

impl<E> LocatedError<E> {
    /// Creates a located error.
    #[must_use]
    pub const fn new(module: u32, error: E) -> Self {
        Self { module, error }
    }
}
