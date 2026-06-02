//! Phoenix syntax — lexer, parser, and AST.

pub mod ast;
pub mod intern;
pub mod lexer;
pub mod parser;
pub mod token;

pub use ast::Program;
pub use intern::{Interner, Symbol};
pub use lexer::{Lexer, lex};
pub use parser::parse;
pub use phx_diagnostics::{LexError, ParseError, Span};
pub use token::{FloatSuffix, IntegerSuffix, Keyword, Token, TokenKind};
