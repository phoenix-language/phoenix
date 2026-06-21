//! Golden diagnostic cases.

use super::{DiagnosticCase, DiagnosticKind};

use super::single::*;

use super::modules::*;

use super::projects::*;

pub const DIAG_BAD_TYPE: DiagnosticCase = DiagnosticCase {
    name: r"bad_type",
    kind: DiagnosticKind::SingleFile,
    source: None,
    expected: r"error[E2001]: type mismatch: expected enum Random, found Bool
  --> tests/cli/fixtures/bad_type.phx:8:21
  |
8 |   const x: Random = true;
  |                     ^^^^
   = note: expected `enum Random` due to type annotation on `const x`
   = help: change the `const` type annotation to `Bool`, or change the initializer to produce `enum Random`
   = help: or remove the type annotation and let the initializer determine the type",
};

pub const DIAG_USE_AFTER_MOVE: DiagnosticCase = DiagnosticCase {
    name: r"use_after_move",
    kind: DiagnosticKind::SingleFile,
    source: None,
    expected: r"error[E2017]: use of moved value `p`
  --> tests/cli/fixtures/use_after_move.phx:9:15
  |
9 |     const _ = p.r;
  |               ^
   = note: value `p` was moved here
  --> tests/cli/fixtures/use_after_move.phx:8:20
  |
8 |     var q: Point = p;
  |                    ^
   = help: use `p` only before it is moved, or bind a new value after the move",
};

pub const DIAG_MISSING_MAIN: DiagnosticCase = DiagnosticCase {
    name: r"missing_main",
    kind: DiagnosticKind::SingleFile,
    source: None,
    expected: r"error[E1013]: missing entry function `main`
  --> tests/cli/fixtures/missing_main.phx:3:1
  |
3 | add :: (a: s32, b: s32) => s32 {
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^",
};

pub const DIAG_IMPORT_CYCLE: DiagnosticCase = DiagnosticCase {
    name: r"import_cycle",
    kind: DiagnosticKind::ModuleTree {
        tree_name: r"cycle_a",
    },
    source: None,
    expected: r"error[E1008]: circular module import: modules::cycle_a → modules::cycle_b → modules::cycle_a
  --> tests/cli/fixtures/modules/cycle_a.phx:1:1
  |
1 | #import cycle_b::helper;
  | ^^^^^^^^^^^^^^^^^^^^^^^^",
};

const DIAG_MULTI_RESOLVE_DUPLICATE_SOURCE: &str = r"main :: () => { };
foo :: () => { };
foo :: () => { };
bar :: () => { };
bar :: () => { };
";

pub const DIAG_MULTI_RESOLVE_DUPLICATE: DiagnosticCase = DiagnosticCase {
    name: r"multi_resolve_duplicate",
    kind: DiagnosticKind::CompileSource,
    source: Some(DIAG_MULTI_RESOLVE_DUPLICATE_SOURCE),
    expected: r"error[E1003]: duplicate definition of `foo`
  --> <entry>:3:1
  |
3 | foo :: () => { };
  | ^^^^^^^^^^^^^^^^^
   = note: previous definition here
  --> <entry>:2:1
  |
2 | foo :: () => { };
  | ^^^^^^^^^^^^^^^^^

error[E1003]: duplicate definition of `bar`
  --> <entry>:5:1
  |
5 | bar :: () => { };
  | ^^^^^^^^^^^^^^^^^
   = note: previous definition here
  --> <entry>:4:1
  |
4 | bar :: () => { };
  | ^^^^^^^^^^^^^^^^^
error: aborting due to 2 previous errors",
};

const DIAG_RETURN_LOCAL_STR_SOURCE: &str = r#"bad :: () => str {
    var arr: [u8; 3] = b"ABC";
    return arr as str;
};

main :: () => {
    const _ = bad();
};
"#;

pub const DIAG_RETURN_LOCAL_STR: DiagnosticCase = DiagnosticCase {
    name: r"return_local_str",
    kind: DiagnosticKind::SingleFile,
    source: Some(DIAG_RETURN_LOCAL_STR_SOURCE),
    expected: r"error[E2014]: invalid cast from `[U8; 3]` to `str`
  --> tests/integration/diagnostics/return_local_str.phx:3:12
  |
3 |     return arr as str;
  |            ^^^^^^^^^^
   = help: explicit casts between `[U8; 3]` and `str` are not allowed in MVP; use a supported cast target (numeric primitives, array→slice, str→[u8])

error[E2022]: cannot return a borrow of a local variable
  --> tests/integration/diagnostics/return_local_str.phx:3:12
  |
3 |     return arr as str;
  |            ^^^^^^^^^^
   = note: borrow of local created here
   = help: return an owned value instead of a borrow, slice view, or `str` view of a local binding

error[E2022]: cannot return a borrow of a local variable
  --> tests/integration/diagnostics/return_local_str.phx:3:12
  |
3 |     return arr as str;
  |            ^^^^^^^^^^
   = note: borrow of local created here
   = help: return an owned value instead of a borrow, slice view, or `str` view of a local binding
error: aborting due to 3 previous errors",
};

const DIAG_POW_UNSUPPORTED_SOURCE: &str = r"main :: () => {
    const _ = 2 ** 3;
};
";

pub const DIAG_POW_UNSUPPORTED: DiagnosticCase = DiagnosticCase {
    name: r"pow_unsupported",
    kind: DiagnosticKind::SingleFile,
    source: Some(DIAG_POW_UNSUPPORTED_SOURCE),
    expected: r"error[E2016]: integer power (`**`) is not available in MVP
  --> tests/integration/diagnostics/pow_unsupported.phx:2:15
  |
2 |     const _ = 2 ** 3;
  |               ^^^^^^
   = help: `integer power (`**`)` is not implemented in the MVP compiler yet",
};

const DIAG_HASH_DERIVE_INVALID_SOURCE: &str = r"PartialEq :: trait {
    eq :: (self: &Self, other: &Self) => bool;
};

#derive(PartialEq)
Point :: struct {
    x: s32,
    y: s32,
};

main :: () => {};
";

pub const DIAG_HASH_DERIVE_INVALID: DiagnosticCase = DiagnosticCase {
    name: r"hash_derive_invalid",
    kind: DiagnosticKind::SingleFile,
    source: Some(DIAG_HASH_DERIVE_INVALID_SOURCE),
    expected: r"error[E1007]: unsupported syntax: `#derive(...)`; use `#[derive(...)]` on the item instead
  --> tests/integration/diagnostics/hash_derive_invalid.phx:5:1
  |
5 | #derive(PartialEq)
  | ^^^^^^^",
};

pub const DIAG_UNIQUE_PTR_USE_AFTER_MOVE: DiagnosticCase = DiagnosticCase {
    name: r"unique_ptr_use_after_move",
    kind: DiagnosticKind::Project {
        project_name: r"unique_ptr_move_in",
        entry: r"src/main.phx",
    },
    source: None,
    expected: r"error[E2017]: use of moved value `a`
  --> tests/cli/fixtures/unique_ptr_move_in/src/main.phx:8:15
  |
8 |     const _ = a.get();
  |               ^
   = note: value `a` was moved here
  --> tests/cli/fixtures/unique_ptr_move_in/src/main.phx:7:29
  |
7 |     var b: UniquePtr<s32> = a;
  |                             ^
   = help: use `a` only before it is moved, or bind a new value after the move",
};

pub const DIAG_TRAIT_IMPL_INCOMPLETE: DiagnosticCase = DiagnosticCase {
    name: r"trait_impl_incomplete",
    kind: DiagnosticKind::SingleFile,
    source: None,
    expected: r"error[E2026]: type `Point` does not implement trait method `eq` from `PartialEq`
  --> tests/cli/fixtures/trait_impl_incomplete.phx:10:1
   |
10 | Point :: impl :: PartialEq {
   | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   = help: add `fn eq(...)` to `Point :: impl :: PartialEq`",
};

pub const DIAG_EXTERN_UNSAFE: DiagnosticCase = DiagnosticCase {
    name: r"extern_unsafe",
    kind: DiagnosticKind::SingleFile,
    source: None,
    expected: r"error[E2032]: call to foreign function `c_add` requires `unsafe`
  --> tests/cli/fixtures/extern_unsafe.phx:6:5
  |
6 |     c_add(10, 2);
  |     ^^^^^^^^^^^^
   = help: wrap the call in `unsafe { ... }` or declare the enclosing function as `unsafe`

error[E2001]: type mismatch: expected (), found S32
  --> tests/cli/fixtures/extern_unsafe.phx:5:15
  |
5 | main :: () => {
  |               ^^^^^^^^^^^^^^^^^^^^^
   = note: function body must produce `()` to match the declared return type
   = help: change the expression to produce `()`, or adjust the expected type to `S32`
error: aborting due to 2 previous errors",
};

pub const DIAG_INVALID_UTF8_BYTE_AS_STR: DiagnosticCase = DiagnosticCase {
    name: r"invalid_utf8_byte_as_str",
    kind: DiagnosticKind::SingleFile,
    source: None,
    expected: r#"error[E2014]: invalid cast from `[U8; 1]` to `str`
  --> tests/cli/fixtures/invalid_utf8_byte_as_str.phx:3:15
  |
3 |     const _ = b"/xFF" as str;
  |               ^^^^^^^^^^^^^^
   = help: explicit casts between `[U8; 1]` and `str` are not allowed in MVP; use a supported cast target (numeric primitives, array→slice, str→[u8])"#,
};

pub const DIAG_MATCH_UNREACHABLE_ARM: DiagnosticCase = DiagnosticCase {
    name: r"match_unreachable_arm",
    kind: DiagnosticKind::SingleFile,
    source: None,
    expected: r"error[E2009]: unreachable `match` arm: a previous `_` arm matches all remaining values
  --> tests/cli/fixtures/match_unreachable_arm.phx:4:9
  |
4 |         2 => 2;
  |         ^
   = help: remove or reorder this arm so it can match before a broader arm",
};

pub const DIAG_TRY_OK_MISMATCH: DiagnosticCase = DiagnosticCase {
    name: r"try_ok_mismatch",
    kind: DiagnosticKind::Project {
        project_name: r"std_try_ok_mismatch",
        entry: r"src/main.phx",
    },
    source: None,
    expected: r"error[E2029]: cannot apply `?` to `Result<S32, S32>` in function returning `Result<Bool, S32>`
  --> tests/cli/fixtures/std_try_ok_mismatch/src/main.phx:8:15
  |
8 |     const x = read_a()?;
  |               ^^^^^^^^^
   = help: `?` requires a std `Option` or `Result` value matching the enclosing return type

error[E2001]: type mismatch: expected Bool, found ()
  --> tests/cli/fixtures/std_try_ok_mismatch/src/main.phx:9:8
  |
9 |     Ok(x)
  |        ^
   = note: argument 1 must be `Bool`
   = help: change the expression to produce `Bool`, or adjust the expected type to `()`
error: aborting due to 2 previous errors",
};

pub const DIAG_TRY_FROM_MISSING: DiagnosticCase = DiagnosticCase {
    name: r"try_from_missing",
    kind: DiagnosticKind::Project {
        project_name: r"std_try_from_missing",
        entry: r"src/main.phx",
    },
    source: None,
    expected: r"error[E2031]: cannot use `?` on `Result<_, IoError>` in function returning `Result<_, Error`: no `From<IoError>` implementation for `Error`
  --> tests/cli/fixtures/std_try_from_missing/src/main.phx:21:17
   |
21 |     const cfg = read_bytes()?;
   |                 ^^^^^^^^^^^^^
   = help: implement `From<IoError>` for `Error`

error[E2001]: type mismatch: expected struct Config, found ()
  --> tests/cli/fixtures/std_try_from_missing/src/main.phx:22:8
   |
22 |     Ok(cfg)
   |        ^^^
   = note: argument 1 must be `struct Config`
   = help: change the expression to produce `struct Config`, or adjust the expected type to `()`
error: aborting due to 2 previous errors",
};

pub const DIAG_DISCARDED_STD_RESULT: DiagnosticCase = DiagnosticCase {
    name: r"discarded_std_result",
    kind: DiagnosticKind::Project {
        project_name: r"lint_std_result_discard",
        entry: r"src/main.phx",
    },
    source: None,
    expected: r"error[E2041]: discarded `Result` value must be handled
  --> tests/cli/fixtures/lint_std_result_discard/src/main.phx:8:5
  |
8 |     fail();
  |     ^^^^^^
   = help: handle the value with `match`, `if const` / `if var`, or `?` inside a compatible return type",
};

pub const DIAG_DISCARDED_STD_OPTION: DiagnosticCase = DiagnosticCase {
    name: r"discarded_std_option",
    kind: DiagnosticKind::Project {
        project_name: r"lint_std_option_discard",
        entry: r"src/main.phx",
    },
    source: None,
    expected: r"error[E2042]: discarded `Option` value must be handled
  --> tests/cli/fixtures/lint_std_option_discard/src/main.phx:8:5
  |
8 |     maybe_one();
  |     ^^^^^^^^^^^
   = help: handle the value with `match`, `if const` / `if var`, or `?` inside a compatible return type",
};

const DIAG_LOOP_MOVE_USE_AFTER_LOOP_SOURCE: &str = r"Point :: struct { r: &s32 };

main :: () => {
    var n: s32 = 1;
    var p: Point = Point { r: &n };
    loop {
        var q: Point = p;
        break;
    };
    const _ = p.r;
};
";

pub const DIAG_LOOP_MOVE_USE_AFTER_LOOP: DiagnosticCase = DiagnosticCase {
    name: r"loop_move_use_after_loop",
    kind: DiagnosticKind::SingleFile,
    source: Some(DIAG_LOOP_MOVE_USE_AFTER_LOOP_SOURCE),
    expected: r"error[E2017]: use of moved value `p`
  --> tests/integration/diagnostics/loop_move_use_after_loop.phx:10:15
   |
10 |     const _ = p.r;
   |               ^
   = note: value `p` was moved here
  --> tests/integration/diagnostics/loop_move_use_after_loop.phx:7:24
  |
7 |         var q: Point = p;
  |                        ^
   = help: use `p` only before it is moved, or bind a new value after the move",
};

const DIAG_IF_BRANCH_SIBLING_NO_FALSE_UAM_SOURCE: &str = r"Point :: struct { r: &s32 };

main :: () => {
    var n: s32 = 1;
    var p: Point = Point { r: &n };
    var c: bool = true;
    if c {
        var q: Point = p;
    } else {
        const _ = p.r;
    };
    const _ = p.r;
};
";

pub const DIAG_IF_BRANCH_SIBLING_NO_FALSE_UAM: DiagnosticCase = DiagnosticCase {
    name: r"if_branch_sibling_no_false_uam",
    kind: DiagnosticKind::SingleFile,
    source: Some(DIAG_IF_BRANCH_SIBLING_NO_FALSE_UAM_SOURCE),
    expected: r"error[E2017]: use of moved value `p`
  --> tests/integration/diagnostics/if_branch_sibling_no_false_uam.phx:12:15
   |
12 |     const _ = p.r;
   |               ^
   = note: value `p` was moved here
  --> tests/integration/diagnostics/if_branch_sibling_no_false_uam.phx:8:24
  |
8 |         var q: Point = p;
  |                        ^
   = help: use `p` only before it is moved, or bind a new value after the move",
};

const DIAG_IF_BRANCH_UNTAKEN_NO_MOVE_SOURCE: &str = r"Point :: struct { r: &s32 };

main :: () => {
    var n: s32 = 1;
    var p: Point = Point { r: &n };
    var c: bool = true;
    if c {
        const _ = p.r;
    } else {
        var q: Point = p;
    };
    const _ = p.r;
};
";

pub const DIAG_IF_BRANCH_UNTAKEN_NO_MOVE: DiagnosticCase = DiagnosticCase {
    name: r"if_branch_untaken_no_move",
    kind: DiagnosticKind::SingleFile,
    source: Some(DIAG_IF_BRANCH_UNTAKEN_NO_MOVE_SOURCE),
    expected: r"error[E2017]: use of moved value `p`
  --> tests/integration/diagnostics/if_branch_untaken_no_move.phx:12:15
   |
12 |     const _ = p.r;
   |               ^
   = note: value `p` was moved here
  --> tests/integration/diagnostics/if_branch_untaken_no_move.phx:10:24
   |
10 |         var q: Point = p;
   |                        ^
   = help: use `p` only before it is moved, or bind a new value after the move",
};

const DIAG_INVALID_CAST_SOURCE: &str = r"// Should fail type-check: numeric literal cannot cast to bool in MVP.
main :: () => {
    const _ = 1 as bool;
};
";

pub const DIAG_INVALID_CAST: DiagnosticCase = DiagnosticCase {
    name: r"invalid_cast",
    kind: DiagnosticKind::SingleFile,
    source: Some(DIAG_INVALID_CAST_SOURCE),
    expected: r"error[E2014]: invalid cast from `S32` to `Bool`
  --> tests/integration/diagnostics/invalid_cast.phx:3:15
  |
3 |     const _ = 1 as bool;
  |               ^^^^^^^^^
   = help: explicit casts between `S32` and `Bool` are not allowed in MVP; use a supported cast target (numeric primitives, array→slice, str→[u8])",
};

const DIAG_DOUBLE_MUT_BORROW_SOURCE: &str = r"main :: () => {
    var x: s32 = 1;
    const a = &mut x;
    const b = &mut x;
    const _ = ();
};
";

pub const DIAG_DOUBLE_MUT_BORROW: DiagnosticCase = DiagnosticCase {
    name: r"double_mut_borrow",
    kind: DiagnosticKind::SingleFile,
    source: Some(DIAG_DOUBLE_MUT_BORROW_SOURCE),
    expected: r"error[E2047]: cannot borrow `x` as mutable more than once
  --> tests/integration/diagnostics/double_mut_borrow.phx:4:15
  |
4 |     const b = &mut x;
  |               ^^^^^^
   = note: `x` was mutably borrowed here
  --> tests/integration/diagnostics/double_mut_borrow.phx:3:15
  |
3 |     const a = &mut x;
  |               ^^^^^^
   = help: finish using the first `&mut x` borrow before creating another",
};
const DIAG_SHARED_MUT_BORROW_SOURCE: &str = r"main :: () => {
    var x: s32 = 1;
    const a = &x;
    const b = &mut x;
    const _ = ();
};
";

pub const DIAG_SHARED_MUT_BORROW: DiagnosticCase = DiagnosticCase {
    name: r"shared_mut_borrow",
    kind: DiagnosticKind::SingleFile,
    source: Some(DIAG_SHARED_MUT_BORROW_SOURCE),
    expected: r"error[E2048]: cannot borrow `x` as mutable while it is borrowed
  --> tests/integration/diagnostics/shared_mut_borrow.phx:4:15
  |
4 |     const b = &mut x;
  |               ^^^^^^
   = note: `x` was borrowed here
  --> tests/integration/diagnostics/shared_mut_borrow.phx:3:15
  |
3 |     const a = &x;
  |               ^^
   = help: finish using shared borrows of `x` before creating `&mut x`",
};
const DIAG_IF_ARM_OVERLAPPING_MUT_SOURCE: &str = r"main :: () => {
    var x: s32 = 1;
    var c: bool = true;
    if c {
        const _a = &mut x;
    } else {
        const _b = &mut x;
    };
    const _ = ();
};
";

pub const DIAG_IF_ARM_OVERLAPPING_MUT: DiagnosticCase = DiagnosticCase {
    name: r"if_arm_overlapping_mut",
    kind: DiagnosticKind::SingleFile,
    source: Some(DIAG_IF_ARM_OVERLAPPING_MUT_SOURCE),
    expected: r"error[E2047]: cannot borrow `x` as mutable more than once
  --> tests/integration/diagnostics/if_arm_overlapping_mut.phx:7:20
  |
7 |         const _b = &mut x;
  |                    ^^^^^^
   = note: `x` was mutably borrowed here
  --> tests/integration/diagnostics/if_arm_overlapping_mut.phx:5:20
  |
5 |         const _a = &mut x;
  |                    ^^^^^^
   = help: finish using the first `&mut x` borrow before creating another",
};
const DIAG_LOOP_OVERLAPPING_MUT_SOURCE: &str = r"main :: () => {
    var x: s32 = 1;
    loop {
        const _a = &mut x;
    };
};
";

pub const DIAG_LOOP_OVERLAPPING_MUT: DiagnosticCase = DiagnosticCase {
    name: r"loop_overlapping_mut",
    kind: DiagnosticKind::SingleFile,
    source: Some(DIAG_LOOP_OVERLAPPING_MUT_SOURCE),
    expected: r"error[E2047]: cannot borrow `x` as mutable more than once
  --> tests/integration/diagnostics/loop_overlapping_mut.phx:4:20
  |
4 |         const _a = &mut x;
  |                    ^^^^^^
   = note: `x` was mutably borrowed here
   = help: finish using the first `&mut x` borrow before creating another",
};

pub const DIAGNOSTIC_CASES: &[DiagnosticCase] = &[
    DIAG_BAD_TYPE,
    DIAG_USE_AFTER_MOVE,
    DIAG_MISSING_MAIN,
    DIAG_IMPORT_CYCLE,
    DIAG_MULTI_RESOLVE_DUPLICATE,
    DIAG_RETURN_LOCAL_STR,
    DIAG_POW_UNSUPPORTED,
    DIAG_HASH_DERIVE_INVALID,
    DIAG_UNIQUE_PTR_USE_AFTER_MOVE,
    DIAG_TRAIT_IMPL_INCOMPLETE,
    DIAG_EXTERN_UNSAFE,
    DIAG_INVALID_UTF8_BYTE_AS_STR,
    DIAG_MATCH_UNREACHABLE_ARM,
    DIAG_TRY_OK_MISMATCH,
    DIAG_TRY_FROM_MISSING,
    DIAG_DISCARDED_STD_RESULT,
    DIAG_DISCARDED_STD_OPTION,
    DIAG_LOOP_MOVE_USE_AFTER_LOOP,
    DIAG_IF_BRANCH_SIBLING_NO_FALSE_UAM,
    DIAG_IF_BRANCH_UNTAKEN_NO_MOVE,
    DIAG_INVALID_CAST,
    DIAG_DOUBLE_MUT_BORROW,
    DIAG_SHARED_MUT_BORROW,
    DIAG_IF_ARM_OVERLAPPING_MUT,
    DIAG_LOOP_OVERLAPPING_MUT,
];
