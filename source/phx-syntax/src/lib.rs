//! Phoenix syntax — lexer, parser, and AST.

pub mod lexer;
pub mod token;

pub use lexer::{Lexer, lex};
pub use phx_diagnostics::{LexError, Span};
pub use token::{FloatSuffix, IntegerSuffix, Keyword, Token, TokenKind};
