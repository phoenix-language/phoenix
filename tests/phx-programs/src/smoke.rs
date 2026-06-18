//! Positive smoke programs.

use super::SmokeProgram;

use super::single::*;

pub const SMOKE_SAMPLE: SmokeProgram = SmokeProgram {
    name: r"sample.phx",
    source: r"// Minimal program with arithmetic, control flow, and a function definition.

add :: (a: s32, b: s32) => s32 {
    a + b
};

main :: () => {
  const base: s32 = 10;
  const step: s32 = 2;
  const sum: s32 = add(base, step);
  const ok: bool = { if sum > 0 { true } else { false } };
  const _ = ok;
};
",
};

pub const SMOKE_CONTROL_FLOW: SmokeProgram = SmokeProgram {
    name: r"control_flow.phx",
    source: r"// Exercises while, loop, break, continue-in-if, and if-in-loop.

main :: () => {
  var i: s32 = 0;
  while 5 > (i) {
    i = i + 1;
  };
  loop {
    if 10 > (i) {
      i = i + 1;
      continue;
    }
    break;
  };
  const _ = i;
};
",
};

pub const SMOKE_CONTINUE_IN_IF: SmokeProgram = SmokeProgram {
    name: r"continue_in_if.phx",
    source: r"// continue inside if within loop (regression for loop back-edge placement).

main :: () => {
  var i: s32 = 0;
  loop {
    i = i + 1;
    if 3 > (i) {
      continue;
    }
    break;
  };
  const _ = i;
};
",
};

pub const SMOKE_LOGICAL: SmokeProgram = SmokeProgram {
    name: r"logical.phx",
    source: r"// Short-circuit && and || in if conditions.

main :: () => {
  const a: bool = { if 1 == 1 { true } else { false } };
  const b: bool = { if 1 == 2 { true } else { false } };
  const c: bool = a && b;
  const d: bool = a || b;
  const ok: bool = { if (c || d) { true } else { false } };
  const _ = ok;
};
",
};

pub const SMOKE_MATCH_INT: SmokeProgram = SmokeProgram {
    name: r"match_int.phx",
    source: r"// match on s32 with literal and wildcard arms.

main :: () => {
  var n: s32 = 2;
  const x: s32 = { match n { 1 => 10; 2 => 20; _ => 30; } };
  const _ = x;
};
",
};

pub const SMOKE_MATCH_IDENT: SmokeProgram = SmokeProgram {
    name: r"match_ident.phx",
    source: r"main :: () => {
    var n: s32 = 1;
    const x: s32 = match n { 0 => 10; _ => 20; };
    const _ = x;
};
",
};

pub const SMOKE_MATCH_BOOL: SmokeProgram = SmokeProgram {
    name: r"match_bool.phx",
    source: r"// match on bool (via if-style literals in arms).

main :: () => {
  const flag: bool = { if 1 == 1 { true } else { false } };
  const x: s32 = { match flag { true => 1; false => 0; _ => 2; } };
  const _ = x;
};
",
};

pub const SMOKE_STRUCT_POINT: SmokeProgram = SmokeProgram {
    name: r"struct_point.phx",
    source: r"Point :: struct {
    x: s32,
    y: s32,
};

sum_x :: (p: Point) => s32 {
    p.x + p.y
};

main :: () => {
    const p: Point = Point { x: 3, y: 4 };
    const total: s32 = sum_x(p);
    const _ = total;
};
",
};

pub const SMOKE_STRUCT_ASSIGN: SmokeProgram = SmokeProgram {
    name: r"struct_assign.phx",
    source: r"Point :: struct {
    x: s32,
    y: s32,
};

main :: () => {
    var p: Point = Point { x: 1, y: 2 };
    p.x = 5;
    const _ = p.x;
};
",
};

pub const SMOKE_ENUM_MATCH: SmokeProgram = SmokeProgram {
    name: r"enum_match.phx",
    source: r"Maybe :: enum {
    None,
    Some(s32),
};

main :: () => {
    const m: Maybe = Some(42);
    const n: s32 = match m {
        None => 0;
        Some(x) => x;
    };
    const _ = n;
};
",
};

pub const SMOKE_ENUM_MATCH_STRUCT: SmokeProgram = SmokeProgram {
    name: r"enum_match_struct.phx",
    source: r"Result :: enum {
    Ok { v: s32 },
    Err { code: s32 },
};

main :: () => {
    const r: Result = Ok { v: 42 };
    const n: s32 = match r {
        Ok { v } => v;
        Err { code } => code;
    };
    const _ = n;
};
",
};

pub const SMOKE_STRUCT_METHOD: SmokeProgram = SmokeProgram {
    name: r"struct_method.phx",
    source: r"Point :: struct {
    x: s32,
    y: s32,
};

Point :: impl {
    zero :: () => s32 {
        self.x + self.y
    };
};

main :: () => {
    const p: Point = Point { x: 3, y: 4 };
    const total: s32 = p.zero();
    const _ = total;
};
",
};

pub const SMOKE_CAST_WIDTH: SmokeProgram = SmokeProgram {
    name: r"cast_width.phx",
    source: r"main :: () => {
    const wide: s64 = 100 as s64;
    const narrow: u8 = 42 as u8;
    const bump: s64 = narrow as s64;
    const _ = wide + bump;
};
",
};

pub const SMOKE_COMPARE_UNARY: SmokeProgram = SmokeProgram {
    name: r"compare_unary.phx",
    source: r"main :: () => {
    const a: s32 = 10;
    const b: s32 = 3;
    const neg: s32 = -a;
    const not_ok: bool = !true;
    const eq: bool = a == b;
    const ne: bool = a != b;
    const lt: bool = a < b;
    const le: bool = a <= b;
    const gt: bool = a > b;
    const ge: bool = a >= b;
    const score: s32 = if eq { 1 } else { 0 };
    const _ = neg + score + (if ne { 1 } else { 0 }) + (if lt { 0 } else { 1 }) + (if le { 0 } else { 1 }) + (if gt { 1 } else { 0 }) + (if ge { 1 } else { 0 }) + (if not_ok { 0 } else { 1 });
};
",
};

pub const SMOKE_DEEP_LOGICAL_CHAIN: SmokeProgram = SmokeProgram {
    name: r"deep_logical_chain.phx",
    source: r"main :: () => {
    const a: s32 = 1;
    const b: s32 = 2;
    const c: s32 = 3;
    const d: s32 = 4;
    const e: s32 = 5;
    const f: s32 = 6;
    const ok: bool = (a < b) && (b < c) && (c < d) && (d < e) && (e < f) && (f < 10);
    const _ = if ok { 1 } else { 0 };
};
",
};

pub const SMOKE_DEEP_LOGICAL_OR_CHAIN: SmokeProgram = SmokeProgram {
    name: r"deep_logical_or_chain.phx",
    source: r"main :: () => {
    const a: s32 = 1;
    const b: s32 = 2;
    const c: s32 = 3;
    const d: s32 = 4;
    const e: s32 = 5;
    const f: s32 = 6;
    const hit: bool = (a > 10) || (b > 10) || (c > 10) || (d > 10) || (e > 10) || (f < 10);
    const _ = if hit { 1 } else { 0 };
};
",
};

pub const SMOKE_MOD_BITWISE: SmokeProgram = SmokeProgram {
    name: r"mod_bitwise.phx",
    source: r"main :: () => {
    const a: s32 = 10;
    const b: s32 = 3;
    const rem: s32 = a % b;
    const masked: s32 = a & 7;
    const neg: s32 = -a;
    const _ = rem + masked + neg;
};
",
};

pub const SMOKE_ARRAY_INDEX: SmokeProgram = SmokeProgram {
    name: r"array_index.phx",
    source: r"main :: () => {
    const arr: [s32; 3] = [10, 20, 30];
    const v: s32 = arr[1];
    const _ = v;
};
",
};

pub const SMOKE_TUPLE_LIT: SmokeProgram = SmokeProgram {
    name: r"tuple_lit.phx",
    source: r"main :: () => {
    const pair: (s32, s32) = (1, 2);
    const v: s32 = pair[0];
    const _ = v;
};
",
};

pub const SMOKE_IF_CONST_STRUCT: SmokeProgram = SmokeProgram {
    name: r"if_const_struct.phx",
    source: r"Point :: struct {
    x: s32,
    y: s32,
};

main :: () => {
    const p: Point = Point { x: 3, y: 4 };
    if const Point { x, y } = (p) {
        const _ = x + y;
    };
};
",
};

pub const SMOKE_TRAIT_EQ: SmokeProgram = SmokeProgram {
    name: r"trait_eq.phx",
    source: r"PartialEq :: trait {
    eq :: (self: Point, other: Point) => bool;
};

Point :: struct {
    x: s32,
    y: s32,
};

Point :: impl :: PartialEq {
    eq :: (self: Point, other: Point) => bool {
        self.x == other.x && self.y == other.y
    };
};

main :: () => {
    var p: Point = Point { x: 1, y: 2 };
    var q: Point = Point { x: 1, y: 2 };
    const same: bool = p.eq(q);
    const _ = same;
};
",
};

pub const SMOKE_TRAIT_INHERENT: SmokeProgram = SmokeProgram {
    name: r"trait_inherent.phx",
    source: r"PartialEq :: trait {
    eq :: (self: Point, other: Point) => bool;
};

Point :: struct {
    x: s32,
    y: s32,
};

Point :: impl {
    sum :: () => s32 {
        self.x + self.y
    };
};

Point :: impl :: PartialEq {
    eq :: (self: Point, other: Point) => bool {
        self.x == other.x && self.y == other.y
    };
};

main :: () => {
    const p: Point = Point { x: 1, y: 2 };
    const q: Point = Point { x: 1, y: 2 };
    const r: Point = Point { x: 1, y: 2 };
    const total: s32 = p.sum();
    const same: bool = r.eq(q);
    const _ = total;
    const check: bool = same;
};
",
};

pub const SMOKE_PRIMITIVES_FLOAT: SmokeProgram = SmokeProgram {
    name: r"primitives_float.phx",
    source: r"main :: () => {
    const a: f32 = 1.5f32;
    const b: f64 = 2.25f64;
    const af: f64 = a as f64;
    const c: f64 = af + b;
    const wide: s64 = 100 as s64;
    const narrow: u8 = 42 as u8;
    const _ = c + (wide as f64) + (narrow as f64);
};
",
};

pub const SMOKE_PRIMITIVES_WIDTH: SmokeProgram = SmokeProgram {
    name: r"primitives_width.phx",
    source: r"main :: () => {
    const a: u8 = 255 as u8;
    const b: s16 = -1 as s16;
    const c: u32 = 1u;
    const d: s64 = (a as s64) + (b as s64) + (c as s64);
    const _ = d;
};
",
};

pub const SMOKE_PRIMITIVES_I128: SmokeProgram = SmokeProgram {
    name: r"primitives_i128.phx",
    source: r"main :: () => {
    const big: s128 = 1000 as s128;
    const small: s8 = big as s8;
    const _ = small;
};
",
};

pub const SMOKE_PRIMITIVES_U128: SmokeProgram = SmokeProgram {
    name: r"primitives_u128.phx",
    source: r"main :: () => {
    const base: u128 = (1u as u128) << (126 as u128);
    const top: u128 = base * (2 as u128);
    const half: u128 = top / (2 as u128);
    const rem: u128 = top % (3 as u128);
    const sum: u128 = half + rem;
    const _ = sum;
};
",
};

pub const SMOKE_SHIFT_WIDTH_MASK: SmokeProgram = SmokeProgram {
    name: r"shift_width_mask.phx",
    source: r"main :: () => {
    const u8_shift: u8 = (1u as u8) << (9 as u8);
    const u32_shift: u32 = (1u as u32) << (32 as u32);
};
",
};

pub const SMOKE_BYTE_STRING: SmokeProgram = SmokeProgram {
    name: r"byte_string.phx",
    source: r#"main :: () => {
    const s: [u8; 3] = b"ABC";
    const v: u8 = s[1];
    const _ = v;
};
"#,
};

pub const SMOKE_STRING_LITERAL: SmokeProgram = SmokeProgram {
    name: r"string_literal.phx",
    source: r#"main :: () => {
    const s: str = "hello";
    const sl: [u8] = s as [u8];
    const v: u8 = sl[4];
    const _ = v;
};
"#,
};

pub const SMOKE_BYTE_STRING_AS_STR: SmokeProgram = SmokeProgram {
    name: r"byte_string_as_str.phx",
    source: r#"main :: () => {
    const arr = b"hello";
    const s: str = arr as str;
    const sl: [u8] = s as [u8];
    const v: u8 = sl[4];
    const _ = v;
};
"#,
};

pub const SMOKE_REF_LOCAL: SmokeProgram = SmokeProgram {
    name: r"ref_local.phx",
    source: r"main :: () => {
    var x: s32 = 10;
    const p: &s32 = &x;
    const v: s32 = *p;
    const _ = v;
};
",
};

pub const SMOKE_REF_FN_PARAM: SmokeProgram = SmokeProgram {
    name: r"ref_fn_param.phx",
    source: r"accepts_ref :: (_v: &s32) => () {
    const _ = ();
};

identity :: (x: s32) => s32 {
    x
};

main :: () => {
    var x: s32 = 42;
    accepts_ref(&x);
    const v: s32 = identity(x);
    const _ = v;
};
",
};

pub const SMOKE_MUT_REF_LOCAL: SmokeProgram = SmokeProgram {
    name: r"mut_ref_local.phx",
    source: r"accepts_mut_ref :: (_v: &mut u8) => () {
    const _ = ();
};

main :: () => {
    var cell: u8 = 66 as u8;
    accepts_mut_ref(&mut cell);
    const _ = cell;
};
",
};

pub const SMOKE_DEREF_PTR: SmokeProgram = SmokeProgram {
    name: r"deref_ptr.phx",
    source: r"main :: () => {
    var cell: u8 = 77 as u8;
    const addr: &u8 = &cell;
    const v: u8 = *addr;
    const _ = v;
};
",
};

pub const SMOKE_SLICE_FROM_ARRAY: SmokeProgram = SmokeProgram {
    name: r"slice_from_array.phx",
    source: r#"main :: () => {
    const arr: [u8; 4] = b"WXYZ";
    const sl: [u8] = arr as [u8];
    const v: u8 = sl[2];
    const _ = v;
};
"#,
};

pub const SMOKE_FACTORIAL: SmokeProgram = SmokeProgram {
    name: r"factorial.phx",
    source: r"fac :: (n: s32) => s32 {
    if n <= 1 {
        1
    } else {
        n * fac(n - 1)
    }
};

main :: () => {
    const r: s32 = fac(5);
    const _ = r;
};
",
};

pub const SMOKE_IF_CONST_ENUM_SINGLE_VARIANT: SmokeProgram = SmokeProgram {
    name: r"if_const_enum_single_variant.phx",
    source: r"Sing :: enum {
    Only(s32),
};

main :: () => {
    const s: Sing = Only(12);
    var out: s32 = 0;
    if const Only(x) = s {
        out = x;
    };
    const _ = out;
};
",
};

pub const SMOKE_IF_CONST_ENUM_NON_EXHAUSTIVE: SmokeProgram = SmokeProgram {
    name: r"if_const_enum_non_exhaustive.phx",
    source: r"Maybe :: enum {
    None,
    Some(s32),
};

main :: () => {
    const m: Maybe = Some(1);
    if const Some(x) = m {
        const _ = x;
    };
};
",
};

pub const SMOKE_IF_VAR_REASSIGN: SmokeProgram = SmokeProgram {
    name: r"if_var_reassign.phx",
    source: r"Maybe :: enum {
    None,
    Some(s32),
};

main :: () => {
    const m: Maybe = Some(5);
    var out: s32 = 0;
    if var Some(x) = m {
        x = 7;
        out = x;
    };
    const _ = out;
};
",
};

pub const SMOKE_IF_CONST_ELSE: SmokeProgram = SmokeProgram {
    name: r"if_const_else.phx",
    source: r"Maybe :: enum {
    None,
    Some(s32),
};

main :: () => {
    const m: Maybe = Some(1);
    var out: s32 = 0;
    if const None = m {
        out = 1;
    } else {
        out = 99;
    };
    const _ = out;
};
",
};

pub const SMOKE_GENERIC_FN: SmokeProgram = SmokeProgram {
    name: r"generic_fn.phx",
    source: r"id :: <t> (x: t) => t {
    x
};

main :: () => {
    const n: s32 = id :: <s32> (42);
    const _ = n;
};
",
};

pub const SMOKE_GENERIC_STRUCT: SmokeProgram = SmokeProgram {
    name: r"generic_struct.phx",
    source: r"Box :: <t> struct {
    v: t,
};

main :: () => {
    const b = Box::<s32> { v: 7 };
    const _ = b.v;
};
",
};

pub const SMOKE_GENERIC_ENUM: SmokeProgram = SmokeProgram {
    name: r"generic_enum.phx",
    source: r"Opt :: <t> enum {
    None,
    Some(t),
};

main :: () => {
    const x = Some :: <s32> (1);
    const _ = x;
};
",
};

pub const SMOKE_GENERIC_INFER: SmokeProgram = SmokeProgram {
    name: r"generic_infer.phx",
    source: r"id :: <t> (x: t) => t { x };

main :: () => {
  const n: s32 = id(42);
  const _ = n;
};
",
};

pub const SMOKE_GENERIC_ENUM_INFER: SmokeProgram = SmokeProgram {
    name: r"generic_enum_infer.phx",
    source: r"Opt :: <t> enum {
    None,
    Some(t),
};

main :: () => {
    const x = Some(1);
    const _ = x;
};
",
};

pub const SMOKE_GENERIC_ENUM_MATCH: SmokeProgram = SmokeProgram {
    name: r"generic_enum_match.phx",
    source: r"Opt :: <t> enum {
    None,
    Some(t),
};

main :: () => {
    const x = Some :: <s32> (1);
    const n: s32 = match x {
        None => 0;
        Some(v) => v;
    };
    const _ = n;
};
",
};

pub const SMOKE_GENERIC_IMPL_METHOD: SmokeProgram = SmokeProgram {
    name: r"generic_impl_method.phx",
    source: r"Box :: <t> struct { v: t };

Box :: <t> impl {
  get :: () => t { self.v };
};

main :: () => {
  const b = Box :: <s32> { v: 7 };
  const n = b.get(); // infer type: s32
  const _ = n;
};
",
};

pub const SMOKE_FN_POINTER: SmokeProgram = SmokeProgram {
    name: r"fn_pointer.phx",
    source: r"// V0-053: function pointer callback and indirect call.

lt :: (a: s32, b: s32) => bool {
    a < b
};

invoke :: (cmp: :: (s32, s32) => bool, x: s32, y: s32) => bool {
    cmp(x, y)
};

main :: () => {
    const ok: bool = invoke(lt, 1, 2);
    const _ = ok;
};
",
};

pub const SMOKE_DROP: SmokeProgram = SmokeProgram {
    name: r"drop.phx",
    source: r"// V0-054: scope-end drop glue calls Drop::drop on owned locals.

Drop :: trait {
    drop :: (self) => ();
};

Wrapper :: struct {};

Wrapper :: impl :: Drop {
    drop :: (self) => () {};
};

main :: () => {
    {
        const w = Wrapper {};
    }
};
",
};

pub const SMOKE_DERIVE_PARTIALEQ: SmokeProgram = SmokeProgram {
    name: r"derive_partialeq.phx",
    source: r"PartialEq :: trait {
    eq :: (self: &Self, other: &Self) => bool;
};

#[derive(PartialEq)]
Point :: struct {
    x: s32,
    y: s32,
};

main :: () => {
    var p: Point = Point { x: 1, y: 2 };
    var q: Point = Point { x: 1, y: 2 };
    var r: Point = Point { x: 0, y: 2 };
    const same: bool = p.eq(&q);
    const diff: bool = p.eq(&r);
    const _ = same;
    const _discard = diff;
};
",
};

pub const SMOKE_DERIVE_ENUM_PARTIALEQ: SmokeProgram = SmokeProgram {
    name: r"derive_enum_partialeq.phx",
    source: r"PartialEq :: trait {
    eq :: (self: &Self, other: &Self) => bool;
};

#[derive(PartialEq)]
Shape :: enum {
    Nil(),
    Point(s32, s32),
};

main :: () => {
    var a: Shape = Nil();
    var b: Shape = Nil();
    var c: Shape = Point(1, 2);
    var d: Shape = Point(1, 2);
    const same_nil: bool = a.eq(&b);
    const same_point: bool = c.eq(&d);
    const diff: bool = a.eq(&c);
    const _ = same_nil;
    const _discard1 = same_point;
    const _discard2 = diff;
};
",
};

pub const SMOKE_DERIVE_GENERIC_STRUCT: SmokeProgram = SmokeProgram {
    name: r"derive_generic_struct.phx",
    source: r"PartialEq :: trait {
    eq :: (self: &Self, other: &Self) => bool;
};

#[derive(PartialEq)]
Box :: <t> struct {
    v: s32,
};

main :: () => {
    var a: Box<s32> = Box :: <s32> { v: 1 };
    var b: Box<s32> = Box :: <s32> { v: 1 };
    var c: Box<s32> = Box :: <s32> { v: 2 };
    const same: bool = a.eq(&b);
    const diff: bool = a.eq(&c);
    const _ = same;
    const _discard = diff;
};
",
};

pub const SMOKE_DERIVE_GENERIC_ENUM: SmokeProgram = SmokeProgram {
    name: r"derive_generic_enum.phx",
    source: r"PartialEq :: trait {
    eq :: (self: &Self, other: &Self) => bool;
};

#[derive(PartialEq)]
Maybe :: <t> enum {
    None,
    Other(s32),
};

main :: () => {
    var a: Maybe<s32> = Other(1);
    var b: Maybe<s32> = Other(1);
    var c: Maybe<s32> = None :: <s32>();
    const same: bool = a.eq(&b);
    const diff: bool = a.eq(&c);
    const _ = same;
    const _discard = diff;
};
",
};

pub const SMOKE_ATTR_BRACKET_DERIVE: SmokeProgram = SmokeProgram {
    name: r"attr_bracket_derive.phx",
    source: r"PartialEq :: trait {
    eq :: (self: &Self, other: &Self) => bool;
};

#[derive(PartialEq)]
pub Point :: struct {
    x: s32,
    y: s32,
};

main :: () => {
    var p: Point = Point { x: 1, y: 2 };
    var q: Point = Point { x: 1, y: 2 };
    const same: bool = p.eq(&q);
    const _ = same;
};
",
};

pub const SMOKE_MILLIMETERS: SmokeProgram = SmokeProgram {
    name: r"millimeters.phx",
    source: r"PartialEq :: trait {
    eq :: (self: &Self, other: &Self) => bool;
};

#[derive(PartialEq)]
Millimeters :: struct(s32);

Millimeters :: impl {
    as_s32 :: (self: &Self) => s32 {
        self.0
    };
};

accept :: (m: Millimeters) => () {
    const _ = m;
};

main :: () => {
    const length: Millimeters = Millimeters(500);
    accept(length);
    const raw: s32 = length.as_s32();
    const _ = raw;
};
",
};

pub const SMOKE_TUPLE_STRUCT_TWO_FIELD: SmokeProgram = SmokeProgram {
    name: r"tuple_struct_two_field.phx",
    source: r"Point :: struct(s32, s32);

main :: () => {
    const p: Point = Point(1, 2);
    const x: s32 = p.0;
    const _ = x;
};
",
};

pub const SMOKE_PROGRAMS: &[SmokeProgram] = &[
    SMOKE_SAMPLE,
    SMOKE_CONTROL_FLOW,
    SMOKE_CONTINUE_IN_IF,
    SMOKE_LOGICAL,
    SMOKE_MATCH_INT,
    SMOKE_MATCH_IDENT,
    SMOKE_MATCH_BOOL,
    SMOKE_STRUCT_POINT,
    SMOKE_STRUCT_ASSIGN,
    SMOKE_ENUM_MATCH,
    SMOKE_ENUM_MATCH_STRUCT,
    SMOKE_STRUCT_METHOD,
    SMOKE_CAST_WIDTH,
    SMOKE_COMPARE_UNARY,
    SMOKE_DEEP_LOGICAL_CHAIN,
    SMOKE_DEEP_LOGICAL_OR_CHAIN,
    SMOKE_MOD_BITWISE,
    SMOKE_ARRAY_INDEX,
    SMOKE_TUPLE_LIT,
    SMOKE_IF_CONST_STRUCT,
    SMOKE_TRAIT_EQ,
    SMOKE_TRAIT_INHERENT,
    SMOKE_PRIMITIVES_FLOAT,
    SMOKE_PRIMITIVES_WIDTH,
    SMOKE_PRIMITIVES_I128,
    SMOKE_PRIMITIVES_U128,
    SMOKE_SHIFT_WIDTH_MASK,
    SMOKE_BYTE_STRING,
    SMOKE_STRING_LITERAL,
    SMOKE_BYTE_STRING_AS_STR,
    SMOKE_REF_LOCAL,
    SMOKE_REF_FN_PARAM,
    SMOKE_MUT_REF_LOCAL,
    SMOKE_DEREF_PTR,
    SMOKE_SLICE_FROM_ARRAY,
    SMOKE_FACTORIAL,
    SMOKE_IF_CONST_ENUM_SINGLE_VARIANT,
    SMOKE_IF_CONST_ENUM_NON_EXHAUSTIVE,
    SMOKE_IF_VAR_REASSIGN,
    SMOKE_IF_CONST_ELSE,
    SMOKE_GENERIC_FN,
    SMOKE_GENERIC_STRUCT,
    SMOKE_GENERIC_ENUM,
    SMOKE_GENERIC_INFER,
    SMOKE_GENERIC_ENUM_INFER,
    SMOKE_GENERIC_ENUM_MATCH,
    SMOKE_GENERIC_IMPL_METHOD,
    SMOKE_FN_POINTER,
    SMOKE_DROP,
    SMOKE_DERIVE_PARTIALEQ,
    SMOKE_DERIVE_ENUM_PARTIALEQ,
    SMOKE_DERIVE_GENERIC_STRUCT,
    SMOKE_DERIVE_GENERIC_ENUM,
    SMOKE_ATTR_BRACKET_DERIVE,
    SMOKE_MILLIMETERS,
    SMOKE_TUPLE_STRUCT_TWO_FIELD,
];
