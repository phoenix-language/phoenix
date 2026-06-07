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
pub mod attr_collect;
pub mod import_walk;
pub mod intern;
pub mod lexer;
pub mod parser;
pub mod source_file;
pub mod token;

pub use ast::{AstNodeId, Program};
pub use attr_collect::{
    DeprecatedMeta, allow_names_from_attrs, attr_named, deprecated_from_attrs,
    derive_from_bracket_attrs, function_bracket_attrs, has_must_use_attr, top_level_bracket_attrs,
};
pub use import_walk::all_imports;
pub use intern::{Interner, Symbol, closure_def_symbol, impl_receiver_symbol};
pub use lexer::{Lexer, lex};
pub use parser::{parse, parse_with_interner};
pub use phx_diagnostics::SymbolNames;
pub use phx_diagnostics::{LexError, ParseBag, ParseError, Span};
pub use source_file::SourceFile;
pub use token::{FloatSuffix, IntegerSuffix, Keyword, Token, TokenKind};
