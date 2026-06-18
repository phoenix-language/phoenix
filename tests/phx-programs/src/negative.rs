//! Negative check programs.

use super::NegativeCase;

use super::single::*;

pub const NEG_BAD_TYPE: NegativeCase = NegativeCase {
    name: r"bad_type.phx",
    source: r"// Should fail type-check: bool assigned to s32.

Random :: enum {};

// Random :: struct {};

main :: () => {
  const x: Random = true;
};
",
    needle: r"type mismatch",
};

pub const NEG_MISSING_MAIN: NegativeCase = NegativeCase {
    name: r"missing_main.phx",
    source: r"// Resolve failure: no `main` entry point.

add :: (a: s32, b: s32) => s32 {
    a + b
};
",
    needle: r"main",
};

pub const NEG_USE_AFTER_MOVE: NegativeCase = NegativeCase {
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
    needle: r"moved",
};

pub const NEG_MIXED_WIDTH: NegativeCase = NegativeCase {
    name: r"mixed_width.phx",
    source: r"main :: () => {
    const a: s32 = 1;
    const b: s64 = 2 as s64;
    const _ = a + b;
};
",
    needle: r"invalid",
};

pub const NEG_INVALID_UTF8_BYTE_AS_STR: NegativeCase = NegativeCase {
    name: r"invalid_utf8_byte_as_str.phx",
    source: r#"// Should fail type-check: invalid UTF-8 byte string cannot cast to str.
main :: () => {
    const _ = b"\xFF" as str;
};
"#,
    needle: r"invalid cast",
};

pub const NEG_MATCH_UNREACHABLE_ARM: NegativeCase = NegativeCase {
    name: r"match_unreachable_arm.phx",
    source: r"main :: () => {
    const x: s32 = match 0 {
        _ => 1;
        2 => 2;
    };
    const _ = x;
};
",
    needle: r"unreachable",
};

pub const NEG_TRAIT_IMPL_INCOMPLETE: NegativeCase = NegativeCase {
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
    needle: r"trait method",
};

pub const NEG_DEFERRED_BREAK_VALUE: NegativeCase = NegativeCase {
    name: r"deferred_break_value.phx",
    source: r"main :: () => {
    loop {
        break 1;
    };
};
",
    needle: r"break",
};

pub const NEG_DEFERRED_AT_SEND: NegativeCase = NegativeCase {
    name: r"deferred_at_send.phx",
    source: r"main :: () => {
    @send(1, 2);
};
",
    needle: r"@send",
};

pub const NEG_EXTERN_UNSAFE: NegativeCase = NegativeCase {
    name: r"extern_unsafe.phx",
    source: r#"// V0-053: extern "C" calls require unsafe.

extern "C" c_add :: (a: s32, b: s32) => s32;

main :: () => {
    c_add(10, 2);
};
"#,
    needle: r"unsafe",
};

pub const NEG_FOR_IN_BAD: NegativeCase = NegativeCase {
    name: r"for_in_bad.phx",
    source: r"main :: () => {
    for x in 0 {
    };
};
",
    needle: r"IntoIter",
};

pub const NEG_DERIVE_BAD: NegativeCase = NegativeCase {
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
    needle: r"unsupported derive trait",
};

pub const NEG_DERIVE_GENERIC_BAD: NegativeCase = NegativeCase {
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
    needle: r"trait bound",
};

pub const NEG_NEWTYPE_BAD: NegativeCase = NegativeCase {
    name: r"newtype_bad.phx",
    source: r"Millimeters :: struct(s32);

main :: () => {
    const m: Millimeters = Millimeters(500);
    const bad: s32 = m;
    const _ = bad;
};
",
    needle: r"type mismatch",
};

pub const NEGATIVE_CASES: &[NegativeCase] = &[
    NEG_BAD_TYPE,
    NEG_MISSING_MAIN,
    NEG_USE_AFTER_MOVE,
    NEG_MIXED_WIDTH,
    NEG_INVALID_UTF8_BYTE_AS_STR,
    NEG_MATCH_UNREACHABLE_ARM,
    NEG_TRAIT_IMPL_INCOMPLETE,
    NEG_DEFERRED_BREAK_VALUE,
    NEG_DEFERRED_AT_SEND,
    NEG_EXTERN_UNSAFE,
    NEG_FOR_IN_BAD,
    NEG_DERIVE_BAD,
    NEG_DERIVE_GENERIC_BAD,
    NEG_NEWTYPE_BAD,
];
