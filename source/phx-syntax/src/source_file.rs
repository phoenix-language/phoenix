//! Parsed source unit carrying the AST and interner.
//!
//! Does not own source text: [`Span`] offsets refer to the `&str` passed to [`crate::parse`].

use crate::ast::Program;
use crate::intern::Interner;

/// A parsed compilation unit: AST plus the interner used to build it.
///
/// `Span` offsets in the AST refer to the same source string passed to [`crate::parse`].
/// This type does not own the source text.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceFile {
    /// Parsed program.
    pub program: Program,
    /// Identifier interner for this parse.
    pub interner: Interner,
}

impl SourceFile {
    /// Creates a source file from `program` and `interner`.
    #[must_use]
    pub const fn new(program: Program, interner: Interner) -> Self {
        Self { program, interner }
    }
}
