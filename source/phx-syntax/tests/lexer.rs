//! Exhaustive lexer integration tests — one case per [`TokenKind`] / [`LexError`] path.
//!
//! Crate: [`phx_syntax`] — entry [`phx_syntax::lex`].

#![allow(
    clippy::approx_constant,
    clippy::byte_char_slices,
    clippy::expect_used,
    clippy::uninlined_format_args
)]

use phx_diagnostics::{LexError, Span};
use phx_syntax::{FloatSuffix, IntegerSuffix, Keyword, Lexer, Token, TokenKind, lex};

// -----------------------------------------------------------------------------
// Test harness
// -----------------------------------------------------------------------------

mod support {
    use super::*;

    /// Expected token shape for table-driven assertions.
    #[derive(Debug, Clone)]
    pub enum Expect<'a> {
        Kw(Keyword),
        Ident(&'a str),
        TypeIdent(&'a str),
        Bool(bool),
        Int { value: i128, suffix: IntegerSuffix },
        Float { value: f64, suffix: FloatSuffix },
        ByteChar(u8),
        ByteString(&'a [u8]),
        String(&'a str),
        Kind(TokenKind<'a>),
    }

    pub fn tokens(source: &str) -> Vec<Token<'_>> {
        lex(source).expect("lex should succeed")
    }

    pub fn without_eof<'a>(tokens: &'a [Token<'a>]) -> &'a [Token<'a>] {
        let len = tokens
            .iter()
            .position(|t| t.kind == TokenKind::Eof)
            .unwrap_or(tokens.len());
        &tokens[..len]
    }

    pub fn assert_tokens(source: &str, expected: &[Expect<'_>]) {
        let all = tokens(source);
        let got = without_eof(&all);
        assert_eq!(
            got.len(),
            expected.len(),
            "token count mismatch for source {:?}",
            source
        );
        for (i, (token, exp)) in got.iter().zip(expected.iter()).enumerate() {
            assert_token_eq(source, i, token, exp);
        }
    }

    pub fn assert_token_eq(source: &str, index: usize, token: &Token<'_>, exp: &Expect<'_>) {
        let fail = |msg: &str| {
            panic!(
                "source {source:?} token[{index}] span={:?}: {msg}\n  got:  {:?}\n  want: {exp:?}",
                token.span, token.kind
            );
        };
        match (exp, &token.kind) {
            (Expect::Kw(k), TokenKind::Keyword(got)) if *k == *got => {}
            (Expect::Ident(w), TokenKind::Ident(got)) if *w == *got => {}
            (Expect::TypeIdent(w), TokenKind::TypeIdent(got)) if *w == *got => {}
            (Expect::Bool(w), TokenKind::Bool(got)) if *w == *got => {}
            (
                Expect::Int { value, suffix },
                TokenKind::Integer {
                    value: v,
                    suffix: s,
                },
            ) if *value == *v && *suffix == *s => {}
            (
                Expect::Float { value, suffix },
                TokenKind::Float {
                    value: v,
                    suffix: s,
                },
            ) if float_eq(*value, *v) && *suffix == *s => {}
            (Expect::ByteChar(w), TokenKind::ByteChar(got)) if *w == *got => {}
            (Expect::ByteString(w), TokenKind::ByteString(got)) if *w == got.as_slice() => {}
            (Expect::String(w), TokenKind::String(got)) if *w == got.as_str() => {}
            (Expect::Kind(k), got) if discriminant_matches(k, got) => {}
            _ => fail("kind mismatch"),
        }
    }

    fn float_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < f64::EPSILON || (a.is_nan() && b.is_nan())
    }

    fn discriminant_matches<'a>(exp: &TokenKind<'a>, got: &TokenKind<'a>) -> bool {
        std::mem::discriminant(exp) == std::mem::discriminant(got)
    }

    pub fn assert_lex_err(source: &str, check: fn(&LexError) -> bool) {
        let err = lex(source).expect_err("expected lex error");
        assert!(check(&err), "unexpected error {err} for source {source:?}");
    }

    pub fn span_text<'a>(source: &'a str, token: &Token<'a>) -> &'a str {
        &source[token.span.start as usize..token.span.end as usize]
    }
}

use support::{Expect, assert_lex_err, assert_tokens, span_text, tokens, without_eof};

// -----------------------------------------------------------------------------
// EOF and trivia
// -----------------------------------------------------------------------------

#[test]
fn eof_only_on_empty_input() {
    let all = tokens("");
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].kind, TokenKind::Eof);
}

#[test]
fn eof_span_at_source_end() {
    let source = "x";
    let all = tokens(source);
    let eof = all.last().expect("eof");
    assert_eq!(eof.kind, TokenKind::Eof);
    assert_eq!(eof.span, Span::new(1, 1));
}

#[test]
fn whitespace_variants_between_tokens() {
    assert_tokens(
        "a\tb\nc\r\rd",
        &[
            Expect::Ident("a"),
            Expect::Ident("b"),
            Expect::Ident("c"),
            Expect::Ident("d"),
        ],
    );
}

#[test]
fn line_comment_to_lf() {
    assert_tokens("// note\n+", &[Expect::Kind(TokenKind::Plus)]);
}

#[test]
fn line_comment_cr_only() {
    assert_tokens("// note\r-", &[Expect::Kind(TokenKind::Minus)]);
}

#[test]
fn line_comment_crlf() {
    assert_tokens("// note\r\n*", &[Expect::Kind(TokenKind::Star)]);
}

#[test]
fn line_comment_not_block() {
    assert_tokens(
        "// not block\n0",
        &[Expect::Int {
            value: 0,
            suffix: IntegerSuffix::None,
        }],
    );
}

#[test]
fn block_comment_basic() {
    assert_tokens("/// inner ///\n?", &[Expect::Kind(TokenKind::Question)]);
}

#[test]
fn block_comment_with_slashes_inside() {
    assert_tokens("/// // still comment /// x", &[Expect::Ident("x")]);
}

// -----------------------------------------------------------------------------
// Delimiters (every delimiter TokenKind)
// -----------------------------------------------------------------------------

#[test]
fn delimiter_comma() {
    assert_tokens(",", &[Expect::Kind(TokenKind::Comma)]);
}

#[test]
fn delimiter_semicolon() {
    assert_tokens(";", &[Expect::Kind(TokenKind::Semicolon)]);
}

#[test]
fn delimiter_dot() {
    assert_tokens(".", &[Expect::Kind(TokenKind::Dot)]);
}

#[test]
fn delimiter_colon() {
    assert_tokens(":", &[Expect::Kind(TokenKind::Colon)]);
}

#[test]
fn delimiter_colon_colon() {
    assert_tokens("::", &[Expect::Kind(TokenKind::ColonColon)]);
}

#[test]
fn delimiter_parens() {
    assert_tokens(
        "()",
        &[
            Expect::Kind(TokenKind::LParen),
            Expect::Kind(TokenKind::RParen),
        ],
    );
}

#[test]
fn delimiter_braces() {
    assert_tokens(
        "{}",
        &[
            Expect::Kind(TokenKind::LBrace),
            Expect::Kind(TokenKind::RBrace),
        ],
    );
}

#[test]
fn delimiter_brackets() {
    assert_tokens(
        "[]",
        &[
            Expect::Kind(TokenKind::LBracket),
            Expect::Kind(TokenKind::RBracket),
        ],
    );
}

// -----------------------------------------------------------------------------
// Operators (every operator TokenKind)
// -----------------------------------------------------------------------------

#[test]
fn operator_fat_arrow() {
    assert_tokens("=>", &[Expect::Kind(TokenKind::FatArrow)]);
}

#[test]
fn operator_dot_dot() {
    assert_tokens("..", &[Expect::Kind(TokenKind::DotDot)]);
}

#[test]
fn operator_dot_dot_eq() {
    assert_tokens("..=", &[Expect::Kind(TokenKind::DotDotEq)]);
}

#[test]
fn operator_plus() {
    assert_tokens("+", &[Expect::Kind(TokenKind::Plus)]);
}

#[test]
fn operator_minus() {
    assert_tokens("-", &[Expect::Kind(TokenKind::Minus)]);
}

#[test]
fn operator_star() {
    assert_tokens("*", &[Expect::Kind(TokenKind::Star)]);
}

#[test]
fn operator_slash() {
    assert_tokens("/", &[Expect::Kind(TokenKind::Slash)]);
}

#[test]
fn operator_percent() {
    assert_tokens("%", &[Expect::Kind(TokenKind::Percent)]);
}

#[test]
fn operator_star_star() {
    assert_tokens("**", &[Expect::Kind(TokenKind::StarStar)]);
}

#[test]
fn operator_and_and() {
    assert_tokens("&&", &[Expect::Kind(TokenKind::AndAnd)]);
}

#[test]
fn operator_or_or() {
    assert_tokens("||", &[Expect::Kind(TokenKind::OrOr)]);
}

#[test]
fn operator_bang() {
    assert_tokens("!", &[Expect::Kind(TokenKind::Bang)]);
}

#[test]
fn operator_amp() {
    assert_tokens("&", &[Expect::Kind(TokenKind::Amp)]);
}

#[test]
fn operator_pipe() {
    assert_tokens("|", &[Expect::Kind(TokenKind::Pipe)]);
}

#[test]
fn operator_caret() {
    assert_tokens("^", &[Expect::Kind(TokenKind::Caret)]);
}

#[test]
fn operator_tilde() {
    assert_tokens("~", &[Expect::Kind(TokenKind::Tilde)]);
}

#[test]
fn operator_shl() {
    assert_tokens("<<", &[Expect::Kind(TokenKind::Shl)]);
}

#[test]
fn operator_shr() {
    assert_tokens(">>", &[Expect::Kind(TokenKind::Shr)]);
}

#[test]
fn operator_eq_eq() {
    assert_tokens("==", &[Expect::Kind(TokenKind::EqEq)]);
}

#[test]
fn operator_ne() {
    assert_tokens("!=", &[Expect::Kind(TokenKind::Ne)]);
}

#[test]
fn operator_lt() {
    assert_tokens("<", &[Expect::Kind(TokenKind::Lt)]);
}

#[test]
fn operator_le() {
    assert_tokens("<=", &[Expect::Kind(TokenKind::Le)]);
}

#[test]
fn operator_gt() {
    assert_tokens(">", &[Expect::Kind(TokenKind::Gt)]);
}

#[test]
fn operator_ge() {
    assert_tokens(">=", &[Expect::Kind(TokenKind::Ge)]);
}

#[test]
fn operator_eq() {
    assert_tokens("=", &[Expect::Kind(TokenKind::Eq)]);
}

#[test]
fn operator_plus_eq() {
    assert_tokens("+=", &[Expect::Kind(TokenKind::PlusEq)]);
}

#[test]
fn operator_minus_eq() {
    assert_tokens("-=", &[Expect::Kind(TokenKind::MinusEq)]);
}

#[test]
fn operator_star_eq() {
    assert_tokens("*=", &[Expect::Kind(TokenKind::StarEq)]);
}

#[test]
fn operator_slash_eq() {
    assert_tokens("/=", &[Expect::Kind(TokenKind::SlashEq)]);
}

#[test]
fn operator_percent_eq() {
    assert_tokens("%=", &[Expect::Kind(TokenKind::PercentEq)]);
}

#[test]
fn operator_question() {
    assert_tokens("?", &[Expect::Kind(TokenKind::Question)]);
}

#[test]
fn operator_amp_mut() {
    assert_tokens("&mut", &[Expect::Kind(TokenKind::AmpMut)]);
}

#[test]
fn operator_star_mut() {
    assert_tokens("*mut", &[Expect::Kind(TokenKind::StarMut)]);
}

#[test]
fn operators_all_in_one_source() {
    let source = ":: => .. ..= && || == != <= >= << >> += -= *= /= %= ** &mut *mut";
    let expected: Vec<Expect<'_>> = vec![
        Expect::Kind(TokenKind::ColonColon),
        Expect::Kind(TokenKind::FatArrow),
        Expect::Kind(TokenKind::DotDot),
        Expect::Kind(TokenKind::DotDotEq),
        Expect::Kind(TokenKind::AndAnd),
        Expect::Kind(TokenKind::OrOr),
        Expect::Kind(TokenKind::EqEq),
        Expect::Kind(TokenKind::Ne),
        Expect::Kind(TokenKind::Le),
        Expect::Kind(TokenKind::Ge),
        Expect::Kind(TokenKind::Shl),
        Expect::Kind(TokenKind::Shr),
        Expect::Kind(TokenKind::PlusEq),
        Expect::Kind(TokenKind::MinusEq),
        Expect::Kind(TokenKind::StarEq),
        Expect::Kind(TokenKind::SlashEq),
        Expect::Kind(TokenKind::PercentEq),
        Expect::Kind(TokenKind::StarStar),
        Expect::Kind(TokenKind::AmpMut),
        Expect::Kind(TokenKind::StarMut),
    ];
    assert_tokens(source, &expected);
}

// -----------------------------------------------------------------------------
// Operator disambiguation (prefix vs longer token)
// -----------------------------------------------------------------------------

#[test]
fn disambig_colon_vs_colon_colon() {
    assert_tokens(
        ": :: :",
        &[
            Expect::Kind(TokenKind::Colon),
            Expect::Kind(TokenKind::ColonColon),
            Expect::Kind(TokenKind::Colon),
        ],
    );
}

#[test]
fn disambig_eq_vs_fat_arrow_vs_eq_eq() {
    assert_tokens(
        "= => ==",
        &[
            Expect::Kind(TokenKind::Eq),
            Expect::Kind(TokenKind::FatArrow),
            Expect::Kind(TokenKind::EqEq),
        ],
    );
}

#[test]
fn disambig_dot_vs_dot_dot_vs_dot_dot_eq() {
    assert_tokens(
        ". .. ..=",
        &[
            Expect::Kind(TokenKind::Dot),
            Expect::Kind(TokenKind::DotDot),
            Expect::Kind(TokenKind::DotDotEq),
        ],
    );
}

#[test]
fn disambig_bang_vs_ne() {
    assert_tokens(
        "! !=",
        &[Expect::Kind(TokenKind::Bang), Expect::Kind(TokenKind::Ne)],
    );
}

#[test]
fn disambig_amp_vs_and_and_vs_amp_mut() {
    assert_tokens(
        "& && &mut",
        &[
            Expect::Kind(TokenKind::Amp),
            Expect::Kind(TokenKind::AndAnd),
            Expect::Kind(TokenKind::AmpMut),
        ],
    );
}

#[test]
fn disambig_star_vs_star_star_vs_star_mut() {
    assert_tokens(
        "* ** *mut",
        &[
            Expect::Kind(TokenKind::Star),
            Expect::Kind(TokenKind::StarStar),
            Expect::Kind(TokenKind::StarMut),
        ],
    );
}

#[test]
fn disambig_lt_vs_shl_vs_le() {
    assert_tokens(
        "< << <=",
        &[
            Expect::Kind(TokenKind::Lt),
            Expect::Kind(TokenKind::Shl),
            Expect::Kind(TokenKind::Le),
        ],
    );
}

#[test]
fn disambig_gt_vs_shr_vs_ge() {
    assert_tokens(
        "> >> >=",
        &[
            Expect::Kind(TokenKind::Gt),
            Expect::Kind(TokenKind::Shr),
            Expect::Kind(TokenKind::Ge),
        ],
    );
}

#[test]
fn disambig_integer_range_vs_float() {
    assert_tokens(
        "3..5 3.14",
        &[
            Expect::Int {
                value: 3,
                suffix: IntegerSuffix::None,
            },
            Expect::Kind(TokenKind::DotDot),
            Expect::Int {
                value: 5,
                suffix: IntegerSuffix::None,
            },
            Expect::Float {
                value: 3.14,
                suffix: FloatSuffix::None,
            },
        ],
    );
}

// -----------------------------------------------------------------------------
// Keywords (every Keyword variant)
// -----------------------------------------------------------------------------

#[test]
fn keyword_const() {
    assert_tokens("const", &[Expect::Kw(Keyword::Const)]);
}

#[test]
fn keyword_var() {
    assert_tokens("var", &[Expect::Kw(Keyword::Var)]);
}

#[test]
fn keyword_if() {
    assert_tokens("if", &[Expect::Kw(Keyword::If)]);
}

#[test]
fn keyword_else() {
    assert_tokens("else", &[Expect::Kw(Keyword::Else)]);
}

#[test]
fn keyword_match() {
    assert_tokens("match", &[Expect::Kw(Keyword::Match)]);
}

#[test]
fn keyword_while() {
    assert_tokens("while", &[Expect::Kw(Keyword::While)]);
}

#[test]
fn keyword_for() {
    assert_tokens("for", &[Expect::Kw(Keyword::For)]);
}

#[test]
fn keyword_loop() {
    assert_tokens("loop", &[Expect::Kw(Keyword::Loop)]);
}

#[test]
fn keyword_break() {
    assert_tokens("break", &[Expect::Kw(Keyword::Break)]);
}

#[test]
fn keyword_continue() {
    assert_tokens("continue", &[Expect::Kw(Keyword::Continue)]);
}

#[test]
fn keyword_return() {
    assert_tokens("return", &[Expect::Kw(Keyword::Return)]);
}

#[test]
fn keyword_struct() {
    assert_tokens("struct", &[Expect::Kw(Keyword::Struct)]);
}

#[test]
fn keyword_enum() {
    assert_tokens("enum", &[Expect::Kw(Keyword::Enum)]);
}

#[test]
fn keyword_type() {
    assert_tokens("type", &[Expect::Kw(Keyword::Type)]);
}

#[test]
fn keyword_pub() {
    assert_tokens("pub", &[Expect::Kw(Keyword::Pub)]);
}

#[test]
fn keyword_trait() {
    assert_tokens("trait", &[Expect::Kw(Keyword::Trait)]);
}

#[test]
fn keyword_impl() {
    assert_tokens("impl", &[Expect::Kw(Keyword::Impl)]);
}

#[test]
fn keyword_as() {
    assert_tokens("as", &[Expect::Kw(Keyword::As)]);
}

#[test]
fn keyword_in() {
    assert_tokens("in", &[Expect::Kw(Keyword::In)]);
}

#[test]
fn keyword_mut() {
    assert_tokens("mut", &[Expect::Kw(Keyword::Mut)]);
}

#[test]
fn keyword_self_lower() {
    assert_tokens("self", &[Expect::Kw(Keyword::SelfLower)]);
}

#[test]
fn keyword_self_upper() {
    assert_tokens("Self", &[Expect::Kw(Keyword::SelfUpper)]);
}

#[test]
fn type_ident_some() {
    assert_tokens("Some", &[Expect::TypeIdent("Some")]);
}

#[test]
fn type_ident_none() {
    assert_tokens("None", &[Expect::TypeIdent("None")]);
}

#[test]
fn type_ident_ok() {
    assert_tokens("Ok", &[Expect::TypeIdent("Ok")]);
}

#[test]
fn type_ident_err() {
    assert_tokens("Err", &[Expect::TypeIdent("Err")]);
}

#[test]
fn keyword_bool() {
    assert_tokens("bool", &[Expect::Kw(Keyword::Bool)]);
}

#[test]
fn keyword_s8() {
    assert_tokens("s8", &[Expect::Kw(Keyword::S8)]);
}

#[test]
fn keyword_s16() {
    assert_tokens("s16", &[Expect::Kw(Keyword::S16)]);
}

#[test]
fn keyword_s32() {
    assert_tokens("s32", &[Expect::Kw(Keyword::S32)]);
}

#[test]
fn keyword_s64() {
    assert_tokens("s64", &[Expect::Kw(Keyword::S64)]);
}

#[test]
fn keyword_s128() {
    assert_tokens("s128", &[Expect::Kw(Keyword::S128)]);
}

#[test]
fn keyword_u8() {
    assert_tokens("u8", &[Expect::Kw(Keyword::U8)]);
}

#[test]
fn keyword_u16() {
    assert_tokens("u16", &[Expect::Kw(Keyword::U16)]);
}

#[test]
fn keyword_u32() {
    assert_tokens("u32", &[Expect::Kw(Keyword::U32)]);
}

#[test]
fn keyword_u64() {
    assert_tokens("u64", &[Expect::Kw(Keyword::U64)]);
}

#[test]
fn keyword_u128() {
    assert_tokens("u128", &[Expect::Kw(Keyword::U128)]);
}

#[test]
fn keyword_f32() {
    assert_tokens("f32", &[Expect::Kw(Keyword::F32)]);
}

#[test]
fn keyword_f64() {
    assert_tokens("f64", &[Expect::Kw(Keyword::F64)]);
}

#[test]
fn type_ident_option() {
    assert_tokens("Option", &[Expect::TypeIdent("Option")]);
}

#[test]
fn type_ident_result() {
    assert_tokens("Result", &[Expect::TypeIdent("Result")]);
}

#[test]
fn keyword_all_reserved_in_one_pass() {
    let source = Keyword::ALL
        .iter()
        .map(|kw| kw.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let all = tokens(&source);
    let got = without_eof(&all);
    assert_eq!(
        got.len(),
        Keyword::ALL.len(),
        "every Keyword variant should lex as one token"
    );
    for (token, kw) in got.iter().zip(Keyword::ALL) {
        assert_eq!(
            token.kind,
            TokenKind::Keyword(kw),
            "expected keyword {:?}, got {:?}",
            kw,
            token.kind
        );
    }
}

// -----------------------------------------------------------------------------
// Identifiers and booleans
// -----------------------------------------------------------------------------

#[test]
fn ident_snake() {
    assert_tokens(
        "main my_fn",
        &[Expect::Ident("main"), Expect::Ident("my_fn")],
    );
}

#[test]
fn ident_leading_underscore() {
    assert_tokens("_hidden", &[Expect::Ident("_hidden")]);
}

#[test]
fn ident_with_digits() {
    assert_tokens("foo_1_bar", &[Expect::Ident("foo_1_bar")]);
}

#[test]
fn type_ident_pascal() {
    assert_tokens(
        "Point Packet2",
        &[Expect::TypeIdent("Point"), Expect::TypeIdent("Packet2")],
    );
}

#[test]
fn bool_true_false() {
    assert_tokens("true false", &[Expect::Bool(true), Expect::Bool(false)]);
}

#[test]
fn main_is_ident_not_keyword() {
    assert_tokens("main", &[Expect::Ident("main")]);
}

#[test]
fn bool_is_keyword_not_byte_prefix() {
    assert_tokens("bool", &[Expect::Kw(Keyword::Bool)]);
}

#[test]
fn b_followed_by_ident_not_byte_literal() {
    assert_tokens("buffer", &[Expect::Ident("buffer")]);
}

// -----------------------------------------------------------------------------
// Integer literals
// -----------------------------------------------------------------------------

#[test]
fn int_decimal_zero() {
    assert_tokens(
        "0",
        &[Expect::Int {
            value: 0,
            suffix: IntegerSuffix::None,
        }],
    );
}

#[test]
fn int_decimal_nonzero() {
    assert_tokens(
        "42",
        &[Expect::Int {
            value: 42,
            suffix: IntegerSuffix::None,
        }],
    );
}

#[test]
fn int_decimal_underscores() {
    assert_tokens(
        "1_000_000",
        &[Expect::Int {
            value: 1_000_000,
            suffix: IntegerSuffix::None,
        }],
    );
}

#[test]
fn int_decimal_unsigned_suffix() {
    assert_tokens(
        "42u",
        &[Expect::Int {
            value: 42,
            suffix: IntegerSuffix::Unsigned,
        }],
    );
}

#[test]
fn int_decimal_zero_unsigned_suffix() {
    assert_tokens(
        "0u",
        &[Expect::Int {
            value: 0,
            suffix: IntegerSuffix::Unsigned,
        }],
    );
}

#[test]
fn int_hex_lower_prefix() {
    assert_tokens(
        "0xFF",
        &[Expect::Int {
            value: 255,
            suffix: IntegerSuffix::None,
        }],
    );
}

#[test]
fn int_hex_upper_prefix() {
    assert_tokens(
        "0XAB",
        &[Expect::Int {
            value: 171,
            suffix: IntegerSuffix::None,
        }],
    );
}

#[test]
fn int_hex_underscores() {
    assert_tokens(
        "0xFF_FF",
        &[Expect::Int {
            value: 0xFFFF,
            suffix: IntegerSuffix::None,
        }],
    );
}

#[test]
fn int_hex_unsigned_suffix() {
    assert_tokens(
        "0x10u",
        &[Expect::Int {
            value: 16,
            suffix: IntegerSuffix::Unsigned,
        }],
    );
}

#[test]
fn int_binary_lower_prefix() {
    assert_tokens(
        "0b1010",
        &[Expect::Int {
            value: 10,
            suffix: IntegerSuffix::None,
        }],
    );
}

#[test]
fn int_binary_upper_prefix() {
    assert_tokens(
        "0B11",
        &[Expect::Int {
            value: 3,
            suffix: IntegerSuffix::None,
        }],
    );
}

#[test]
fn int_binary_underscores() {
    assert_tokens(
        "0b10_10",
        &[Expect::Int {
            value: 10,
            suffix: IntegerSuffix::None,
        }],
    );
}

#[test]
fn int_binary_unsigned_suffix() {
    assert_tokens(
        "0b11u",
        &[Expect::Int {
            value: 3,
            suffix: IntegerSuffix::Unsigned,
        }],
    );
}

#[test]
fn int_i128_max() {
    let max = i128::MAX.to_string();
    assert_tokens(
        &max,
        &[Expect::Int {
            value: i128::MAX,
            suffix: IntegerSuffix::None,
        }],
    );
}

#[test]
fn int_decimal_leading_zero_not_hex() {
    assert_tokens(
        "0 0x1",
        &[
            Expect::Int {
                value: 0,
                suffix: IntegerSuffix::None,
            },
            Expect::Int {
                value: 1,
                suffix: IntegerSuffix::None,
            },
        ],
    );
}

// -----------------------------------------------------------------------------
// Float literals
// -----------------------------------------------------------------------------

#[test]
fn float_decimal_basic() {
    assert_tokens(
        "3.14",
        &[Expect::Float {
            value: 3.14,
            suffix: FloatSuffix::None,
        }],
    );
}

#[test]
fn float_decimal_underscores() {
    assert_tokens(
        "1_000.5",
        &[Expect::Float {
            value: 1000.5,
            suffix: FloatSuffix::None,
        }],
    );
}

#[test]
fn float_exponent_lower_e() {
    assert_tokens(
        "1e10",
        &[Expect::Float {
            value: 1e10,
            suffix: FloatSuffix::None,
        }],
    );
}

#[test]
fn float_exponent_upper_e() {
    assert_tokens(
        "1E10",
        &[Expect::Float {
            value: 1e10,
            suffix: FloatSuffix::None,
        }],
    );
}

#[test]
fn float_exponent_signed() {
    assert_tokens(
        "2.5e-3 1e+2",
        &[
            Expect::Float {
                value: 0.0025,
                suffix: FloatSuffix::None,
            },
            Expect::Float {
                value: 100.0,
                suffix: FloatSuffix::None,
            },
        ],
    );
}

#[test]
fn float_suffix_f32() {
    assert_tokens(
        "1.0f32",
        &[Expect::Float {
            value: 1.0,
            suffix: FloatSuffix::F32,
        }],
    );
}

#[test]
fn float_suffix_f64() {
    assert_tokens(
        "2.5f64",
        &[Expect::Float {
            value: 2.5,
            suffix: FloatSuffix::F64,
        }],
    );
}

#[test]
fn float_dot_not_followed_by_digit_is_not_float() {
    assert_tokens(
        ". 1",
        &[
            Expect::Kind(TokenKind::Dot),
            Expect::Int {
                value: 1,
                suffix: IntegerSuffix::None,
            },
        ],
    );
}

// -----------------------------------------------------------------------------
// Byte literals and escapes
// -----------------------------------------------------------------------------

#[test]
fn byte_char_ascii() {
    assert_tokens("b'a'", &[Expect::ByteChar(b'a')]);
}

#[test]
fn byte_char_escape_n() {
    assert_tokens("b'\\n'", &[Expect::ByteChar(b'\n')]);
}

#[test]
fn byte_char_escape_r() {
    assert_tokens("b'\\r'", &[Expect::ByteChar(b'\r')]);
}

#[test]
fn byte_char_escape_t() {
    assert_tokens("b'\\t'", &[Expect::ByteChar(b'\t')]);
}

#[test]
fn byte_char_escape_zero() {
    assert_tokens("b'\\0'", &[Expect::ByteChar(0)]);
}

#[test]
fn byte_char_escape_backslash() {
    assert_tokens("b'\\\\'", &[Expect::ByteChar(b'\\')]);
}

#[test]
fn byte_char_escape_single_quote() {
    assert_tokens("b'\\''", &[Expect::ByteChar(b'\'')]);
}

#[test]
fn byte_char_escape_double_quote() {
    assert_tokens("b'\\\"'", &[Expect::ByteChar(b'"')]);
}

#[test]
fn byte_char_escape_hex() {
    assert_tokens("b'\\x41'", &[Expect::ByteChar(b'A')]);
}

#[test]
fn byte_string_empty() {
    assert_tokens("b\"\"", &[Expect::ByteString(&[])]);
}

#[test]
fn byte_string_bytes() {
    assert_tokens("b\"ABC\"", &[Expect::ByteString(b"ABC")]);
}

#[test]
fn byte_string_escape_newline() {
    assert_tokens("b\"\\n\"", &[Expect::ByteString(b"\n")]);
}

#[test]
fn byte_string_escape_hex() {
    assert_tokens("b\"A\\x0A\"", &[Expect::ByteString(b"A\n")]);
}

#[test]
fn byte_string_all_escapes() {
    let source = "b\"\\n\\r\\t\\0\\\\\\\"\\'\\x42\"";
    assert_tokens(
        source,
        &[Expect::ByteString(&[
            b'\n', b'\r', b'\t', 0, b'\\', b'"', b'\'', b'B',
        ])],
    );
}

#[test]
fn string_empty() {
    assert_tokens("\"\"", &[Expect::String("")]);
}

#[test]
fn string_ascii() {
    assert_tokens("\"hello\"", &[Expect::String("hello")]);
}

#[test]
fn string_escape_newline() {
    assert_tokens("\"\\n\"", &[Expect::String("\n")]);
}

#[test]
fn string_utf8() {
    assert_tokens("\"héllo\"", &[Expect::String("héllo")]);
}

#[test]
fn keyword_str_type() {
    assert_tokens("str", &[Expect::Kw(Keyword::Str)]);
}

// -----------------------------------------------------------------------------
// Compile-time and runtime directives
// -----------------------------------------------------------------------------

#[test]
fn directive_hash_import() {
    assert_tokens("#import", &[Expect::Kind(TokenKind::HashImport)]);
}

#[test]
fn directive_hash_unsafe_removed() {
    assert_lex_err("#unsafe", |e| matches!(e, LexError::UnexpectedChar { .. }));
}

#[test]
fn directive_hash_inline() {
    assert_tokens("#inline", &[Expect::Kind(TokenKind::HashInline)]);
}

#[test]
fn directive_hash_cold() {
    assert_tokens("#cold", &[Expect::Kind(TokenKind::HashCold)]);
}

#[test]
fn directive_hash_hot() {
    assert_tokens("#hot", &[Expect::Kind(TokenKind::HashHot)]);
}

#[test]
fn directive_hash_derive() {
    assert_tokens("#derive", &[Expect::Kind(TokenKind::HashDerive)]);
}

#[test]
fn hash_bracket_attribute_token() {
    assert_tokens(
        "#[cfg(target_os = \"linux\")]",
        &[
            Expect::Kind(TokenKind::HashBracket),
            Expect::Ident("cfg"),
            Expect::Kind(TokenKind::LParen),
            Expect::Ident("target_os"),
            Expect::Kind(TokenKind::Eq),
            Expect::String("linux"),
            Expect::Kind(TokenKind::RParen),
            Expect::Kind(TokenKind::RBracket),
        ],
    );
}

#[test]
fn directive_at_spawn() {
    assert_tokens("@spawn", &[Expect::Kind(TokenKind::AtSpawn)]);
}

#[test]
fn directive_at_send() {
    assert_tokens("@send", &[Expect::Kind(TokenKind::AtSend)]);
}

#[test]
fn directive_at_receive() {
    assert_tokens("@receive", &[Expect::Kind(TokenKind::AtReceive)]);
}

#[test]
fn directive_at_reply() {
    assert_tokens("@reply", &[Expect::Kind(TokenKind::AtReply)]);
}

#[test]
fn directive_import_path() {
    assert_tokens(
        "#import core::mem;",
        &[
            Expect::Kind(TokenKind::HashImport),
            Expect::Ident("core"),
            Expect::Kind(TokenKind::ColonColon),
            Expect::Ident("mem"),
            Expect::Kind(TokenKind::Semicolon),
        ],
    );
}

// -----------------------------------------------------------------------------
// Lex errors (every LexError variant)
// -----------------------------------------------------------------------------

#[test]
fn error_unterminated_byte_string() {
    assert_lex_err("b\"open", |e| {
        matches!(e, LexError::UnterminatedString { .. })
    });
}

#[test]
fn error_unterminated_byte_char() {
    assert_lex_err("b'", |e| matches!(e, LexError::UnterminatedString { .. }));
}

#[test]
fn error_unterminated_block_comment() {
    assert_lex_err("/// no end", |e| {
        matches!(e, LexError::UnterminatedBlockComment { .. })
    });
}

#[test]
fn error_invalid_escape_byte_char() {
    assert_lex_err(r"b'\z'", |e| matches!(e, LexError::InvalidEscape { .. }));
}

#[test]
fn error_invalid_escape_byte_string() {
    assert_lex_err(r#"b"\q""#, |e| matches!(e, LexError::InvalidEscape { .. }));
}

#[test]
fn error_invalid_escape_hex_incomplete() {
    assert_lex_err(r"b'\x1'", |e| matches!(e, LexError::InvalidEscape { .. }));
}

#[test]
fn error_invalid_escape_hex_non_digit() {
    assert_lex_err(r"b'\xGH'", |e| matches!(e, LexError::InvalidEscape { .. }));
}

#[test]
fn error_unexpected_char_dollar() {
    assert_lex_err("$", |e| {
        matches!(e, LexError::UnexpectedChar { ch: '$', offset: 0 })
    });
}

#[test]
fn error_unexpected_hash_unknown() {
    assert_lex_err("#unknown", |e| matches!(e, LexError::UnexpectedChar { .. }));
}

#[test]
fn error_unexpected_at_unknown() {
    assert_lex_err("@unknown", |e| matches!(e, LexError::UnexpectedChar { .. }));
}

#[test]
fn error_unexpected_at_bare() {
    assert_lex_err("@", |e| matches!(e, LexError::UnexpectedChar { .. }));
}

#[test]
fn error_integer_overflow() {
    let too_big = format!("{}0", i128::MAX);
    assert_lex_err(&too_big, |e| matches!(e, LexError::IntegerOverflow { .. }));
}

#[test]
fn error_invalid_float_hex_no_digits() {
    assert_lex_err("0x", |e| matches!(e, LexError::InvalidInt { .. }));
}

#[test]
fn error_invalid_float_binary_no_digits() {
    assert_lex_err("0b", |e| matches!(e, LexError::InvalidInt { .. }));
}

#[test]
fn error_invalid_float_exponent_without_digits() {
    assert_lex_err("1e", |e| matches!(e, LexError::InvalidFloat { .. }));
}

#[test]
fn error_invalid_float_exponent_without_mantissa_digits() {
    assert_lex_err("1e+", |e| matches!(e, LexError::InvalidFloat { .. }));
}

#[test]
fn error_lexeme_too_long() {
    let huge = "a".repeat(1_048_577);
    assert_lex_err(&huge, |e| matches!(e, LexError::LexemeTooLong { .. }));
}

// -----------------------------------------------------------------------------
// Spans and API invariants
// -----------------------------------------------------------------------------

#[test]
fn spans_cover_source_text() {
    let source = "fn::()";
    let all = tokens(source);
    let got = without_eof(&all);
    for token in got {
        let text = span_text(source, token);
        assert!(!text.is_empty() || matches!(token.kind, TokenKind::Eof));
    }
}

#[test]
fn spans_monotonic_non_overlapping() {
    let source = "a + b * c";
    let all = tokens(source);
    let got = without_eof(&all);
    for w in got.windows(2) {
        assert!(w[0].span.end <= w[1].span.start);
    }
}

#[test]
fn lexer_next_token_matches_lex_helper() {
    let source = "const n: s32 = 0;";
    let from_lex = tokens(source);
    let mut lexer = Lexer::new(source);
    let mut from_next = Vec::new();
    loop {
        let t = lexer.next_token().expect("token");
        let done = t.kind == TokenKind::Eof;
        from_next.push(t);
        if done {
            break;
        }
    }
    assert_eq!(from_lex, from_next);
}

#[test]
fn real_world_main_signature() {
    assert_tokens(
        "main :: () => { }",
        &[
            Expect::Ident("main"),
            Expect::Kind(TokenKind::ColonColon),
            Expect::Kind(TokenKind::LParen),
            Expect::Kind(TokenKind::RParen),
            Expect::Kind(TokenKind::FatArrow),
            Expect::Kind(TokenKind::LBrace),
            Expect::Kind(TokenKind::RBrace),
        ],
    );
}

#[test]
fn real_world_const_and_call() {
    assert_tokens(
        "const x = 0; foo()",
        &[
            Expect::Kw(Keyword::Const),
            Expect::Ident("x"),
            Expect::Kind(TokenKind::Eq),
            Expect::Int {
                value: 0,
                suffix: IntegerSuffix::None,
            },
            Expect::Kind(TokenKind::Semicolon),
            Expect::Ident("foo"),
            Expect::Kind(TokenKind::LParen),
            Expect::Kind(TokenKind::RParen),
        ],
    );
}
