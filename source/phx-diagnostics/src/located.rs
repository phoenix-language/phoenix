//! Errors tagged with the module id where they were emitted.
//!
//! Spans are byte offsets into that module's source buffer until `Span` carries a file id.

/// A diagnostic tied to a dense module index in the loaded program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedError<E> {
    /// Owning module id (matches `SourceModule::id` in the compiler resolver).
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
