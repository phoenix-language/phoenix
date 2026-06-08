//! Exhaustive parser integration tests — one case per MVP grammar / [`ParseError`] path.
//!
//! Crate: [`phx_syntax`] — entry [`phx_syntax::parse`].

#![allow(
    clippy::approx_constant,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::too_many_lines
)]

use phx_diagnostics::{ExpectedToken, LexError, ParseBag, ParseError};
use phx_syntax::ast::decl::{FnDirective, StructBody, TopLevelDecl, Variant};
use phx_syntax::ast::expr::{AssignOp, Expr, PostfixOp};
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
        parse(source).map_or_else(
            |e| panic!("expected parse ok for:\n{source}\nerror: {e}"),
            |sf| sf.program,
        )
    }

    pub fn parse_err(source: &str) -> ParseBag {
        parse(source).expect_err("expected parse error")
    }

    pub fn parse_err_first(source: &str) -> ParseError {
        let bag = parse_err(source);
        bag.errors()
            .first()
            .cloned()
            .expect("expected at least one parse error")
    }

    pub fn assert_ok(source: &str) {
        let _ = parse_ok(source);
    }

    pub fn assert_unsupported(source: &str, feature: &str) {
        match parse_err_first(source) {
            ParseError::UnsupportedSyntax { feature: f, .. } => {
                assert_eq!(f, feature, "source: {source:?}");
            }
            other => panic!("expected UnsupportedSyntax({feature}) for {source:?}, got {other:?}"),
        }
    }

    pub fn assert_parse_err(source: &str, check: fn(&ParseError) -> bool) {
        let err = parse_err_first(source);
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

    pub fn first_stmt_expr(block: &BlockNode) -> &Expr {
        match &block.inner.items[0] {
            BlockItem::Stmt(Stmt::Const { init, .. } | Stmt::Expr(init)) => &init.inner,
            BlockItem::Expr(e) => &e.inner,
            other => panic!("expected expr item, got {other:?}"),
        }
    }
}

use support::{
    assert_ok, assert_parse_err, assert_unsupported, in_main, in_main_expr, main_fn, parse_err,
    parse_ok, with_type_alias,
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
fn program_import_brace_type_names() {
    assert_ok("#import std::core::error::{ Error, FooError }; main :: () => { };");
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
fn block_import_single_item() {
    let p = parse_ok("main :: () => { #import util::math::add; };");
    let TopLevelDecl::Function(f) = &p.items[0].inner.decl else {
        panic!("expected function");
    };
    assert!(matches!(
        f.body.inner.items.first(),
        Some(BlockItem::Import(_))
    ));
    assert_eq!(p.imports.len(), 0);
}

#[test]
fn block_import_glob_in_if_arm() {
    let p = parse_ok(
        "main :: () => { if true { #import util::math::{ * }; const _ = 0; } else { const _ = 0; }; };",
    );
    let TopLevelDecl::Function(f) = &p.items[0].inner.decl else {
        panic!("expected function");
    };
    let BlockItem::Expr(expr) = &f.body.inner.items[0] else {
        panic!("expected if as block expr");
    };
    let Expr::If { then_block, .. } = &expr.inner else {
        panic!("expected if expr");
    };
    assert!(matches!(
        then_block.inner.items.first(),
        Some(BlockItem::Import(_))
    ));
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
fn decl_impl_associated_type() {
    assert_ok(
        "Counter :: struct { n: s32 }; It :: trait { type Item; peek :: () => Self::Item; }; Counter :: impl :: It { type Item = s32; peek :: () => s32 { self.n }; }; main :: () => { };",
    );
}

#[test]
fn type_self_assoc() {
    assert_ok("It :: trait { type Item; f :: () => Self::Item; }; main :: () => { };");
}

#[test]
fn decl_impl_for_trait() {
    assert_ok("P :: impl :: Eq { eq :: (self, o: &Self) => bool { true }; }; main :: () => { };");
}

#[test]
fn decl_impl_for_trait_legacy_syntax_rejected() {
    assert_unsupported(
        "P :: impl for Eq { eq :: (self, o: &Self) => bool { true }; }; main :: () => { };",
        "trait impl uses 'Type :: impl :: Trait', not 'impl for Trait'",
    );
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
fn decl_generic_param_from_bound() {
    assert_ok(
        "From :: <source> trait { from :: (value: source) => Self; }; to :: <u, t: From<u>> (x: u) => t { t::from(x) }; main :: () => { };",
    );
}

#[test]
fn decl_impl_parameterized_trait() {
    assert_ok(
        "From :: <source> trait { from :: (value: source) => Self; }; Wrap :: struct { n: s32 }; Wrap :: impl :: From<s32> { from :: (value: s32) => Wrap { Wrap { n: value } }; }; main :: () => { };",
    );
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
fn decl_bracket_attribute_cfg() {
    assert_ok("#[cfg(target_os = \"linux\")] pub f :: () => { }; main :: () => { };");
}

#[test]
fn decl_bracket_attribute_must_use() {
    let p = parse_ok("#[must_use] f :: () => s32 { 1 }; main :: () => { };");
    assert_eq!(p.items[0].inner.attrs.len(), 1);
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
    let p = parse_ok("unsafe f :: () => { }; main :: () => { };");
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

#[test]
fn expr_literal_string() {
    assert_ok(&in_main_expr("\"hi\""));
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
fn expr_match_ident_scrutinee_no_parens() {
    assert_ok(&in_main("const _ = match n { _ => 0; };"));
}

#[test]
fn expr_match_ident_literal_arms_no_parens() {
    assert_ok(&in_main("const _ = match n { 0 => 1; _ => 2; };"));
}

#[test]
fn expr_match_enum_struct_variant_arm() {
    assert_ok(
        "E :: enum { Err { code: s32 }, }; main :: () => { const _ = match e { Err { code: c } => c; }; };",
    );
}

#[test]
fn expr_match_type_name_scrutinee_no_parens() {
    assert_ok(&in_main("const _ = match Point { _ => 0; };"));
}

#[test]
fn stmt_if_const_ident_scrutinee_no_parens() {
    assert_ok(&in_main("if const _ = x { };"));
}

#[test]
fn stmt_while_comparison_block_no_parens() {
    assert_ok(&in_main("while i < n { };"));
}

#[test]
fn expr_if_logical_block_no_parens() {
    assert_ok(&in_main("const _v = if c || d { 1 } else { 0 };"));
}

#[test]
fn stmt_if_expr_no_parens() {
    assert_ok(&in_main("if c || d { 1 } else { 0 };"));
}

#[test]
fn const_if_true_else_no_parens() {
    assert_ok(&in_main("const _v = if true { 1 } else { 0 };"));
}

#[test]
fn expr_if_in_block_as_const_init() {
    assert_ok(&in_main("const z = { if true { 1 } else { 2 } };"));
}

#[test]
fn expr_block_as_expr() {
    assert_ok(&in_main_expr("{ 1 }"));
}

#[test]
fn expr_unsafe_block() {
    assert_ok(&in_main_expr("unsafe { 1 }"));
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
fn expr_struct_literal_generics_snake_case() {
    let src = &format!(
        "Box :: struct {{ v: s32 }}; {}",
        in_main_expr("box :: <s32> { v: 1 }")
    );
    let program = parse_ok(src);
    let expr = support::first_stmt_expr(&main_fn(&program).body);
    match expr {
        Expr::StructLit { generics, .. } => {
            assert!(
                generics.as_ref().is_some_and(|g| !g.is_empty()),
                "expected generic type arguments on snake_case struct literal"
            );
        }
        other => panic!("expected StructLit, got {other:?}"),
    }
}

#[test]
fn expr_struct_literal_generics() {
    let src = &format!(
        "Point :: struct {{ x: s32, y: s32 }}; {}",
        in_main_expr("Point::<s32> { x: 1, y: 2 }")
    );
    let program = parse_ok(src);
    let expr = support::first_stmt_expr(&main_fn(&program).body);
    match expr {
        Expr::StructLit { generics, .. } => {
            assert!(
                generics.as_ref().is_some_and(|g| !g.is_empty()),
                "expected generic type arguments"
            );
        }
        other => panic!("expected StructLit, got {other:?}"),
    }
}

#[test]
fn expr_path_type_only() {
    assert_ok(&in_main_expr("MyType"));
}

#[test]
fn expr_path_qualified() {
    assert_ok(&in_main_expr("core::mem::size"));
}

#[test]
fn expr_generic_call_on_value_ident() {
    let src = &format!(
        "id :: <t> (x: s32) => s32 {{ x }}; {}",
        in_main_expr("id :: <s32> (1)")
    );
    let program = parse_ok(src);
    let expr = support::first_stmt_expr(&main_fn(&program).body);
    match expr {
        Expr::Postfix { ops, .. } => {
            assert!(
                ops.iter().any(|op| matches!(
                    op,
                    PostfixOp::Call {
                        generics: Some(g),
                        ..
                    } if !g.is_empty()
                )),
                "expected Call with generic args"
            );
        }
        other => panic!("expected Postfix call, got {other:?}"),
    }
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

/// `while` conditions use logical-or precedence (same as `if`), not assignment.
#[test]
fn stmt_while_equality_condition() {
    assert_ok(&in_main("while a == b { };"));
}

/// Assignment is not valid at `while` condition precedence.
#[test]
fn stmt_while_assignment_in_condition_is_parse_error() {
    assert_parse_err(&in_main("while x = 1 { };"), |e| {
        matches!(
            e,
            ParseError::UnexpectedToken { .. } | ParseError::UnexpectedEof { .. }
        )
    });
}

#[test]
fn stmt_loop() {
    assert_ok(&in_main("loop { };"));
}

#[test]
fn stmt_if_const() {
    assert_ok(&in_main("if const Some(x) = (v) { x; };"));
}

#[test]
fn stmt_if_var() {
    assert_ok(&in_main("if var Some(x) = (v) { x; };"));
}

#[test]
fn stmt_if_const_else_if_const() {
    assert_ok(&in_main(
        "if const Some(x) = v { x; } else if const None = v { 0; } else { 1; };",
    ));
}

#[test]
fn stmt_unsafe() {
    assert_ok(&in_main("unsafe { };"));
}

#[test]
fn stmt_expr() {
    assert_ok(&in_main("f();"));
}

#[test]
fn expr_enum_ctor_ok_err_some_none() {
    assert_ok(&in_main(
        "const a = Ok(1); const b = Err(2); const c = Some(3); const d = None;",
    ));
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
// Parse recovery (multiple errors per file)
// -----------------------------------------------------------------------------

#[test]
fn parse_recovery_collects_multiple_errors() {
    let bag = parse_err("main :: () => { const x = ; const y: s32 = ; };");
    assert!(
        bag.errors().len() >= 2,
        "expected at least two parse errors, got {:?}",
        bag.errors()
    );
}

// -----------------------------------------------------------------------------
// Deferred syntax (parse-only; typeck rejects — see typeck integration tests)
// -----------------------------------------------------------------------------

#[test]
fn deferred_parse_for_in_loop() {
    assert_ok(&in_main("for x in xs { };"));
}

#[test]
fn deferred_parse_range_expr_dot_dot() {
    assert_ok(&in_main_expr("0..1"));
}

#[test]
fn deferred_parse_range_expr_dot_dot_eq() {
    assert_ok(&in_main_expr("0..=1"));
}

#[test]
fn deferred_parse_lambda_empty() {
    assert_ok(&in_main_expr("() => 1"));
}

#[test]
fn deferred_parse_lambda_with_param() {
    assert_ok(&in_main_expr("(x: s32) => x"));
}

#[test]
fn deferred_parse_at_spawn() {
    assert_ok(&in_main("@spawn(f);"));
}

#[test]
fn deferred_parse_at_send() {
    assert_ok(&in_main("@send(a, b);"));
}

#[test]
fn deferred_parse_at_receive() {
    assert_ok(&in_main("@receive(m);"));
}

#[test]
fn deferred_parse_at_reply() {
    assert_ok(&in_main("@reply(v);"));
}

#[test]
fn deferred_parse_hash_derive_top_level() {
    assert_ok("#derive(Clone)\nmain :: () => { };");
}

#[test]
fn parse_hash_derive_before_struct() {
    assert_ok("#derive(PartialEq)\nPoint :: struct { x: s32 }; main :: () => { };");
}

#[test]
fn deferred_parse_hash_derive_on_fn() {
    assert_ok("#derive(Clone)\nf :: () => { }; main :: () => { };");
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

#[test]
fn error_unclosed_paren_in_expr() {
    assert_parse_err(&in_main_expr("(1 + 2;"), |e| {
        matches!(
            e,
            ParseError::UnexpectedToken {
                expected: ExpectedToken::Punct(")"),
                ..
            } | ParseError::UnexpectedEof {
                expected: ExpectedToken::Punct(")"),
                ..
            }
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
    assert_ok(&in_main("const _r = match v { Some(x) => x; None => 0; };"));
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
