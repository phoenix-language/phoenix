//! Phoenix syntax — lexer, parser, and AST.
//!
//! Front of the compiler pipeline. Call [`parse`] on source text to obtain a [`SourceFile`]
//! (untyped [`Program`] plus [`Interner`]) for resolver, type checker, and later passes.
//!
//! ## Typical pipeline
//!
//! [`lex`] → [`parse`] → [`SourceFile`] → resolver → typeck → lower → codegen
//!
//! Most embedders call only [`parse`] or [`parse_with_interner`]. Incremental lexing via
//! [`Lexer`] is available when a pass needs to interleave tokenization with other work.
//!
//! ## Modules
//!
//! - [`ast`] — untyped syntax tree nodes (declarations, expressions, types, patterns).
//! - [`token`] — lexical token kinds and [`Keyword`]s.
//! - [`lexer`] — [`Lexer`] and [`lex`] over a source `&str`.
//! - [`parser`] — recursive-descent parser; public entries [`parse`] and [`parse_with_interner`].
//! - [`intern`] — [`Interner`] and [`Symbol`] for identifier deduplication.
//! - [`source_file`] — [`SourceFile`] bundles [`Program`] + interner after parse.
//! - [`attr_collect`] — gather bracket `#[…]` attributes from AST nodes for later passes.
//! - [`import_walk`] — collect all `#import` directives (file scope and block scope).
//!
//! ## Invariants
//!
//! - **AST is untyped.** Types in [`ast::Type`] are surface syntax only; semantic types live in
//!   `phx-compiler` after type checking.
//! - **Spans index the parse `source` string.** [`SourceFile`] does not own source text.
//! - **Identifiers are interned.** AST name fields use [`Symbol`], not `String` (except literal
//!   and attribute string payloads).
//! - **Never panics on user input.** Lex and parse return structured errors in [`ParseResult`].

pub mod ast;
pub mod attr_collect;
pub mod import_walk;
pub mod intern;
pub mod lexer;
pub mod parser;
pub mod source_file;
pub mod token;

pub use ast::{AstNodeId, Program};
pub use attr_collect::{function_bracket_attrs, top_level_bracket_attrs};
pub use import_walk::all_imports;
pub use intern::{
    InternError, Interner, Symbol, closure_def_symbol, for_in_iter_symbol, impl_receiver_symbol,
    scratch_binding_symbol,
};
pub use lexer::{Lexer, lex};
pub use parser::{parse, parse_with_interner};
pub use phx_diagnostics::SymbolNames;
pub use phx_diagnostics::{LexError, ParseBag, ParseError, ParseResult, Span};
pub use source_file::SourceFile;
pub use token::{FloatSuffix, IntegerSuffix, Keyword, Token, TokenKind};
