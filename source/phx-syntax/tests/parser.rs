//! Exhaustive parser integration tests — one case per MVP grammar / [`ParseError`] path.
//!
//! Crate: [`phx_syntax`] — entry [`phx_syntax::parse`].

#![allow(
    clippy::approx_constant,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::too_many_lines
)]

use phx_diagnostics::{LexError, ParseError};
use phx_syntax::ast::decl::{FnDirective, StructBody, TopLevelDecl, Variant};
use phx_syntax::ast::expr::{AssignOp, Expr};
use phx_syntax::ast::stmt::{BlockItem, Stmt};
use phx_syntax::ast::{BlockNode, Program};
use phx_syntax::parse;

// -----------------------------------------------------------------------------
// Test harness
// -----------------------------------------------------------------------------

mod support {
    use super::*;

    /// Wrap statements inside `main` (each statement should include its own `;` where required).
    pub fn in_main(body: &str) -> String {
        format!("main :: () => {{ {body} }};")
    }

    /// Wrap a single expression as `const _v = expr;` inside `main`.
    pub fn in_main_expr(expr: &str) -> String {
        in_main(&format!("const _v = {expr};"))
    }

    /// Program with a type alias plus empty `main`.
    pub fn with_type_alias(ty: &str) -> String {
        format!("type Alias = {ty}; main :: () => {{ }};")
    }

    pub fn parse_ok(source: &str) -> Program {
        parse(source)
            .map(|sf| sf.program)
            .unwrap_or_else(|e| panic!("expected parse ok for:\n{source}\nerror: {e}"))
    }

    pub fn parse_err(source: &str) -> ParseError {
        parse(source).expect_err("expected parse error")
    }

    pub fn assert_ok(source: &str) {
        let _ = parse_ok(source);
    }

    pub fn assert_unsupported(source: &str, feature: &str) {
        match parse_err(source) {
            ParseError::UnsupportedSyntax { feature: f, .. } => {
                assert_eq!(f, feature, "source: {source:?}");
            }
            other => panic!("expected UnsupportedSyntax({feature}) for {source:?}, got {other:?}"),
        }
    }

    pub fn assert_parse_err(source: &str, check: fn(&ParseError) -> bool) {
        let err = parse_err(source);
        assert!(
            check(&err),
            "unexpected error {err:?} for source {source:?}"
        );
    }

    pub fn main_fn(program: &Program) -> &phx_syntax::ast::decl::Function {
        let TopLevelDecl::Function(f) = &program.items.last().expect("item").inner.decl else {
            panic!("expected trailing function");
        };
        f
    }

    pub fn first_stmt_expr<'a>(block: &'a BlockNode) -> &'a Expr {
        match &block.inner.items[0] {
            BlockItem::Stmt(Stmt::Const { init, .. }) | BlockItem::Stmt(Stmt::Expr(init)) => {
                &init.inner
            }
            BlockItem::Expr(e) => &e.inner,
            other => panic!("expected expr item, got {other:?}"),
        }
    }
}

use support::{
    assert_ok, assert_parse_err, assert_unsupported, in_main, in_main_expr, main_fn, parse_ok,
    with_type_alias,
};

// -----------------------------------------------------------------------------
// Program and imports
// -----------------------------------------------------------------------------

#[test]
fn program_empty() {
    let p = parse_ok("");
    assert!(p.imports.is_empty());
    assert!(p.items.is_empty());
}

#[test]
fn program_import_only() {
    let p = parse_ok("#import core::mem;");
    assert_eq!(p.imports.len(), 1);
    assert!(p.items.is_empty());
}

#[test]
fn program_import_path_segments() {
    assert_ok("#import std::io::fs;");
}

#[test]
fn program_import_brace_single() {
    assert_ok("#import m::{ foo }; main :: () => { };");
}

#[test]
fn program_import_brace_multiple() {
    assert_ok("#import m::{ a, b, c }; main :: () => { };");
}

#[test]
fn program_import_brace_glob() {
    assert_ok("#import m::{ * }; main :: () => { };");
}

#[test]
fn program_import_brace_empty() {
    assert_ok("#import m::{ }; main :: () => { };");
}

#[test]
fn program_multiple_imports() {
    let p = parse_ok("#import a; #import b::c; main :: () => { };");
    assert_eq!(p.imports.len(), 2);
}

#[test]
fn program_pub_top_level() {
    assert_ok("pub main :: () => { };");
}

// -----------------------------------------------------------------------------
// Struct declarations
// -----------------------------------------------------------------------------

#[test]
fn decl_struct_unit() {
    assert_ok("Empty :: struct; main :: () => { };");
}

#[test]
fn decl_struct_fields() {
    let p = parse_ok("Point :: struct { x: s32, y: s32, }; main :: () => { };");
    let TopLevelDecl::Struct { body, .. } = &p.items[0].inner.decl else {
        panic!("struct");
    };
    assert!(matches!(body, StructBody::Fields(_)));
}

#[test]
fn decl_struct_fields_trailing_comma_only() {
    assert_ok("S :: struct { a: s32, }; main :: () => { };");
}

#[test]
fn decl_struct_empty_braces() {
    assert_ok("S :: struct { }; main :: () => { };");
}

#[test]
fn decl_struct_tuple() {
    let p = parse_ok("Pkt :: struct([u8; 4]); main :: () => { };");
    let TopLevelDecl::Struct { body, .. } = &p.items[0].inner.decl else {
        panic!("struct");
    };
    assert!(matches!(body, StructBody::Tuple(_)));
}

#[test]
fn decl_struct_tuple_empty() {
    assert_ok("U :: struct(); main :: () => { };");
}

#[test]
fn decl_struct_generics() {
    assert_ok("Box :: <t> struct { value: s32, }; main :: () => { };");
}

#[test]
fn decl_struct_pub() {
    assert_ok("pub Point :: struct { x: s32, }; main :: () => { };");
}

// -----------------------------------------------------------------------------
// Enum declarations
// -----------------------------------------------------------------------------

#[test]
fn decl_enum_unit_variants() {
    let p = parse_ok("E :: enum { A, B, C, }; main :: () => { };");
    let TopLevelDecl::Enum { variants, .. } = &p.items[0].inner.decl else {
        panic!("enum");
    };
    assert_eq!(variants.len(), 3);
    assert!(matches!(variants[0].kind, Variant::Unit));
}

#[test]
fn decl_enum_tuple_variant() {
    let p = parse_ok("E :: enum { N(s32), }; main :: () => { };");
    let TopLevelDecl::Enum { variants, .. } = &p.items[0].inner.decl else {
        panic!("enum");
    };
    assert!(matches!(variants[0].kind, Variant::Tuple(_)));
}

#[test]
fn decl_enum_struct_variant() {
    let p = parse_ok("E :: enum { S { x: s32, }, }; main :: () => { };");
    let TopLevelDecl::Enum { variants, .. } = &p.items[0].inner.decl else {
        panic!("enum");
    };
    assert!(matches!(variants[0].kind, Variant::Struct(_)));
}

#[test]
fn decl_enum_empty() {
    assert_ok("E :: enum { }; main :: () => { };");
}

#[test]
fn decl_enum_generics() {
    assert_ok("Opt :: <t> enum { Started, Stopped, }; main :: () => { };");
}

// -----------------------------------------------------------------------------
// Type alias, trait, impl, function, const, var
// -----------------------------------------------------------------------------

#[test]
fn decl_type_alias() {
    let p = parse_ok("type Id = u64; main :: () => { };");
    assert!(matches!(
        p.items[0].inner.decl,
        TopLevelDecl::TypeAlias { .. }
    ));
}

#[test]
fn decl_type_alias_generics() {
    assert_ok("type Pair<k, v> = (s32, bool); main :: () => { };");
}

#[test]
fn decl_trait_method_sig() {
    assert_ok("Eq :: trait { eq :: (self, other: &Self) => bool; }; main :: () => { };");
}

#[test]
fn decl_trait_associated_type() {
    assert_ok("It :: trait { type Item; }; main :: () => { };");
}

#[test]
fn decl_impl_for_trait() {
    assert_ok("P :: impl for Eq { eq :: (self, o: &Self) => bool { true }; }; main :: () => { };");
}

#[test]
fn decl_impl_inherent() {
    assert_ok("P :: impl { len :: () => s32 { 0 }; }; main :: () => { };");
}

#[test]
fn decl_function_no_return_type() {
    assert_ok("main :: () { };");
}

#[test]
fn decl_function_with_return_type() {
    assert_ok("main :: () => s32 { 0 };");
}

#[test]
fn decl_function_omit_unit_before_block() {
    assert_ok("main :: () => { };");
}

#[test]
fn decl_function_generics() {
    assert_ok("id :: <t> (x: s32) => s32 { x }; main :: () => { };");
}

#[test]
fn decl_function_receiver_self() {
    assert_ok("f :: (self) => s32 { 0 }; main :: () => { };");
}

#[test]
fn decl_function_receiver_mut_self() {
    assert_ok("f :: (mut self) => s32 { 0 }; main :: () => { };");
}

#[test]
fn decl_function_receiver_typed() {
    assert_ok("f :: (self: &Self) => s32 { 0 }; main :: () => { };");
}

#[test]
fn decl_function_receiver_mut_typed() {
    assert_ok("f :: (mut self: &mut Self) => s32 { 0 }; main :: () => { };");
}

#[test]
fn decl_function_directive_inline() {
    let p = parse_ok("#inline f :: () => { }; main :: () => { };");
    let TopLevelDecl::Function(f) = &p.items[0].inner.decl else {
        panic!("fn");
    };
    assert!(f.directives.contains(&FnDirective::Inline));
}

#[test]
fn decl_function_directive_cold() {
    let p = parse_ok("#cold f :: () => { }; main :: () => { };");
    let TopLevelDecl::Function(f) = &p.items[0].inner.decl else {
        panic!("fn");
    };
    assert!(f.directives.contains(&FnDirective::Cold));
}

#[test]
fn decl_function_directive_hot() {
    let p = parse_ok("#hot f :: () => { }; main :: () => { };");
    let TopLevelDecl::Function(f) = &p.items[0].inner.decl else {
        panic!("fn");
    };
    assert!(f.directives.contains(&FnDirective::Hot));
}

#[test]
fn decl_function_unsafe() {
    let p = parse_ok("#unsafe f :: () => { }; main :: () => { };");
    let TopLevelDecl::Function(f) = &p.items[0].inner.decl else {
        panic!("fn");
    };
    assert!(f.unsafe_);
}

#[test]
fn decl_const_with_type() {
    assert_ok("const x: s32 = 1; main :: () => { };");
}

#[test]
fn decl_const_inferred_type() {
    assert_ok("const x = 1; main :: () => { };");
}

#[test]
fn decl_var() {
    assert_ok("var x: s32 = 0; main :: () => { };");
}

#[test]
fn decl_top_level_const_in_main() {
    assert_ok(&in_main("const x = 1;"));
}

// -----------------------------------------------------------------------------
// Primitive and composite types
// -----------------------------------------------------------------------------

#[test]
fn type_primitive_bool() {
    assert_ok(&with_type_alias("bool"));
}

#[test]
fn type_primitive_s8() {
    assert_ok(&with_type_alias("s8"));
}

#[test]
fn type_primitive_s16() {
    assert_ok(&with_type_alias("s16"));
}

#[test]
fn type_primitive_s32() {
    assert_ok(&with_type_alias("s32"));
}

#[test]
fn type_primitive_s64() {
    assert_ok(&with_type_alias("s64"));
}

#[test]
fn type_primitive_s128() {
    assert_ok(&with_type_alias("s128"));
}

#[test]
fn type_primitive_u8() {
    assert_ok(&with_type_alias("u8"));
}

#[test]
fn type_primitive_u16() {
    assert_ok(&with_type_alias("u16"));
}

#[test]
fn type_primitive_u32() {
    assert_ok(&with_type_alias("u32"));
}

#[test]
fn type_primitive_u64() {
    assert_ok(&with_type_alias("u64"));
}

#[test]
fn type_primitive_u128() {
    assert_ok(&with_type_alias("u128"));
}

#[test]
fn type_primitive_f32() {
    assert_ok(&with_type_alias("f32"));
}

#[test]
fn type_primitive_f64() {
    assert_ok(&with_type_alias("f64"));
}

#[test]
fn type_option() {
    assert_ok(&with_type_alias("Option<s32>"));
}

#[test]
fn type_result() {
    assert_ok(&with_type_alias("Result<s32, bool>"));
}

#[test]
fn type_unit() {
    assert_ok(&with_type_alias("()"));
}

#[test]
fn type_tuple() {
    assert_ok(&with_type_alias("(s32, bool)"));
}

#[test]
fn type_array() {
    assert_ok(&with_type_alias("[u8; 16]"));
}

#[test]
fn type_slice() {
    assert_ok(&with_type_alias("[u8]"));
}

#[test]
fn type_ref() {
    assert_ok(&with_type_alias("&s32"));
}

#[test]
fn type_ref_mut() {
    assert_ok(&with_type_alias("&mut s32"));
}

#[test]
fn type_ptr() {
    assert_ok(&with_type_alias("*s32"));
}

#[test]
fn type_ptr_mut() {
    assert_ok(&with_type_alias("*mut s32"));
}

#[test]
fn type_named_generics() {
    assert_ok(&with_type_alias("Vec<s32>"));
}

#[test]
fn type_function() {
    assert_ok(&with_type_alias(":: (s32, bool) => s32"));
}

#[test]
fn type_self_type() {
    assert_ok("f :: (self: &Self) => Self { 0 }; main :: () => { };");
}

#[test]
fn type_generic_param_bounds() {
    assert_ok("F :: <t: Clone + Copy> struct { x: s32, }; main :: () => { };");
}

// -----------------------------------------------------------------------------
// Literals (expression context)
// -----------------------------------------------------------------------------

#[test]
fn expr_literal_int_decimal() {
    assert_ok(&in_main_expr("42"));
}

#[test]
fn expr_literal_int_hex() {
    assert_ok(&in_main_expr("0xff"));
}

#[test]
fn expr_literal_int_binary() {
    assert_ok(&in_main_expr("0b1010"));
}

#[test]
fn expr_literal_int_unsigned_suffix() {
    assert_ok(&in_main_expr("42u"));
}

#[test]
fn expr_literal_float() {
    assert_ok(&in_main_expr("3.14"));
}

#[test]
fn expr_literal_float_exponent() {
    assert_ok(&in_main_expr("1e10"));
}

#[test]
fn expr_literal_float_suffix_f64() {
    assert_ok(&in_main_expr("2.5f64"));
}

#[test]
fn expr_literal_bool_true() {
    assert_ok(&in_main_expr("true"));
}

#[test]
fn expr_literal_bool_false() {
    assert_ok(&in_main_expr("false"));
}

#[test]
fn expr_literal_byte_char() {
    assert_ok(&in_main_expr("b'a'"));
}

#[test]
fn expr_literal_byte_string() {
    assert_ok(&in_main_expr("b\"hi\""));
}

// -----------------------------------------------------------------------------
// Binary operators (each BinOp)
// -----------------------------------------------------------------------------

#[test]
fn expr_binop_or() {
    assert_ok(&in_main_expr("true || false"));
}

#[test]
fn expr_binop_and() {
    assert_ok(&in_main_expr("true && false"));
}

#[test]
fn expr_binop_eq() {
    assert_ok(&in_main_expr("1 == 2"));
}

#[test]
fn expr_binop_ne() {
    assert_ok(&in_main_expr("1 != 2"));
}

#[test]
fn expr_binop_lt() {
    assert_ok(&in_main_expr("1 < 2"));
}

#[test]
fn expr_binop_le() {
    assert_ok(&in_main_expr("1 <= 2"));
}

#[test]
fn expr_binop_gt() {
    assert_ok(&in_main_expr("1 > 2"));
}

#[test]
fn expr_binop_ge() {
    assert_ok(&in_main_expr("1 >= 2"));
}

#[test]
fn expr_binop_bit_or() {
    assert_ok(&in_main_expr("1 | 2"));
}

#[test]
fn expr_binop_bit_xor() {
    assert_ok(&in_main_expr("1 ^ 2"));
}

#[test]
fn expr_binop_bit_and() {
    assert_ok(&in_main_expr("1 & 2"));
}

#[test]
fn expr_binop_shl() {
    assert_ok(&in_main_expr("1 << 2"));
}

#[test]
fn expr_binop_shr() {
    assert_ok(&in_main_expr("1 >> 2"));
}

#[test]
fn expr_binop_add() {
    assert_ok(&in_main_expr("1 + 2"));
}

#[test]
fn expr_binop_sub() {
    assert_ok(&in_main_expr("1 - 2"));
}

#[test]
fn expr_binop_mul() {
    assert_ok(&in_main_expr("1 * 2"));
}

#[test]
fn expr_binop_div() {
    assert_ok(&in_main_expr("1 / 2"));
}

#[test]
fn expr_binop_mod() {
    assert_ok(&in_main_expr("1 % 2"));
}

#[test]
fn expr_binop_pow() {
    assert_ok(&in_main_expr("2 ** 3"));
}

// -----------------------------------------------------------------------------
// Unary operators (each UnaryOp)
// -----------------------------------------------------------------------------

#[test]
fn expr_unary_neg() {
    assert_ok(&in_main_expr("-1"));
}

#[test]
fn expr_unary_not() {
    assert_ok(&in_main_expr("!true"));
}

#[test]
fn expr_unary_bitnot() {
    assert_ok(&in_main_expr("~0"));
}

#[test]
fn expr_unary_deref() {
    assert_ok(&in_main_expr("*p"));
}

#[test]
fn expr_unary_ref() {
    assert_ok(&in_main_expr("&x"));
}

#[test]
fn expr_unary_ref_mut() {
    assert_ok(&in_main_expr("&mut x"));
}

// -----------------------------------------------------------------------------
// Assignment operators (each AssignOp)
// -----------------------------------------------------------------------------

#[test]
fn expr_assign_plain() {
    assert_ok(&in_main("var x: s32 = 0; x = 1;"));
}

#[test]
fn expr_assign_add() {
    assert_ok(&in_main("var x: s32 = 0; x += 1;"));
}

#[test]
fn expr_assign_sub() {
    assert_ok(&in_main("var x: s32 = 0; x -= 1;"));
}

#[test]
fn expr_assign_mul() {
    assert_ok(&in_main("var x: s32 = 0; x *= 2;"));
}

#[test]
fn expr_assign_div() {
    assert_ok(&in_main("var x: s32 = 0; x /= 2;"));
}

#[test]
fn expr_assign_mod() {
    assert_ok(&in_main("var x: s32 = 0; x %= 2;"));
}

#[test]
fn expr_assign_right_associative() {
    let p = parse_ok(&in_main_expr("a = b = 1"));
    let e = support::first_stmt_expr(&main_fn(&p).body);
    assert!(matches!(
        e,
        Expr::Assign {
            op: AssignOp::Assign,
            ..
        }
    ));
}

// -----------------------------------------------------------------------------
// Cast, postfix, primary forms
// -----------------------------------------------------------------------------

#[test]
fn expr_cast_single() {
    assert_ok(&in_main_expr("1 as u8"));
}

#[test]
fn expr_cast_chained() {
    assert_ok(&in_main_expr("1 as s32 as u8"));
}

#[test]
fn expr_postfix_field() {
    assert_ok(&in_main_expr("p.x"));
}

#[test]
fn expr_postfix_call() {
    assert_ok(&in_main_expr("f()"));
}

#[test]
fn expr_postfix_call_args() {
    assert_ok(&in_main_expr("f(1, 2)"));
}

#[test]
fn expr_postfix_method() {
    assert_ok(&in_main_expr("v.len()"));
}

#[test]
fn expr_postfix_method_generics() {
    assert_ok(&in_main("var v: s32 = 0; v.get<s32>();"));
}

#[test]
fn expr_postfix_index() {
    assert_ok(&in_main_expr("a[0]"));
}

#[test]
fn expr_postfix_try() {
    assert_ok(&in_main_expr("r?"));
}

#[test]
fn expr_paren_grouping() {
    assert_ok(&in_main_expr("(1 + 2)"));
}

#[test]
fn expr_tuple_literal() {
    assert_ok(&in_main_expr("(1, 2, 3)"));
}

#[test]
fn expr_array_literal_empty() {
    assert_ok(&in_main_expr("[]"));
}

#[test]
fn expr_array_literal_elems() {
    assert_ok(&in_main_expr("[1, 2, 3]"));
}

#[test]
fn expr_if_expr() {
    assert_ok(&in_main_expr("{ if true { 1 } else { 2 } }"));
}

#[test]
fn expr_if_else_if_chain() {
    assert_ok(&in_main_expr(
        "{ if true { 1 } else if true { 2 } else { 3 } }",
    ));
}

#[test]
fn expr_match_expr() {
    assert_ok(&in_main_expr("match 0 { 0 => 1; _ => 2; }"));
}

#[test]
fn expr_match_arm_guard() {
    assert_ok(&in_main_expr("match 0 { 0 if true => 1; _ => 2; }"));
}

#[test]
fn expr_block_as_expr() {
    assert_ok(&in_main_expr("{ 1 }"));
}

#[test]
fn expr_unsafe_block() {
    assert_ok(&in_main_expr("#unsafe { 1 }"));
}

#[test]
fn expr_struct_literal() {
    assert_ok(&in_main_expr("Point { x: 1, y: 2 }"));
}

#[test]
fn expr_struct_literal_spread() {
    assert_ok(&in_main_expr("Point { x: 1, ..base }"));
}

#[test]
fn expr_path_type_only() {
    assert_ok(&in_main_expr("MyType"));
}

#[test]
fn expr_path_qualified() {
    assert_ok(&in_main_expr("core::mem::size"));
}

// -----------------------------------------------------------------------------
// Statements
// -----------------------------------------------------------------------------

#[test]
fn stmt_var() {
    assert_ok(&in_main("var x: s32 = 0;"));
}

#[test]
fn stmt_const() {
    assert_ok(&in_main("const x = 1;"));
}

#[test]
fn stmt_return_value() {
    assert_ok(&in_main("return 1;"));
}

#[test]
fn stmt_return_unit() {
    assert_ok(&in_main("return;"));
}

#[test]
fn stmt_break_value() {
    assert_ok(&in_main("loop { break 1; };"));
}

#[test]
fn stmt_break_unit() {
    assert_ok(&in_main("loop { break; };"));
}

#[test]
fn stmt_continue() {
    assert_ok(&in_main("loop { continue; };"));
}

#[test]
fn stmt_while() {
    assert_ok(&in_main("while true { };"));
}

#[test]
fn stmt_loop() {
    assert_ok(&in_main("loop { };"));
}

#[test]
fn stmt_given() {
    assert_ok(&in_main("given Some(x) = (v) { x; }"));
}

#[test]
fn stmt_unsafe() {
    assert_ok(&in_main("#unsafe { };"));
}

#[test]
fn stmt_expr() {
    assert_ok(&in_main("f();"));
}

// -----------------------------------------------------------------------------
// Block trailing expression
// -----------------------------------------------------------------------------

#[test]
fn block_trailing_expr() {
    let p = parse_ok(&in_main("const z = { const a = 1; a + 2 };"));
    let TopLevelDecl::Function(f) = &p.items[0].inner.decl else {
        panic!("expected function");
    };
    let BlockItem::Stmt(Stmt::Const { init, .. }) = &f.body.inner.items[0] else {
        panic!("expected const");
    };
    let Expr::Block(block) = &init.inner else {
        panic!("expected block expr");
    };
    assert!(matches!(block.inner.items.last(), Some(BlockItem::Expr(_))));
}

#[test]
fn block_trailing_if_expr() {
    assert_ok(&in_main("const z = { if true { 1 } else { 2 } };"));
}

#[test]
fn block_trailing_match_expr() {
    assert_ok(&in_main("const z = { match 0 { _ => 1; } };"));
}

// -----------------------------------------------------------------------------
// Patterns (via match)
// -----------------------------------------------------------------------------

#[test]
fn pat_wildcard() {
    assert_ok(&in_main_expr("match 0 { _ => 0; }"));
}

#[test]
fn pat_literal_int() {
    assert_ok(&in_main_expr("match 0 { 0 => 0; _ => 1; }"));
}

#[test]
fn pat_ident() {
    assert_ok(&in_main_expr("match 0 { n => n; }"));
}

#[test]
fn pat_enum_none() {
    assert_ok(&in_main_expr("match 0 { None => 0; _ => 1; }"));
}

#[test]
fn pat_enum_some() {
    assert_ok(&in_main_expr("match 0 { Some(x) => x; _ => 0; }"));
}

#[test]
fn pat_enum_ok() {
    assert_ok(&in_main_expr("match 0 { Ok(x) => x; _ => 0; }"));
}

#[test]
fn pat_enum_err() {
    assert_ok(&in_main_expr("match 0 { Err(e) => e; _ => 0; }"));
}

#[test]
fn pat_struct() {
    assert_ok(&in_main_expr("match 0 { Point { x, y } => x; _ => 0; }"));
}

#[test]
fn pat_struct_field_pattern() {
    assert_ok(&in_main_expr(
        "match 0 { Point { x: 0, y: 1 } => 0; _ => 1; }",
    ));
}

#[test]
fn pat_tuple() {
    assert_ok(&in_main_expr("match 0 { Pair(a, b) => a; _ => 0; }"));
}

// -----------------------------------------------------------------------------
// Unsupported syntax (grammar-deferred)
// -----------------------------------------------------------------------------

#[test]
fn unsupported_for_in_loop() {
    assert_unsupported(&in_main("for x in xs { };"), "for-in loop");
}

#[test]
fn unsupported_range_expr_dot_dot() {
    assert_unsupported(&in_main_expr("0..1"), "range expression");
}

#[test]
fn unsupported_range_expr_dot_dot_eq() {
    assert_unsupported(&in_main_expr("0..=1"), "range expression");
}

#[test]
fn unsupported_lambda_empty() {
    assert_unsupported(&in_main_expr("() => 1"), "lambda expression");
}

#[test]
fn unsupported_lambda_with_param() {
    assert_parse_err(&in_main_expr("(x, y) => x"), |e| {
        matches!(
            e,
            ParseError::UnsupportedSyntax { .. } | ParseError::UnexpectedToken { .. }
        )
    });
}

#[test]
fn unsupported_at_spawn() {
    assert_unsupported(&in_main("@spawn f();"), "@spawn directive");
}

#[test]
fn unsupported_at_send() {
    assert_unsupported(&in_main("@send(a, b);"), "@send directive");
}

#[test]
fn unsupported_at_receive() {
    assert_unsupported(&in_main("@receive(m);"), "@receive directive");
}

#[test]
fn unsupported_at_reply() {
    assert_unsupported(&in_main("@reply(v);"), "@reply directive");
}

#[test]
fn unsupported_hash_derive_top_level() {
    assert_unsupported("#derive(Clone)\nmain :: () => { };", "#derive directive");
}

#[test]
fn unsupported_hash_derive_on_fn() {
    assert_unsupported(
        "#derive(Clone)\nf :: () => { }; main :: () => { };",
        "#derive directive",
    );
}

// -----------------------------------------------------------------------------
// Parse errors
// -----------------------------------------------------------------------------

#[test]
fn error_unexpected_token() {
    assert_parse_err(&in_main("const x = ;"), |e| {
        matches!(
            e,
            ParseError::UnexpectedToken { .. } | ParseError::UnexpectedEof { .. }
        )
    });
}

#[test]
fn error_unexpected_eof() {
    assert_parse_err("main :: () => {", |e| {
        matches!(e, ParseError::UnexpectedEof { .. })
    });
}

#[test]
fn error_invalid_pattern() {
    assert_parse_err(&in_main_expr("match 0 { + => 0; }"), |e| {
        matches!(e, ParseError::InvalidPattern { .. })
    });
}

#[test]
fn error_lex_propagates() {
    assert_parse_err("main :: () => { b\"open };", |e| {
        matches!(e, ParseError::Lex(LexError::UnterminatedString { .. }))
    });
}

#[test]
fn error_missing_semicolon_top_level() {
    assert_parse_err("main :: () => { }", |e| {
        matches!(
            e,
            ParseError::UnexpectedToken { .. } | ParseError::UnexpectedEof { .. }
        )
    });
}

// -----------------------------------------------------------------------------
// Real-world snippets (from design docs)
// -----------------------------------------------------------------------------

#[test]
fn real_world_main_signature() {
    assert_ok("main :: () => { };");
}

#[test]
fn real_world_add_function() {
    assert_ok("add :: (a: s32, b: s32) => s32 { a + b }; main :: () => { };");
}

#[test]
fn real_world_struct_enum_program() {
    assert_ok(
        r"
Point :: struct { x: s32, y: s32, };
Token :: enum { Eof, Number(s32), };
main :: () => { };
",
    );
}

#[test]
fn real_world_cast_no_implicit_widen() {
    assert_ok(&in_main("const n: u8 = 42 as u8;"));
}

#[test]
fn real_world_match_option() {
    assert_ok(&in_main(
        "const _r = match (v) { Some(x) => x; None => 0; };",
    ));
}

// -----------------------------------------------------------------------------
// Operator precedence smoke (nested expression parses)
// -----------------------------------------------------------------------------

#[test]
fn precedence_cast_vs_add() {
    assert_ok(&in_main_expr("1 + 2 as u8"));
}

#[test]
fn precedence_mul_vs_add() {
    assert_ok(&in_main_expr("1 + 2 * 3"));
}

#[test]
fn precedence_unary_vs_postfix() {
    assert_ok(&in_main_expr("-p.x"));
}

#[test]
fn precedence_all_binops_in_one_expr() {
    assert_ok(&in_main_expr(
        "1 || 2 && 3 == 4 < 5 | 6 ^ 7 & 8 << 9 >> 10 + 11 - 12 * 13 / 14 % 15 ** 16",
    ));
}
