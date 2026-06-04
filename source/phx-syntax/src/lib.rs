//! Phoenix syntax — lexer, parser, and AST.
//!
//! Typical use: [`lex`] → [`parse`] → [`SourceFile`] (program + [`Interner`]) for later passes.
//!
//! ## Modules
//!
//! - [`ast`] — untyped syntax tree nodes (declarations, expressions, types, patterns).
//! - [`token`] — lexical token kinds and [`Keyword`]s.
//! - [`lexer`] — [`Lexer`] and [`lex`] over a source `&str`.
//! - [`parser`] — recursive-descent parser; public entry [`parse`].
//! - [`intern`] — [`Interner`] and [`Symbol`] for identifier deduplication.
//! - [`source_file`] — [`SourceFile`] bundles [`Program`] + interner after parse.

pub mod ast;
pub mod intern;
pub mod lexer;
pub mod parser;
pub mod source_file;
pub mod token;

pub use ast::Program;
pub use intern::{Interner, Symbol, impl_receiver_symbol};
pub use phx_diagnostics::SymbolNames;
pub use lexer::{Lexer, lex};
pub use parser::{parse, parse_with_interner};
pub use phx_diagnostics::{LexError, ParseBag, ParseError, Span};
pub use source_file::SourceFile;
pub use token::{FloatSuffix, IntegerSuffix, Keyword, Token, TokenKind};
