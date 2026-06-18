//! Single-file programs at fixture root.

use super::SingleFile;

pub const SINGLE_ARRAY_INDEX: SingleFile = SingleFile {
    name: r"array_index.phx",
    source: r"main :: () => {
    const arr: [s32; 3] = [10, 20, 30];
    const v: s32 = arr[1];
    const _ = v;
};
",
};

pub const SINGLE_ATTR_BRACKET_DERIVE: SingleFile = SingleFile {
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

pub const SINGLE_ATTR_CFG_STRIP: SingleFile = SingleFile {
    name: r"attr_cfg_strip.phx",
    source: r#"#[cfg(target_os = "no_such_os")]
pub phantom :: () => s32 {
  99
};

main :: () => {
  const x = 7;
};
"#,
};

pub const SINGLE_ATTR_DEPRECATED_ALLOW: SingleFile = SingleFile {
    name: r"attr_deprecated_allow.phx",
    source: r#"#[deprecated(since = "0.1.0", note = "use new_fn instead", suggestion = "new_fn")]
old_fn :: () => () {
};

new_fn :: () => () {
};

#[allow(deprecated)]
main :: () => {
  old_fn();
};
"#,
};

pub const SINGLE_ATTR_DEPRECATED_WARN: SingleFile = SingleFile {
    name: r"attr_deprecated_warn.phx",
    source: r#"#[deprecated(note = "use new_fn instead")]
old_fn :: () => s32 {
  0
};

new_fn :: () => s32 {
  0
};

main :: () => {
  const _ = old_fn();
};
"#,
};

pub const SINGLE_ATTR_MUST_USE: SingleFile = SingleFile {
    name: r"attr_must_use.phx",
    source: r"#[must_use]
pub Box :: struct {
  value: s32,
};

main :: () => {
  Box { value: 1 };
  const _ = 0;
};
",
};

pub const SINGLE_BAD_TYPE: SingleFile = SingleFile {
    name: r"bad_type.phx",
    source: r"// Should fail type-check: bool assigned to s32.

Random :: enum {};

// Random :: struct {};

main :: () => {
  const x: Random = true;
};
",
};

pub const SINGLE_BYTE_STRING: SingleFile = SingleFile {
    name: r"byte_string.phx",
    source: r#"main :: () => {
    const s: [u8; 3] = b"ABC";
    const v: u8 = s[1];
    const _ = v;
};
"#,
};

pub const SINGLE_BYTE_STRING_AS_STR: SingleFile = SingleFile {
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

pub const SINGLE_CAST_WIDTH: SingleFile = SingleFile {
    name: r"cast_width.phx",
    source: r"main :: () => {
    const wide: s64 = 100 as s64;
    const narrow: u8 = 42 as u8;
    const bump: s64 = narrow as s64;
    const _ = wide + bump;
};
",
};

pub const SINGLE_COMPARE_UNARY: SingleFile = SingleFile {
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

pub const SINGLE_CONTINUE_IN_IF: SingleFile = SingleFile {
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

pub const SINGLE_CONTROL_FLOW: SingleFile = SingleFile {
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

pub const SINGLE_DEEP_LOGICAL_CHAIN: SingleFile = SingleFile {
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

pub const SINGLE_DEEP_LOGICAL_OR_CHAIN: SingleFile = SingleFile {
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

pub const SINGLE_DEFERRED_AT_SEND: SingleFile = SingleFile {
    name: r"deferred_at_send.phx",
    source: r"main :: () => {
    @send(1, 2);
};
",
};

pub const SINGLE_DEFERRED_BREAK_VALUE: SingleFile = SingleFile {
    name: r"deferred_break_value.phx",
    source: r"main :: () => {
    loop {
        break 1;
    };
};
",
};

pub const SINGLE_DEREF_PTR: SingleFile = SingleFile {
    name: r"deref_ptr.phx",
    source: r"main :: () => {
    var cell: u8 = 77 as u8;
    const addr: &u8 = &cell;
    const v: u8 = *addr;
    const _ = v;
};
",
};

pub const SINGLE_DERIVE_BAD: SingleFile = SingleFile {
    name: r"derive_bad.phx",
    source: r"Point :: struct {
    x: s32,
};

#[derive(Clone)]
Point :: struct {
    x: s32,
};

main :: () => { };
",
};

pub const SINGLE_DERIVE_DYNAMIC_ARRAY_SHAPE: SingleFile = SingleFile {
    name: r"derive_dynamic_array_shape.phx",
    source: r"PartialEq :: trait {
    eq :: (self: &Self, other: &Self) => bool;
};

Debug :: trait {
    fmt :: (self: &Self) => [u8; 32];
};

Drop :: trait {
    drop :: (self) => ();
};

#[derive(PartialEq, Debug)]
DynamicArray :: <t> struct {
    len: u32,
    cap: u32,
};

DynamicArray :: <t> impl :: Drop {
    drop :: (self) => () {};
};

main :: () => {
    const a = DynamicArray :: <s32> { len: 1u, cap: 4u };
    const b = DynamicArray :: <s32> { len: 1u, cap: 4u };
    const _: bool = a.eq(&b);
};
",
};

pub const SINGLE_DERIVE_ENUM_PARTIALEQ: SingleFile = SingleFile {
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

pub const SINGLE_DERIVE_GENERIC_BAD: SingleFile = SingleFile {
    name: r"derive_generic_bad.phx",
    source: r"Copyable :: trait {};

#[derive(Copyable)]
Box :: <t> struct {
    v: t,
};

Holder :: struct {
    r: &s32,
};

max :: <t: Copyable> (a: t, b: t) => t {
    a
};

main :: () => {
    var n: s32 = 1;
    const left = Box :: <Holder> { v: Holder { r: &n } };
    const right = Box :: <Holder> { v: Holder { r: &n } };
    const _unused = max(left, right);
};
",
};

pub const SINGLE_DERIVE_GENERIC_ENUM: SingleFile = SingleFile {
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

pub const SINGLE_DERIVE_GENERIC_STRUCT: SingleFile = SingleFile {
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

pub const SINGLE_DERIVE_PARTIALEQ: SingleFile = SingleFile {
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

pub const SINGLE_DROP: SingleFile = SingleFile {
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

pub const SINGLE_DROP_DOUBLE: SingleFile = SingleFile {
    name: r"drop_double.phx",
    source: r"Drop :: trait {
    drop :: (self) => ();
};

Wrapper :: struct {};

Wrapper :: impl :: Drop {
    drop :: (self) => () {};
};

main :: () => {
    const w = Wrapper {};
    w.drop();
    w.drop();
};
",
};

pub const SINGLE_DROP_USE_AFTER: SingleFile = SingleFile {
    name: r"drop_use_after.phx",
    source: r"Drop :: trait {
    drop :: (self) => ();
};

Wrapper :: struct {};

Wrapper :: impl :: Drop {
    drop :: (self) => () {};
};

main :: () => {
    const w = Wrapper {};
    w.drop();
    const _ = w;
};
",
};

pub const SINGLE_ENUM_MATCH: SingleFile = SingleFile {
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

pub const SINGLE_ENUM_MATCH_STRUCT: SingleFile = SingleFile {
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

pub const SINGLE_EXTERN_UNSAFE: SingleFile = SingleFile {
    name: r"extern_unsafe.phx",
    source: r#"// V0-053: extern "C" calls require unsafe.

extern "C" c_add :: (a: s32, b: s32) => s32;

main :: () => {
    c_add(10, 2);
};
"#,
};

pub const SINGLE_FACTORIAL: SingleFile = SingleFile {
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

pub const SINGLE_FN_POINTER: SingleFile = SingleFile {
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

pub const SINGLE_FOR_IN: SingleFile = SingleFile {
    name: r"for_in.phx",
    source: r"Option :: <t> enum {
    None,
    Some(t),
};

IntoIter :: trait {
    type Item;
    type IntoIter;
    into_iter :: (self) => Self::IntoIter;
};

Iterator :: trait {
    type Item;
    next :: (self: &mut Self) => Option<Self::Item>;
};

Range :: struct {
    start: s32,
    end: s32,
};

RangeIter :: struct {
    cur: s32,
    end: s32,
};

Range :: impl :: IntoIter {
    type Item = s32;
    type IntoIter = RangeIter;
    into_iter :: (self) => RangeIter {
        RangeIter { cur: self.start, end: self.end }
    };
};

RangeIter :: impl :: Iterator {
    type Item = s32;
    next :: (self: &mut Self) => Option<s32> {
        const cur = self.cur;
        const end = self.end;
        if cur < end {
            self.cur = cur + 1;
            Some :: <s32> (cur)
        } else {
            None :: <s32> ()
        }
    };
};

main :: () => {
    var sum: s32 = 0;
    for x in Range { start: 0, end: 3 } {
        sum = sum + x;
    };
    const _ = sum;
};
",
};

pub const SINGLE_FOR_IN_BAD: SingleFile = SingleFile {
    name: r"for_in_bad.phx",
    source: r"main :: () => {
    for x in 0 {
    };
};
",
};

pub const SINGLE_GENERIC_ENUM: SingleFile = SingleFile {
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

pub const SINGLE_GENERIC_ENUM_INFER: SingleFile = SingleFile {
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

pub const SINGLE_GENERIC_ENUM_MATCH: SingleFile = SingleFile {
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

pub const SINGLE_GENERIC_FN: SingleFile = SingleFile {
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

pub const SINGLE_GENERIC_IMPL_METHOD: SingleFile = SingleFile {
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

pub const SINGLE_GENERIC_INFER: SingleFile = SingleFile {
    name: r"generic_infer.phx",
    source: r"id :: <t> (x: t) => t { x };

main :: () => {
  const n: s32 = id(42);
  const _ = n;
};
",
};

pub const SINGLE_GENERIC_STRUCT: SingleFile = SingleFile {
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

pub const SINGLE_IF_CONST_ELSE: SingleFile = SingleFile {
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

pub const SINGLE_IF_CONST_ENUM_NON_EXHAUSTIVE: SingleFile = SingleFile {
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

pub const SINGLE_IF_CONST_ENUM_SINGLE_VARIANT: SingleFile = SingleFile {
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

pub const SINGLE_IF_CONST_STRUCT: SingleFile = SingleFile {
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

pub const SINGLE_IF_VAR_REASSIGN: SingleFile = SingleFile {
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

pub const SINGLE_INVALID_UTF8_BYTE_AS_STR: SingleFile = SingleFile {
    name: r"invalid_utf8_byte_as_str.phx",
    source: r#"// Should fail type-check: invalid UTF-8 byte string cannot cast to str.
main :: () => {
    const _ = b"\xFF" as str;
};
"#,
};

pub const SINGLE_LOGICAL: SingleFile = SingleFile {
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

pub const SINGLE_MATCH_BOOL: SingleFile = SingleFile {
    name: r"match_bool.phx",
    source: r"// match on bool (via if-style literals in arms).

main :: () => {
  const flag: bool = { if 1 == 1 { true } else { false } };
  const x: s32 = { match flag { true => 1; false => 0; _ => 2; } };
  const _ = x;
};
",
};

pub const SINGLE_MATCH_IDENT: SingleFile = SingleFile {
    name: r"match_ident.phx",
    source: r"main :: () => {
    var n: s32 = 1;
    const x: s32 = match n { 0 => 10; _ => 20; };
    const _ = x;
};
",
};

pub const SINGLE_MATCH_INT: SingleFile = SingleFile {
    name: r"match_int.phx",
    source: r"// match on s32 with literal and wildcard arms.

main :: () => {
  var n: s32 = 2;
  const x: s32 = { match n { 1 => 10; 2 => 20; _ => 30; } };
  const _ = x;
};
",
};

pub const SINGLE_MATCH_UNREACHABLE_ARM: SingleFile = SingleFile {
    name: r"match_unreachable_arm.phx",
    source: r"main :: () => {
    const x: s32 = match 0 {
        _ => 1;
        2 => 2;
    };
    const _ = x;
};
",
};

pub const SINGLE_MILLIMETERS: SingleFile = SingleFile {
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

pub const SINGLE_MISSING_MAIN: SingleFile = SingleFile {
    name: r"missing_main.phx",
    source: r"// Resolve failure: no `main` entry point.

add :: (a: s32, b: s32) => s32 {
    a + b
};
",
};

pub const SINGLE_MIXED_WIDTH: SingleFile = SingleFile {
    name: r"mixed_width.phx",
    source: r"main :: () => {
    const a: s32 = 1;
    const b: s64 = 2 as s64;
    const _ = a + b;
};
",
};

pub const SINGLE_MOD_BITWISE: SingleFile = SingleFile {
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

pub const SINGLE_MUT_REF_LOCAL: SingleFile = SingleFile {
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

pub const SINGLE_NEWTYPE_BAD: SingleFile = SingleFile {
    name: r"newtype_bad.phx",
    source: r"Millimeters :: struct(s32);

main :: () => {
    const m: Millimeters = Millimeters(500);
    const bad: s32 = m;
    const _ = bad;
};
",
};

pub const SINGLE_OPTION_IF_ELSE: SingleFile = SingleFile {
    name: r"option_if_else.phx",
    source: r"Option :: <t> enum {
    None,
    Some(t),
};

pick :: () => Option<s32> {
    if 1 < 0 {
        Some(1)
    } else {
        None
    }
};

main :: () => { };
",
};

pub const SINGLE_OPTION_MATCH: SingleFile = SingleFile {
    name: r"option_match.phx",
    source: r"Option :: <t> enum {
    None,
    Some(t),
};

pick :: () => Option<s32> {
    match true {
        true => Some :: <s32> (1);
        false => None :: <s32> ();
        _ => None :: <s32> ();
    }
};

main :: () => { };
",
};

pub const SINGLE_PRIMITIVES_FLOAT: SingleFile = SingleFile {
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

pub const SINGLE_PRIMITIVES_I128: SingleFile = SingleFile {
    name: r"primitives_i128.phx",
    source: r"main :: () => {
    const big: s128 = 1000 as s128;
    const small: s8 = big as s8;
    const _ = small;
};
",
};

pub const SINGLE_PRIMITIVES_U128: SingleFile = SingleFile {
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

pub const SINGLE_PRIMITIVES_WIDTH: SingleFile = SingleFile {
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

pub const SINGLE_REF_FN_PARAM: SingleFile = SingleFile {
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

pub const SINGLE_REF_LOCAL: SingleFile = SingleFile {
    name: r"ref_local.phx",
    source: r"main :: () => {
    var x: s32 = 10;
    const p: &s32 = &x;
    const v: s32 = *p;
    const _ = v;
};
",
};

pub const SINGLE_SAMPLE: SingleFile = SingleFile {
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

pub const SINGLE_SELF_MUT_TRAIT: SingleFile = SingleFile {
    name: r"self_mut_trait.phx",
    source: r"PartialEq :: trait {
    eq :: (self: &Self, other: &Self) => bool;
};

Point :: struct { x: s32 };

Point :: impl :: PartialEq {
    eq :: (self: &Self, other: &Self) => bool {
        self.x == other.x
    };
};

main :: () => { };
",
};

pub const SINGLE_SHIFT_WIDTH_MASK: SingleFile = SingleFile {
    name: r"shift_width_mask.phx",
    source: r"main :: () => {
    const u8_shift: u8 = (1u as u8) << (9 as u8);
    const u32_shift: u32 = (1u as u32) << (32 as u32);
};
",
};

pub const SINGLE_SLICE_FROM_ARRAY: SingleFile = SingleFile {
    name: r"slice_from_array.phx",
    source: r#"main :: () => {
    const arr: [u8; 4] = b"WXYZ";
    const sl: [u8] = arr as [u8];
    const v: u8 = sl[2];
    const _ = v;
};
"#,
};

pub const SINGLE_STRING_LITERAL: SingleFile = SingleFile {
    name: r"string_literal.phx",
    source: r#"main :: () => {
    const s: str = "hello";
    const sl: [u8] = s as [u8];
    const v: u8 = sl[4];
    const _ = v;
};
"#,
};

pub const SINGLE_STRUCT_ASSIGN: SingleFile = SingleFile {
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

pub const SINGLE_STRUCT_METHOD: SingleFile = SingleFile {
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

pub const SINGLE_STRUCT_POINT: SingleFile = SingleFile {
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

pub const SINGLE_TRAIT_EQ: SingleFile = SingleFile {
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

pub const SINGLE_TRAIT_IMPL_INCOMPLETE: SingleFile = SingleFile {
    name: r"trait_impl_incomplete.phx",
    source: r"PartialEq :: trait {
    eq :: (self: Point, other: Point) => bool;
};

Point :: struct {
    x: s32,
    y: s32,
};

Point :: impl :: PartialEq {
};

main :: () => {
    const _ = 0;
};
",
};

pub const SINGLE_TRAIT_INHERENT: SingleFile = SingleFile {
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

pub const SINGLE_TUPLE_LIT: SingleFile = SingleFile {
    name: r"tuple_lit.phx",
    source: r"main :: () => {
    const pair: (s32, s32) = (1, 2);
    const v: s32 = pair[0];
    const _ = v;
};
",
};

pub const SINGLE_TUPLE_STRUCT_TWO_FIELD: SingleFile = SingleFile {
    name: r"tuple_struct_two_field.phx",
    source: r"Point :: struct(s32, s32);

main :: () => {
    const p: Point = Point(1, 2);
    const x: s32 = p.0;
    const _ = x;
};
",
};

pub const SINGLE_USE_AFTER_MOVE: SingleFile = SingleFile {
    name: r"use_after_move.phx",
    source: r"Point :: struct {
    r: &s32,
};

main :: () => {
    var n: s32 = 1;
    var p: Point = Point { r: &n };
    var q: Point = p;
    const _ = p.r;
};
",
};

pub const SINGLE_FILES: &[SingleFile] = &[
    SINGLE_ARRAY_INDEX,
    SINGLE_ATTR_BRACKET_DERIVE,
    SINGLE_ATTR_CFG_STRIP,
    SINGLE_ATTR_DEPRECATED_ALLOW,
    SINGLE_ATTR_DEPRECATED_WARN,
    SINGLE_ATTR_MUST_USE,
    SINGLE_BAD_TYPE,
    SINGLE_BYTE_STRING,
    SINGLE_BYTE_STRING_AS_STR,
    SINGLE_CAST_WIDTH,
    SINGLE_COMPARE_UNARY,
    SINGLE_CONTINUE_IN_IF,
    SINGLE_CONTROL_FLOW,
    SINGLE_DEEP_LOGICAL_CHAIN,
    SINGLE_DEEP_LOGICAL_OR_CHAIN,
    SINGLE_DEFERRED_AT_SEND,
    SINGLE_DEFERRED_BREAK_VALUE,
    SINGLE_DEREF_PTR,
    SINGLE_DERIVE_BAD,
    SINGLE_DERIVE_DYNAMIC_ARRAY_SHAPE,
    SINGLE_DERIVE_ENUM_PARTIALEQ,
    SINGLE_DERIVE_GENERIC_BAD,
    SINGLE_DERIVE_GENERIC_ENUM,
    SINGLE_DERIVE_GENERIC_STRUCT,
    SINGLE_DERIVE_PARTIALEQ,
    SINGLE_DROP,
    SINGLE_DROP_DOUBLE,
    SINGLE_DROP_USE_AFTER,
    SINGLE_ENUM_MATCH,
    SINGLE_ENUM_MATCH_STRUCT,
    SINGLE_EXTERN_UNSAFE,
    SINGLE_FACTORIAL,
    SINGLE_FN_POINTER,
    SINGLE_FOR_IN,
    SINGLE_FOR_IN_BAD,
    SINGLE_GENERIC_ENUM,
    SINGLE_GENERIC_ENUM_INFER,
    SINGLE_GENERIC_ENUM_MATCH,
    SINGLE_GENERIC_FN,
    SINGLE_GENERIC_IMPL_METHOD,
    SINGLE_GENERIC_INFER,
    SINGLE_GENERIC_STRUCT,
    SINGLE_IF_CONST_ELSE,
    SINGLE_IF_CONST_ENUM_NON_EXHAUSTIVE,
    SINGLE_IF_CONST_ENUM_SINGLE_VARIANT,
    SINGLE_IF_CONST_STRUCT,
    SINGLE_IF_VAR_REASSIGN,
    SINGLE_INVALID_UTF8_BYTE_AS_STR,
    SINGLE_LOGICAL,
    SINGLE_MATCH_BOOL,
    SINGLE_MATCH_IDENT,
    SINGLE_MATCH_INT,
    SINGLE_MATCH_UNREACHABLE_ARM,
    SINGLE_MILLIMETERS,
    SINGLE_MISSING_MAIN,
    SINGLE_MIXED_WIDTH,
    SINGLE_MOD_BITWISE,
    SINGLE_MUT_REF_LOCAL,
    SINGLE_NEWTYPE_BAD,
    SINGLE_OPTION_IF_ELSE,
    SINGLE_OPTION_MATCH,
    SINGLE_PRIMITIVES_FLOAT,
    SINGLE_PRIMITIVES_I128,
    SINGLE_PRIMITIVES_U128,
    SINGLE_PRIMITIVES_WIDTH,
    SINGLE_REF_FN_PARAM,
    SINGLE_REF_LOCAL,
    SINGLE_SAMPLE,
    SINGLE_SELF_MUT_TRAIT,
    SINGLE_SHIFT_WIDTH_MASK,
    SINGLE_SLICE_FROM_ARRAY,
    SINGLE_STRING_LITERAL,
    SINGLE_STRUCT_ASSIGN,
    SINGLE_STRUCT_METHOD,
    SINGLE_STRUCT_POINT,
    SINGLE_TRAIT_EQ,
    SINGLE_TRAIT_IMPL_INCOMPLETE,
    SINGLE_TRAIT_INHERENT,
    SINGLE_TUPLE_LIT,
    SINGLE_TUPLE_STRUCT_TWO_FIELD,
    SINGLE_USE_AFTER_MOVE,
];

pub fn single_by_name(name: &str) -> Option<&'static SingleFile> {
    SINGLE_FILES.iter().find(|f| f.name == name)
}
