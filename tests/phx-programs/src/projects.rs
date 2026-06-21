//! `phoenix.toml` project fixtures.

use super::ProjectSpec;

static PROJECT_ALLOCATOR_SMOKE_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::memory::allocator::{Allocator, Layout, Global};

main :: () => {
    var alloc: Global = Global {};
    const layout: Layout = Layout { size: 8u, align: 1u };
    unsafe {
        const ptr: *mut u8 = alloc.alloc(layout);
        alloc.dealloc(ptr, layout);
    };
};
",
)];

pub const PROJECT_ALLOCATOR_SMOKE: ProjectSpec = ProjectSpec {
    name: r"allocator_smoke",
    toml: r#"[project]
name = "allocator_smoke"
version = "0.1.0"
description = "V0-066 Global allocator smoke test"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_ALLOCATOR_SMOKE_FILES,
};

static PROJECT_ALLOCATOR_SMOKE_UNSAFE_FAIL_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::memory::allocator::{Allocator, Layout, Global};

main :: () => {
    var alloc: Global = Global {};
    const layout: Layout = Layout { size: 8u, align: 1u };
    const ptr: *mut u8 = alloc.alloc(layout);
    alloc.dealloc(ptr, layout);
};
",
)];

pub const PROJECT_ALLOCATOR_SMOKE_UNSAFE_FAIL: ProjectSpec = ProjectSpec {
    name: r"allocator_smoke_unsafe_fail",
    toml: r#"[project]
name = "allocator_smoke_unsafe_fail"
version = "0.1.0"
description = "V0-066 Allocator trait call outside unsafe must fail (E2035)"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_ALLOCATOR_SMOKE_UNSAFE_FAIL_FILES,
};

static PROJECT_APP_DEP_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import math::{add, id};

main :: () => {
    const a = add(1, 2);
    const b = id :: <s32> (1);
};
",
)];

pub const PROJECT_APP_DEP: ProjectSpec = ProjectSpec {
    name: r"app_dep",
    toml: r#"[project]
name = "app_dep"
version = "0.1.0"
description = "App depending on math lib"
type = "bin"
module_src = "src"
bundle_std = false

[dependencies]
math = { path = "../math_lib" }

[build]
dir = "build"
"#,
    files: PROJECT_APP_DEP_FILES,
};

static PROJECT_BAD_DEP_KEY_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"main :: () => { };
",
)];

pub const PROJECT_BAD_DEP_KEY: ProjectSpec = ProjectSpec {
    name: r"bad_dep_key",
    toml: r#"[project]
name = "bad_dep_key"
version = "0.1.0"
description = "Negative fixture: dependency key mismatch"
type = "bin"
module_src = "src"
bundle_std = false

[dependencies]
wrong_name = { path = "../math_lib" }

[build]
dir = "build"
"#,
    files: PROJECT_BAD_DEP_KEY_FILES,
};

static PROJECT_BIN_MISSING_MAIN_FILES: &[(&str, &str)] = &[];

pub const PROJECT_BIN_MISSING_MAIN: ProjectSpec = ProjectSpec {
    name: r"bin_missing_main",
    toml: r#"[project]
name = "bin_missing_main"
version = "0.1.0"
description = "Negative fixture: bin without main.phx"
type = "bin"
module_src = "src"
bundle_std = false

[build]
dir = "build"
"#,
    files: PROJECT_BIN_MISSING_MAIN_FILES,
};

static PROJECT_DYNAMIC_ARRAY_DOUBLE_FREE_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::collections::dynamic_array::DynamicArray;
#import std::core::memory::allocator::{Allocator, Global};

main :: () => {
    var arr: DynamicArray<s32> = DynamicArray :: <s32>::empty(Global {});
    arr.push(42);
    unsafe {
        var mut_alloc: Global = Global {};
        const layout = arr.byte_layout();
        mut_alloc.dealloc(arr.as_ptr() as *mut u8, layout);
    };
};
",
)];

pub const PROJECT_DYNAMIC_ARRAY_DOUBLE_FREE: ProjectSpec = ProjectSpec {
    name: r"dynamic_array_double_free",
    toml: r#"[project]
name = "dynamic_array_double_free"
version = "0.1.0"
description = "DynamicArray manual dealloc then scope Drop double-free"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_DYNAMIC_ARRAY_DOUBLE_FREE_FILES,
};

static PROJECT_DYNAMIC_ARRAY_DROP_SMOKE_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::collections::dynamic_array::DynamicArray;
#import std::core::memory::allocator::Global;

main :: () => {
    {
        var arr: DynamicArray<s32> = DynamicArray :: <s32>::empty(Global {});
        arr.push(1);
        const _ = arr.len();
    };
};
",
)];

pub const PROJECT_DYNAMIC_ARRAY_DROP_SMOKE: ProjectSpec = ProjectSpec {
    name: r"dynamic_array_drop_smoke",
    toml: r#"[project]
name = "dynamic_array_drop_smoke"
version = "0.1.0"
description = "DynamicArray scope-exit Drop smoke test"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_DYNAMIC_ARRAY_DROP_SMOKE_FILES,
};

static PROJECT_DYNAMIC_ARRAY_GROW_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::collections::dynamic_array::DynamicArray;
#import std::core::memory::allocator::Global;

main :: () => {
    var arr: DynamicArray<s32> = DynamicArray :: <s32>::empty(Global {});
    arr.push(1);
    arr.push(2);
    arr.push(3);
    arr.push(4);
    arr.push(5);
    arr.push(6);
    arr.push(7);
    arr.push(8);
    const n: u32 = arr.len();
    const a: s32 = arr.get(0u);
    const b: s32 = arr.get(1u);
    const c: s32 = arr.get(2u);
    const d: s32 = arr.get(3u);
    const e: s32 = arr.get(4u);
    const f: s32 = arr.get(5u);
    const g: s32 = arr.get(6u);
    const h: s32 = arr.get(7u);
    const sum: s32 = a + b + c + d + e + f + g + h;
    const check_len: u32 = n;
    const check_sum: s32 = sum;
};
",
)];

pub const PROJECT_DYNAMIC_ARRAY_GROW: ProjectSpec = ProjectSpec {
    name: r"dynamic_array_grow",
    toml: r#"[project]
name = "dynamic_array_grow"
version = "0.1.0"
description = "DynamicArray push past initial cap forces grow"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_DYNAMIC_ARRAY_GROW_FILES,
};

static PROJECT_DYNAMIC_ARRAY_INDEX_OOB_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::collections::dynamic_array::DynamicArray;
#import std::core::memory::allocator::Global;

main :: () => {
    var arr: DynamicArray<s32> = DynamicArray :: <s32>::empty(Global {});
    arr.push(10);
    arr.push(20);
    const bad: s32 = arr.get(arr.len());
    const _ = bad;
};
",
)];

pub const PROJECT_DYNAMIC_ARRAY_INDEX_OOB: ProjectSpec = ProjectSpec {
    name: r"dynamic_array_index_oob",
    toml: r#"[project]
name = "dynamic_array_index_oob"
version = "0.1.0"
description = "DynamicArray get with out-of-bounds index fails at runtime"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_DYNAMIC_ARRAY_INDEX_OOB_FILES,
};

static PROJECT_DYNAMIC_ARRAY_MOVE_IN_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::collections::dynamic_array::DynamicArray;
#import std::core::memory::allocator::Global;
#import std::core::drop::Drop;

Wrapper :: struct {};

Wrapper :: impl :: Drop {
    drop :: (self) => () {};
};

main :: () => {
    var arr: DynamicArray<Wrapper> = DynamicArray :: <Wrapper>::empty(Global {});
    var w: Wrapper = Wrapper {};
    arr.push(w);
    const _ = w;
};
",
)];

pub const PROJECT_DYNAMIC_ARRAY_MOVE_IN: ProjectSpec = ProjectSpec {
    name: r"dynamic_array_move_in",
    toml: r#"[project]
name = "dynamic_array_move_in"
version = "0.1.0"
description = "DynamicArray push moves value; use-after-move is compile error"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_DYNAMIC_ARRAY_MOVE_IN_FILES,
};

static PROJECT_DYNAMIC_ARRAY_NESTED_DROP_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::collections::dynamic_array::DynamicArray;
#import std::core::memory::allocator::Global;

// MVP heap slices are primitive-only; this fixture validates container drop with len > 1
// (element drop loop is a no-op for `s32`). HeapBuf/UniquePtr nested drop awaits aggregate slices.
main :: () => {
    {
        var arr: DynamicArray<s32> = DynamicArray :: <s32>::empty(Global {});
        arr.push(1);
        arr.push(2);
        arr.push(3);
        const n: u32 = arr.len();
        const _ = n;
    };
};
",
)];

pub const PROJECT_DYNAMIC_ARRAY_NESTED_DROP: ProjectSpec = ProjectSpec {
    name: r"dynamic_array_nested_drop",
    toml: r#"[project]
name = "dynamic_array_nested_drop"
version = "0.1.0"
description = "DynamicArray element-wise Drop on scope exit"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_DYNAMIC_ARRAY_NESTED_DROP_FILES,
};

static PROJECT_DYNAMIC_ARRAY_POP_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::collections::dynamic_array::DynamicArray;
#import std::core::memory::allocator::Global;

main :: () => {
    var arr: DynamicArray<s32> = DynamicArray :: <s32>::empty(Global {});
    arr.push(10);
    arr.push(20);
    arr.push(30);
    const third: s32 = arr.pop();
    const second: s32 = arr.pop();
    const n: u32 = arr.len();
    const remaining: s32 = arr.get(0u);
    const check_third: s32 = third;
    const check_second: s32 = second;
    const check_len: u32 = n;
    const check_remaining: s32 = remaining;
};
",
)];

pub const PROJECT_DYNAMIC_ARRAY_POP: ProjectSpec = ProjectSpec {
    name: r"dynamic_array_pop",
    toml: r#"[project]
name = "dynamic_array_pop"
version = "0.1.0"
description = "DynamicArray pop move-out semantics"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_DYNAMIC_ARRAY_POP_FILES,
};

static PROJECT_DYNAMIC_ARRAY_SMOKE_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::collections::dynamic_array::DynamicArray;
#import std::core::memory::allocator::Global;

main :: () => {
    var arr: DynamicArray<s32> = DynamicArray :: <s32>::empty(Global {});
    arr.push(10);
    arr.push(20);
    arr.push(30);
    const n: u32 = arr.len();
    const first: s32 = arr.get(0u);
    const second: s32 = arr.get(1u);
    const sum: s32 = first + second + (n as s32);
    const check_sum: s32 = sum;
    const check_len: u32 = n;
};
",
)];

pub const PROJECT_DYNAMIC_ARRAY_SMOKE: ProjectSpec = ProjectSpec {
    name: r"dynamic_array_smoke",
    toml: r#"[project]
name = "dynamic_array_smoke"
version = "0.1.0"
description = "Std DynamicArray push/get/Drop smoke test"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_DYNAMIC_ARRAY_SMOKE_FILES,
};

static PROJECT_DYNAMIC_ARRAY_UAF_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::collections::dynamic_array::DynamicArray;
#import std::core::memory::allocator::Global;
#import std::core::alloc::alloc_bytes;
#import std::core::slice::slice_from_raw_parts;

main :: () => {
    unsafe {
        var saved_ptr: *mut s32 = alloc_bytes(4u) as *mut s32;
        var saved_len: u32 = 0u;
        {
            var arr: DynamicArray<s32> = DynamicArray :: <s32>::empty(Global {});
            arr.push(42);
            saved_ptr = arr.as_ptr();
            saved_len = arr.len();
        };
        const sl: [s32] = slice_from_raw_parts(saved_ptr, saved_len);
        const v: s32 = sl[0];
        const _ = v;
    };
};
",
)];

pub const PROJECT_DYNAMIC_ARRAY_UAF: ProjectSpec = ProjectSpec {
    name: r"dynamic_array_uaf",
    toml: r#"[project]
name = "dynamic_array_uaf"
version = "0.1.0"
description = "DynamicArray use-after-free via saved slice after drop"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_DYNAMIC_ARRAY_UAF_FILES,
};

static PROJECT_EXTERN_C_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r#"#import std::ffi::c_int;

extern "C" {
    c_add :: (a: s32, b: s32) => c_int;
};

main :: () => {
    unsafe {
        const sum: c_int = c_add(10, 2);
        const _ = sum;
    };
};
"#,
)];

pub const PROJECT_EXTERN_C: ProjectSpec = ProjectSpec {
    name: r"extern_c",
    toml: r#"[project]
name = "extern_c"
version = "0.1.0"
description = "V0-053 extern C FFI smoke (unsafe call)"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_EXTERN_C_FILES,
};

static PROJECT_HEAP_ALLOC_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::alloc::alloc_bytes;

main :: () => {
    unsafe {
        const buf: *mut u8 = alloc_bytes(4u);
        const byte: u8 = 77 as u8;
        *buf = byte;
        const v: u8 = *buf;
        const _ = v;
    };
};
",
)];

pub const PROJECT_HEAP_ALLOC: ProjectSpec = ProjectSpec {
    name: r"heap_alloc",
    toml: r#"[project]
name = "heap_alloc"
version = "0.1.0"
description = "V0-030 heap alloc intrinsic smoke test"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_HEAP_ALLOC_FILES,
};

static PROJECT_HEAP_ALLOC_OOM_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::alloc::alloc_bytes;

main :: () => {
    unsafe {
        loop {
            const _buf: *mut u8 = alloc_bytes(8u);
        };
    };
};
",
)];

pub const PROJECT_HEAP_ALLOC_OOM: ProjectSpec = ProjectSpec {
    name: r"heap_alloc_oom",
    toml: r#"[project]
name = "heap_alloc_oom"
version = "0.1.0"
description = "V0-030 heap alloc OOM regression with low heap cap"
type = "bin"
module_src = "src"

[build]
dir = "build"

[vm]
heap_cap = 32
"#,
    files: PROJECT_HEAP_ALLOC_OOM_FILES,
};

static PROJECT_HEAP_ALLOC_UNSAFE_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::alloc::alloc_bytes;

main :: () => {
    alloc_bytes(4u);
};
",
)];

pub const PROJECT_HEAP_ALLOC_UNSAFE: ProjectSpec = ProjectSpec {
    name: r"heap_alloc_unsafe",
    toml: r#"[project]
name = "heap_alloc_unsafe"
version = "0.1.0"
description = "V0-030 alloc_bytes requires unsafe"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_HEAP_ALLOC_UNSAFE_FILES,
};

static PROJECT_HEAP_DEALLOC_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::alloc::{alloc_bytes, dealloc_bytes};

main :: () => {
    unsafe {
        const buf: *mut u8 = alloc_bytes(4u);
        dealloc_bytes(buf, 4u);
    };
};
",
)];

pub const PROJECT_HEAP_DEALLOC: ProjectSpec = ProjectSpec {
    name: r"heap_dealloc",
    toml: r#"[project]
name = "heap_dealloc"
version = "0.1.0"
description = "V0-065 heap dealloc intrinsic smoke test"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_HEAP_DEALLOC_FILES,
};

static PROJECT_HEAP_DEALLOC_DOUBLE_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::alloc::{alloc_bytes, dealloc_bytes};

main :: () => {
    unsafe {
        const buf: *mut u8 = alloc_bytes(4u);
        dealloc_bytes(buf, 4u);
        dealloc_bytes(buf, 4u);
    };
};
",
)];

pub const PROJECT_HEAP_DEALLOC_DOUBLE: ProjectSpec = ProjectSpec {
    name: r"heap_dealloc_double",
    toml: r#"[project]
name = "heap_dealloc_double"
version = "0.1.0"
description = "V0-065 double-free runtime error"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_HEAP_DEALLOC_DOUBLE_FILES,
};

static PROJECT_HEAP_DEALLOC_UNSAFE_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::alloc::{alloc_bytes, dealloc_bytes};

main :: () => {
    const buf: *mut u8 = alloc_bytes(4u);
    dealloc_bytes(buf, 4u);
};
",
)];

pub const PROJECT_HEAP_DEALLOC_UNSAFE: ProjectSpec = ProjectSpec {
    name: r"heap_dealloc_unsafe",
    toml: r#"[project]
name = "heap_dealloc_unsafe"
version = "0.1.0"
description = "V0-065 dealloc_bytes requires unsafe"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_HEAP_DEALLOC_UNSAFE_FILES,
};

static PROJECT_HEAP_DROP_DEALLOC_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::alloc::{alloc_bytes, dealloc_bytes};
#import std::core::drop::Drop;

HeapBuf :: struct {
    ptr: *mut u8,
    size: u32,
};

free_heap :: (ptr: *mut u8, size: u32) => {
    unsafe {
        dealloc_bytes(ptr, size);
    };
};

HeapBuf :: impl :: Drop {
    drop :: (self) => {
        free_heap(self.ptr, self.size);
    };
};

main :: () => {
    unsafe {
        const p: *mut u8 = alloc_bytes(8u);
        const buf: HeapBuf = HeapBuf { ptr: p, size: 8u };
    };
};
",
)];

pub const PROJECT_HEAP_DROP_DEALLOC: ProjectSpec = ProjectSpec {
    name: r"heap_drop_dealloc",
    toml: r#"[project]
name = "heap_drop_dealloc"
version = "0.1.0"
description = "V0-065 Drop glue calls dealloc_bytes"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_HEAP_DROP_DEALLOC_FILES,
};

static PROJECT_HEAP_SLICE_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::alloc::alloc_bytes;
#import std::core::slice::slice_from_raw_parts;

main :: () => {
    unsafe {
        const buf: *mut u8 = alloc_bytes(4u);
        const byte: u8 = 77 as u8;
        *buf = byte;
        const sl: [u8] = slice_from_raw_parts(buf, 4u);
        const v: u8 = sl[0];
        const _ = v;
    };
};
",
)];

pub const PROJECT_HEAP_SLICE: ProjectSpec = ProjectSpec {
    name: r"heap_slice",
    toml: r#"[project]
name = "heap_slice"
version = "0.1.0"
description = "V0-062 heap slice intrinsic smoke test"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_HEAP_SLICE_FILES,
};

static PROJECT_HEAP_SLICE_NESTED_INDEX_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::alloc::alloc_bytes;
#import std::core::slice::slice_from_raw_parts;

idx :: () => s32 {
    0
};

main :: () => {
    unsafe {
        const buf: *mut u8 = alloc_bytes(4u);
        var sl: [u8] = slice_from_raw_parts(buf, 4u);
        const byte: u8 = 42 as u8;
        sl[idx()] = byte;
        const v: u8 = sl[idx()];
        const _ = v;
    };
};
",
)];

pub const PROJECT_HEAP_SLICE_NESTED_INDEX: ProjectSpec = ProjectSpec {
    name: r"heap_slice_nested_index",
    toml: r#"[project]
name = "heap_slice_nested_index"
version = "0.1.0"
description = "V0-062 heap slice non-literal index store"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_HEAP_SLICE_NESTED_INDEX_FILES,
};

static PROJECT_HEAP_SLICE_OOB_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::alloc::alloc_bytes;
#import std::core::slice::slice_from_raw_parts;

main :: () => {
    unsafe {
        const buf: *mut u8 = alloc_bytes(4u);
        const sl: [u8] = slice_from_raw_parts(buf, 4u);
        const v: u8 = sl[10];
        const _ = v;
    };
};
",
)];

pub const PROJECT_HEAP_SLICE_OOB: ProjectSpec = ProjectSpec {
    name: r"heap_slice_oob",
    toml: r#"[project]
name = "heap_slice_oob"
version = "0.1.0"
description = "V0-062 heap slice out-of-bounds index"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_HEAP_SLICE_OOB_FILES,
};

static PROJECT_HEAP_SLICE_STORE_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::alloc::alloc_bytes;
#import std::core::slice::slice_from_raw_parts;

main :: () => {
    unsafe {
        const buf: *mut u8 = alloc_bytes(4u);
        var sl: [u8] = slice_from_raw_parts(buf, 4u);
        const byte: u8 = 42 as u8;
        sl[0] = byte;
        const v: u8 = sl[0];
        const _ = v;
    };
};
",
)];

pub const PROJECT_HEAP_SLICE_STORE: ProjectSpec = ProjectSpec {
    name: r"heap_slice_store",
    toml: r#"[project]
name = "heap_slice_store"
version = "0.1.0"
description = "V0-062 heap slice IndexStore smoke test"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_HEAP_SLICE_STORE_FILES,
};

static PROJECT_HEAP_SLICE_UNSAFE_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::slice::slice_from_raw_parts;

main :: () => {
    const buf: *mut u8 = 0 as *mut u8;
    const sl: [u8] = slice_from_raw_parts(buf, 4u);
    const _ = sl;
};
",
)];

pub const PROJECT_HEAP_SLICE_UNSAFE: ProjectSpec = ProjectSpec {
    name: r"heap_slice_unsafe",
    toml: r#"[project]
name = "heap_slice_unsafe"
version = "0.1.0"
description = "V0-062 heap slice requires unsafe"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_HEAP_SLICE_UNSAFE_FILES,
};

static PROJECT_HEAP_UAF_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::alloc::{alloc_bytes, dealloc_bytes};

main :: () => {
    unsafe {
        const buf: *mut u8 = alloc_bytes(4u);
        dealloc_bytes(buf, 4u);
        const v: u8 = *buf;
        const _ = v;
    };
};
",
)];

pub const PROJECT_HEAP_UAF: ProjectSpec = ProjectSpec {
    name: r"heap_uaf",
    toml: r#"[project]
name = "heap_uaf"
version = "0.1.0"
description = "heap use-after-free runtime error"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_HEAP_UAF_FILES,
};

static PROJECT_NESTED_TRAP_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::alloc::{alloc_bytes, dealloc_bytes};

read_after_free :: () => {
    unsafe {
        const buf: *mut u8 = alloc_bytes(4u);
        dealloc_bytes(buf, 4u);
        const v: u8 = *buf;
        const _ = v;
    };
};

main :: () => {
    read_after_free();
};
",
)];

pub const PROJECT_NESTED_TRAP: ProjectSpec = ProjectSpec {
    name: r"nested_trap",
    toml: r#"[project]
name = "nested_trap"
version = "0.1.0"
description = "VM fault in nested helper resolves to helper source line"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_NESTED_TRAP_FILES,
};

static PROJECT_HELLO_PRINT_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r#"#import std::io::write_stdout;

main :: () => {
    write_stdout("hello\n");
};
"#,
)];

pub const PROJECT_HELLO_PRINT: ProjectSpec = ProjectSpec {
    name: r"hello_print",
    toml: r#"[project]
name = "hello_print"
version = "0.1.0"
description = "Hello world via std::io::write_stdout"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_HELLO_PRINT_FILES,
};

static PROJECT_LIB_WITH_MAIN_FILES: &[(&str, &str)] = &[(
    r"src/lib.phx",
    r"pub add :: (a: s32, b: s32) => s32 { a + b };

main :: () => { };
",
)];

pub const PROJECT_LIB_WITH_MAIN: ProjectSpec = ProjectSpec {
    name: r"lib_with_main",
    toml: r#"[project]
name = "lib_with_main"
version = "0.1.0"
description = "Negative fixture: main in lib package"
type = "lib"
module_src = "src"
bundle_std = false

[build]
dir = "build"
"#,
    files: PROJECT_LIB_WITH_MAIN_FILES,
};

static PROJECT_LINK_REBASE_FILES: &[(&str, &str)] = &[
    (
        r"src/main.phx",
        r"#import types::point::{Point, get_x};

mod types;

Outcome :: enum {
    Ok { v: s32 },
    Err { code: s32 },
};

main :: () => {
    const p: Point = Point { x: 10, y: 20 };
    const field: s32 = get_x(p);
    const r: Outcome = Ok { v: field };
    const _ = match r {
        Ok { v } => v;
        Err { code } => code;
    };
};
",
    ),
    (
        r"src/types/mod.phx",
        r"pub mod point;
",
    ),
    (
        r"src/types/point.phx",
        r"pub Point :: struct {
    x: s32,
    y: s32,
};

pub get_x :: (p: Point) => s32 {
    p.x
};
",
    ),
];

pub const PROJECT_LINK_REBASE: ProjectSpec = ProjectSpec {
    name: r"link_rebase",
    toml: r#"[project]
name = "link_rebase"
version = "0.1.0"
description = "Cross-module link rebase: GetField and MatchTag"
type = "bin"
module_src = "src"
bundle_std = false

[build]
dir = "build"
"#,
    files: PROJECT_LINK_REBASE_FILES,
};

static PROJECT_LINT_DENY_PROJECT_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r#"#[deprecated(note = "use new_fn instead")]
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
)];

pub const PROJECT_LINT_DENY_PROJECT: ProjectSpec = ProjectSpec {
    name: r"lint_deny_project",
    toml: r#"[project]
name = "lint_deny_project"
type = "bin"
module_src = "src"
bundle_std = false

[lint]
deny = ["deprecated"]
"#,
    files: PROJECT_LINT_DENY_PROJECT_FILES,
};

static PROJECT_LINT_STD_RESULT_DISCARD_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::result::{Result, Ok, Err};

fail :: () => Result<s32, s32> {
    Ok(1)
};

main :: () => {
    fail();
    const _ = 0;
};
",
)];

pub const PROJECT_LINT_STD_RESULT_DISCARD: ProjectSpec = ProjectSpec {
    name: r"lint_std_result_discard",
    toml: r#"[project]
name = "lint_std_result_discard"
version = "0.1.0"
description = "PHX-029 lint: discarded std Result must-use warning"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_LINT_STD_RESULT_DISCARD_FILES,
};

static PROJECT_LINT_STD_OPTION_DISCARD_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::option::{Option, Some, None};

maybe_one :: () => Option<s32> {
    Some(1)
};

main :: () => {
    maybe_one();
    const _ = 0;
};
",
)];

pub const PROJECT_LINT_STD_OPTION_DISCARD: ProjectSpec = ProjectSpec {
    name: r"lint_std_option_discard",
    toml: r#"[project]
name = "lint_std_option_discard"
version = "0.1.0"
description = "PHX-061 golden: discarded std Option must be handled (E2042)"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_LINT_STD_OPTION_DISCARD_FILES,
};

static PROJECT_MATH_LIB_FILES: &[(&str, &str)] = &[(
    r"src/lib.phx",
    r"pub add :: (a: s32, b: s32) => s32 { a + b + 1 };

pub id :: <t> (x: t) => t { x };
",
)];

pub const PROJECT_MATH_LIB: ProjectSpec = ProjectSpec {
    name: r"math_lib",
    toml: r#"[project]
name = "math"
version = "0.1.0"
description = "Test library"
type = "lib"
module_src = "src"
bundle_std = false

[build]
dir = "build"
"#,
    files: PROJECT_MATH_LIB_FILES,
};

static PROJECT_MODULES_BIN_BARREL_FILES: &[(&str, &str)] = &[
    (
        r"src/main.phx",
        r"#import util::add;

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"src/util/math.phx",
        r"pub add :: (a: s32, b: s32) => s32 { a + b };
",
    ),
    (
        r"src/util/mod.phx",
        r"pub mod math;

pub reexport :: math::add;
",
    ),
];

pub const PROJECT_MODULES_BIN_BARREL: ProjectSpec = ProjectSpec {
    name: r"modules_bin_barrel",
    toml: r#"[project]
name = "modules_bin_barrel"
version = "0.1.0"
description = "Bin barrel reexport smoke"
type = "bin"
module_src = "src"
bundle_std = false

[build]
dir = "build"
"#,
    files: PROJECT_MODULES_BIN_BARREL_FILES,
};

static PROJECT_MODULES_TRAP_FILES: &[(&str, &str)] = &[
    (
        r"src/main.phx",
        r"#import util::div_zero;

mod util;

main :: () => {
    const _ = div_zero(1, 0);
};
",
    ),
    (
        r"src/util/trap.phx",
        r"pub div_zero :: (a: s32, b: s32) => s32 {
    a / b
};
",
    ),
    (
        r"src/util/mod.phx",
        r"pub mod trap;

pub reexport :: trap::div_zero;
",
    ),
];

pub const PROJECT_MODULES_TRAP: ProjectSpec = ProjectSpec {
    name: r"modules_trap",
    toml: r#"[project]
name = "modules_trap"
version = "0.1.0"
description = "Multi-module VM trap resolves to callee source line"
type = "bin"
module_src = "src"
bundle_std = false

[build]
dir = "build"
"#,
    files: PROJECT_MODULES_TRAP_FILES,
};

static PROJECT_MODULES_MISSING_MOD_FILES: &[(&str, &str)] = &[
    (
        r"src/main.phx",
        r"#import util::math::add;

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"src/util/math.phx",
        r"pub add :: (a: s32, b: s32) => s32 { a + b };
",
    ),
];

pub const PROJECT_MODULES_MISSING_MOD: ProjectSpec = ProjectSpec {
    name: r"modules_missing_mod",
    toml: r#"[project]
name = "modules_missing_mod"
version = "0.1.0"
description = "Missing util/mod.phx diagnostic"
type = "bin"
module_src = "src"
bundle_std = false

[build]
dir = "build"
"#,
    files: PROJECT_MODULES_MISSING_MOD_FILES,
};

static PROJECT_MODULES_ORPHAN_FILE_FILES: &[(&str, &str)] = &[
    (
        r"src/main.phx",
        r"#import util::math::add;

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"src/util/extra.phx",
        r"orphan :: () => { };
",
    ),
    (
        r"src/util/math.phx",
        r"pub add :: (a: s32, b: s32) => s32 { a + b };
",
    ),
    (
        r"src/util/mod.phx",
        r"mod math;
",
    ),
];

pub const PROJECT_MODULES_ORPHAN_FILE: ProjectSpec = ProjectSpec {
    name: r"modules_orphan_file",
    toml: r#"[project]
name = "modules_orphan_file"
version = "0.1.0"
description = "Orphan util/extra.phx diagnostic"
type = "bin"
module_src = "src"
bundle_std = false

[build]
dir = "build"
"#,
    files: PROJECT_MODULES_ORPHAN_FILE_FILES,
};

static PROJECT_MVP_ACCEPTANCE_FILES: &[(&str, &str)] = &[
    (
        r"src/main.phx",
        r"#import shapes::math::pick;

mod shapes;

Point :: struct {
    x: s32,
    y: s32,
};

Result :: enum {
    Ok { v: s32 },
    Err { code: s32 },
};

main :: () => {
    const p: Point = Point { x: 3, y: 4 };
    const r: Result = Ok { v: p.x };
    const along: s32 = match r {
        Ok { v } => v;
        Err { code } => code;
    };
    const _ = along + pick();
};
",
    ),
    (
        r"src/shapes/math.phx",
        r"pub pick :: () => s32 {
    1
};
",
    ),
    (
        r"src/shapes/mod.phx",
        r"pub mod math;
",
    ),
];

pub const PROJECT_MVP_ACCEPTANCE: ProjectSpec = ProjectSpec {
    name: r"mvp_acceptance",
    toml: r#"[project]
name = "mvp_acceptance"
version = "0.1.0"
description = "MVP acceptance smoke project"
type = "bin"
module_src = "src"
bundle_std = false

[build]
dir = "build"
"#,
    files: PROJECT_MVP_ACCEPTANCE_FILES,
};

static PROJECT_NO_BUNDLE_STD_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"main :: () => {
    const x: Option<s32> = None;
    const _ = x;
};
",
)];

pub const PROJECT_NO_BUNDLE_STD: ProjectSpec = ProjectSpec {
    name: r"no_bundle_std",
    toml: r#"[project]
name = "no_bundle_std"
version = "0.1.0"
description = "Verifies bundle_std = false does not link std"
type = "bin"
module_src = "src"
bundle_std = false

[build]
dir = "build"
"#,
    files: PROJECT_NO_BUNDLE_STD_FILES,
};

static PROJECT_PRIMITIVE_DISPLAY_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::fmt_display::Display;
#import std::text::string::String;

main :: () => {
    const n: s32 = 42;
    const text = String::from_display_buf(n.fmt());
    const check_len = text.len();
    const _ = check_len;
};
",
)];

pub const PROJECT_PRIMITIVE_DISPLAY: ProjectSpec = ProjectSpec {
    name: r"primitive_display",
    toml: r#"[project]
name = "primitive_display"
version = "0.1.0"
description = "Primitive s32 :: impl :: Display smoke test"
type = "bin"
module_src = "src"
prelude = false

[build]
dir = "build"
"#,
    files: PROJECT_PRIMITIVE_DISPLAY_FILES,
};

static PROJECT_PRINT_S32_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::text::fmt::print;

main :: () => {
    print(42);
};
",
)];

pub const PROJECT_PRINT_S32: ProjectSpec = ProjectSpec {
    name: r"print_s32",
    toml: r#"[project]
name = "print_s32"
version = "0.1.0"
description = "Std text::fmt print smoke test"
type = "bin"
module_src = "src"
prelude = true

[build]
dir = "build"
"#,
    files: PROJECT_PRINT_S32_FILES,
};

static PROJECT_PROJECT_FILES: &[(&str, &str)] = &[
    (
        r"src/main.phx",
        r"#import util::math::add;

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"src/util/math.phx",
        r"pub add :: (a: s32, b: s32) => s32 { a + b };
",
    ),
    (
        r"src/util/mod.phx",
        r"pub mod math;
",
    ),
];

pub const PROJECT_PROJECT: ProjectSpec = ProjectSpec {
    name: r"project",
    toml: r#"[project]
name = "cli_project_test"
version = "0.1.0"
description = "CLI build/run fixture"
type = "bin"
module_src = "src"
bundle_std = false

[build]
dir = "build"
"#,
    files: PROJECT_PROJECT_FILES,
};

static PROJECT_STD_CONVERT_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::convert::{From, Into, TryFrom};
#import std::core::result::{Result, Ok, Err};

Wrap :: struct {
    n: s32,
};

Wrap :: impl :: From<s32> {
    from :: (value: s32) => Wrap { Wrap { n: value } };
};

Wrap :: impl :: Into<s32> {
    into :: (self) => s32 {
        self.n
    };
};

convert :: <t: From<s32>> (x: s32) => t {
    t::from(x)
};

Parseable :: struct {
    ok: bool,
};

Parseable :: impl :: TryFrom<s32> {
    type Error = s32;
    try_from :: (value: s32) => Result<Parseable, s32> {
        if value >= 0 {
            Ok(Parseable { ok: true })
        } else {
            Err(value)
        }
    };
};

main :: () => {
    const w: Wrap = convert :: <Wrap>(42);
    const n: s32 = w.into();
    const p = Parseable::try_from(1);
    const _ = n;
    const _discard = p;
};
",
)];

pub const PROJECT_STD_CONVERT: ProjectSpec = ProjectSpec {
    name: r"std_convert",
    toml: r#"[project]
name = "std_convert"
version = "0.1.0"
description = "V0-058 std conversion traits smoke test"
type = "bin"
module_src = "src"
bundle_std = true
prelude = false

[build]
dir = "build"
"#,
    files: PROJECT_STD_CONVERT_FILES,
};

static PROJECT_STD_DERIVE_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#[derive(PartialEq, Copyable)]
Rect :: struct {
    w: s32,
    h: s32,
};

main :: () => {
    var a: Rect = Rect { w: 2, h: 3 };
    var b: Rect = Rect { w: 2, h: 3 };
    var c: Rect = Rect { w: 1, h: 3 };
    const same: bool = a.eq(&b);
    const diff: bool = a.eq(&c);
    const _ = same;
    const _discard = diff;
};
",
)];

pub const PROJECT_STD_DERIVE: ProjectSpec = ProjectSpec {
    name: r"std_derive",
    toml: r#"[project]
name = "std_derive"
version = "0.1.0"
description = "V0-056 std derive smoke test"
type = "bin"
module_src = "src"
prelude = true

[build]
dir = "build"
"#,
    files: PROJECT_STD_DERIVE_FILES,
};

static PROJECT_STD_ERRORS_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::error::Error;
#import std::core::result::{Err, Ok, Result};

Config :: struct {
    value: s32,
};

AppError :: struct {
    code: s32,
};

AppError :: impl :: Error {
};

read_bytes :: () => Result<Config, AppError> {
    Ok(Config { value: 42 })
};

read_config :: () => Result<Config, AppError> {
    const cfg = read_bytes()?;
    Ok(cfg)
};

main :: () => {
    match read_config() {
        Ok(c) => { const _ = c.value; };
        Err(e) => { const _ = e; };
    };
};
",
)];

pub const PROJECT_STD_ERRORS: ProjectSpec = ProjectSpec {
    name: r"std_errors",
    toml: r#"[project]
name = "std_errors"
version = "0.1.0"
description = "V0-060 layered std error types with ? and From conversion"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_STD_ERRORS_FILES,
};

static PROJECT_STD_GENERIC_DERIVE_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::option::{Option, Some, None};
#import std::core::result::{Result, Ok, Err};

main :: () => {
    var a: Option<s32> = Some(1);
    var b: Option<s32> = Some(1);
    var c: Option<s32> = None :: <s32>();
    const opt_same: bool = a.eq(&b);
    const opt_diff: bool = a.eq(&c);

    var ok_a: Result<s32, s32> = Ok(2);
    var ok_b: Result<s32, s32> = Ok(2);
    var err: Result<s32, s32> = Err(3);
    const res_same: bool = ok_a.eq(&ok_b);
    const res_diff: bool = ok_a.eq(&err);

    const _ = opt_same;
    const _discard1 = opt_diff;
    const _discard2 = res_same;
    const _discard3 = res_diff;
};
",
)];

pub const PROJECT_STD_GENERIC_DERIVE: ProjectSpec = ProjectSpec {
    name: r"std_generic_derive",
    toml: r#"[project]
name = "std_generic_derive"
version = "0.1.0"
description = "Generic derive on std Option and Result"
type = "bin"
module_src = "src"
prelude = true

[build]
dir = "build"
"#,
    files: PROJECT_STD_GENERIC_DERIVE_FILES,
};

static PROJECT_STD_ITER_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::iter::{Range};
#import std::core::option::{Option, Some, None};

main :: () => {
    var sum: s32 = 0;
    for x in Range { start: 0, end: 3 } {
        sum = sum + x;
    };
    const _ = sum;
};
",
)];

pub const PROJECT_STD_ITER: ProjectSpec = ProjectSpec {
    name: r"std_iter",
    toml: r#"[project]
name = "std_iter"
version = "0.1.0"
description = "V0-055 std iterator for-loop smoke test"
type = "bin"
module_src = "src"
bundle_std = true
prelude = false

[build]
dir = "build"
"#,
    files: PROJECT_STD_ITER_FILES,
};

static PROJECT_STD_PLATFORM_SMOKE_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::error::Error;
#import std::core::result::{Result, Ok, Err};
#import std::core::alloc::alloc_bytes;
#import std::core::slice::slice_from_raw_parts;

Config :: struct {
    value: s32,
};

AppError :: struct {
    code: s32,
};

AppError :: impl :: Error {
};

ByteMarker :: trait {
    marker_byte :: () => u8 {
        77u as u8
    };
};

HeapTag :: struct {
    n: s32,
};

HeapTag :: impl :: ByteMarker { };

read_config :: () => Result<Config, AppError> {
    Ok(Config { value: 42 })
};

combine_heap :: (c: Config) => s32 {
    var heap_byte: u8 = 0u as u8;
    unsafe {
        const buf: *mut u8 = alloc_bytes(4u);
        const tag: u8 = HeapTag::marker_byte();
        *buf = tag;
        const sl: [u8] = slice_from_raw_parts(buf, 4u);
        heap_byte = sl[0];
    };
    c.value + (heap_byte as s32)
};

main :: () => {
    match read_config() {
        Ok(c) => {
            const sum: s32 = combine_heap(c);
            const _ = sum;
        };
        Err(e) => { const _ = e; };
    };
};
",
)];

pub const PROJECT_STD_PLATFORM_SMOKE: ProjectSpec = ProjectSpec {
    name: r"std_platform_smoke",
    toml: r#"[project]
name = "std_platform_smoke"
version = "0.1.0"
description = "V0-067 Phase 7 capstone — Result match, trait defaults, heap slices"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_STD_PLATFORM_SMOKE_FILES,
};

static PROJECT_STD_PRELUDE_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"wrap :: (n: s32) => Option<s32> {
    Some :: <s32> (n)
};

main :: () => {
    const v: Option<s32> = wrap(7);
    const _ = match v {
        None => 0;
        Some(n) => n;
    };
};
",
)];

pub const PROJECT_STD_PRELUDE: ProjectSpec = ProjectSpec {
    name: r"std_prelude",
    toml: r#"[project]
name = "std_prelude"
version = "0.1.0"
description = "V0-044 prelude smoke test"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_STD_PRELUDE_FILES,
};

static PROJECT_STD_PRELUDE_OFF_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"main :: () => {
    const _unused: Option<s32> = Option :: <s32> { };
};
",
)];

pub const PROJECT_STD_PRELUDE_OFF: ProjectSpec = ProjectSpec {
    name: r"std_prelude_off",
    toml: r#"[project]
name = "std_prelude_off"
version = "0.1.0"
description = "V0-044 prelude disabled — requires explicit import"
type = "bin"
module_src = "src"
prelude = false

[build]
dir = "build"
"#,
    files: PROJECT_STD_PRELUDE_OFF_FILES,
};

static PROJECT_STD_RESULT_MATCH_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::result::{Result, Ok, Err};

Config :: struct {
    value: s32,
};

AppError :: struct {
    code: s32,
};

make_ok :: () => Result<Config, AppError> {
    Ok(Config { value: 42 })
};

main :: () => {
    match make_ok() {
        Ok(c) => { const n: s32 = c.value; const _ = n; };
        Err(e) => { const _ = e; };
    };
};
",
)];

pub const PROJECT_STD_RESULT_MATCH: ProjectSpec = ProjectSpec {
    name: r"std_result_match",
    toml: r#"[project]
name = "std_result_match"
version = "0.1.0"
description = "V0-064 match on Result with struct payloads (no ? in main)"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_STD_RESULT_MATCH_FILES,
};

static PROJECT_STD_RESULT_MATCH_NON_EXHAUSTIVE_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::result::{Result, Ok, Err};

Config :: struct {
    value: s32,
};

AppError :: struct {
    code: s32,
};

make_ok :: () => Result<Config, AppError> {
    Ok(Config { value: 0 })
};

main :: () => {
    match make_ok() {
        Ok(c) => { const _ = c.value; };
    };
};
",
)];

pub const PROJECT_STD_RESULT_MATCH_NON_EXHAUSTIVE: ProjectSpec = ProjectSpec {
    name: r"std_result_match_non_exhaustive",
    toml: r#"[project]
name = "std_result_match_non_exhaustive"
version = "0.1.0"
description = "V0-064 negative: non-exhaustive Result match"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_STD_RESULT_MATCH_NON_EXHAUSTIVE_FILES,
};

static PROJECT_STD_SMOKE_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::version;
#import std::core::option::{Option, Some, None};
#import std::core::result::{Result, Ok, Err};

wrap :: (n: s32) => Option<s32> { Some :: <s32> (n) };

to_result :: () => Result<s32, s32> {
    match wrap(42) {
        None => Err :: <s32, s32> (0);
        Some(v) => Ok :: <s32, s32> (v);
    }
};

main :: () => {
    const _ = version();
    const r: Result<s32, s32> = to_result();
    const n: s32 = match wrap(42) {
        None => 0;
        Some(v) => v;
    };
    const m: s32 = match r {
        Ok(v) => v;
        Err(e) => e;
    };
    const _ignore = n;
    const _discard = m;
};
",
)];

pub const PROJECT_STD_SMOKE: ProjectSpec = ProjectSpec {
    name: r"std_smoke",
    toml: r#"[project]
name = "std_smoke"
version = "0.1.0"
description = "Smoke test consumer for bundled std package"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_STD_SMOKE_FILES,
};

static PROJECT_STD_TRAITS_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::copyable::Copyable;
#import std::core::clone::Clone;
#import std::core::cmp::PartialEq;
#import std::core::error::Error;

dup :: <t: Copyable + Clone> (x: t) => t {
    x.clone()
};

pass_err :: <e: Error> (x: e) => e {
    x
};

main :: () => {
    const n: s32 = dup(42);
    const same: bool = n.eq(42);
    const _ = same;
};
",
)];

pub const PROJECT_STD_TRAITS: ProjectSpec = ProjectSpec {
    name: r"std_traits",
    toml: r#"[project]
name = "std_traits"
version = "0.1.0"
description = "V0-043 std core traits smoke test"
type = "bin"
module_src = "src"
prelude = false

[build]
dir = "build"
"#,
    files: PROJECT_STD_TRAITS_FILES,
};

static PROJECT_STD_TRY_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r#"#import std::core::option::{Option, Some, None};
#import std::core::result::{Result, Ok, Err};

read_bytes :: (path: [u8]) => Result<s32, s32> {
    const _ignore = path;
    Ok(1)
};

read_config :: (path: [u8]) => Result<s32, s32> {
    const n = read_bytes(path)?;
    Ok(n + 41)
};

main :: () => {
    const path = b"\x00";
    match read_config(path as [u8]) {
        Ok(v) => { const _ = v; };
        Err(e) => { const _ = e; };
    };
};
"#,
)];

pub const PROJECT_STD_TRY: ProjectSpec = ProjectSpec {
    name: r"std_try",
    toml: r#"[project]
name = "std_try"
version = "0.1.0"
description = "V0-042 try operator smoke test"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_STD_TRY_FILES,
};

static PROJECT_STD_TRY_FROM_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::convert::From;
#import std::core::error::Error;
#import std::core::result::{Result, Ok, Err};

Config :: struct {
    value: s32,
};

AppIoError :: struct {
    code: s32,
};

AppIoError :: impl :: Error {
};

AppError :: enum {
    Io(AppIoError),
};

AppError :: impl :: Error {
};

AppError :: impl :: From<AppIoError> {
    from :: (value: AppIoError) => AppError {
        Io(value)
    };
};

read_bytes :: () => Result<Config, AppIoError> {
    Ok(Config { value: 42 })
};

read_config :: () => Result<Config, AppError> {
    const cfg = read_bytes()?;
    Ok(cfg)
};

main :: () => {
    match read_config() {
        Ok(c) => { const _ = c.value; };
        Err(e) => { const _ = e; };
    };
};
",
)];

pub const PROJECT_STD_TRY_FROM: ProjectSpec = ProjectSpec {
    name: r"std_try_from",
    toml: r#"[project]
name = "std_try_from"
version = "0.1.0"
description = "V0-059 try operator with From error conversion"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_STD_TRY_FROM_FILES,
};

static PROJECT_STD_TRY_FROM_MISSING_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::convert::From;
#import std::core::result::{Result, Ok, Err};

Config :: struct {
    value: s32,
};

IoError :: struct {
    code: s32,
};

Error :: enum {
    Io(IoError),
};

read_bytes :: () => Result<Config, IoError> {
    Ok(Config { value: 42 })
};

read_config :: () => Result<Config, Error> {
    const cfg = read_bytes()?;
    Ok(cfg)
};

main :: () => {
    const _ = read_config();
};
",
)];

pub const PROJECT_STD_TRY_FROM_MISSING: ProjectSpec = ProjectSpec {
    name: r"std_try_from_missing",
    toml: r#"[project]
name = "std_try_from_missing"
version = "0.1.0"
description = "V0-059 negative: missing From impl at ?"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_STD_TRY_FROM_MISSING_FILES,
};

static PROJECT_STD_TRY_OK_MISMATCH_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::result::{Result, Ok};

read_a :: () => Result<s32, s32> {
    Ok(1)
};

read_b :: () => Result<bool, s32> {
    const x = read_a()?;
    Ok(x)
};

main :: () => {
    const _ = read_b();
};
",
)];

pub const PROJECT_STD_TRY_OK_MISMATCH: ProjectSpec = ProjectSpec {
    name: r"std_try_ok_mismatch",
    toml: r#"[project]
name = "std_try_ok_mismatch"
version = "0.1.0"
description = "V0-059 negative: Ok type mismatch at ?"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_STD_TRY_OK_MISMATCH_FILES,
};

static PROJECT_STRING_CLONE_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r#"#import std::text::string::String;
#import std::core::clone::Clone;

main :: () => {
    var a: String = String::from_str("abc");
    var b: String = a.clone();
    a.push(120u as u8);
    const a_len: u32 = a.len();
    const b_len: u32 = b.len();
    const b_first: u8 = b.get(0u);
    const check_a_len: u32 = a_len;
    const check_b_len: u32 = b_len;
    const check_b_first: u8 = b_first;
};
"#,
)];

pub const PROJECT_STRING_CLONE: ProjectSpec = ProjectSpec {
    name: r"string_clone",
    toml: r#"[project]
name = "string_clone"
version = "0.1.0"
description = "Std String Clone smoke test"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_STRING_CLONE_FILES,
};

static PROJECT_STRING_FMT_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::text::fmt::to_string;

main :: () => {
    const n: s32 = 42;
    const text = to_string(n);
    const check_len = text.len();
    const _ = check_len;
};
",
)];

pub const PROJECT_STRING_FMT: ProjectSpec = ProjectSpec {
    name: r"string_fmt",
    toml: r#"[project]
name = "string_fmt"
version = "0.1.0"
description = "Std text::fmt format_s32 smoke test"
type = "bin"
module_src = "src"
prelude = false

[build]
dir = "build"
"#,
    files: PROJECT_STRING_FMT_FILES,
};

static PROJECT_STRING_FMT_BOOL_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::text::fmt::to_string;

main :: () => {
    const b: bool = true;
    const text = to_string(b);
    const check_len = text.len();
    const _ = check_len;
};
",
)];

pub const PROJECT_STRING_FMT_BOOL: ProjectSpec = ProjectSpec {
    name: r"string_fmt_bool",
    toml: r#"[project]
name = "string_fmt_bool"
version = "0.1.0"
description = "Std text::fmt bool display smoke test"
type = "bin"
module_src = "src"
prelude = false

[build]
dir = "build"
"#,
    files: PROJECT_STRING_FMT_BOOL_FILES,
};

static PROJECT_STRING_PARTIALEQ_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r#"#import std::text::string::String;

main :: () => {
    var a: String = String::from_str("hi");
    var b: String = String::from_str("hi");
    var c: String = String::from_str("ho");
    const same: bool = a.eq(&b);
    const diff: bool = a.eq(&c);
    const check_same: bool = same;
    const check_diff: bool = diff;
};
"#,
)];

pub const PROJECT_STRING_PARTIALEQ: ProjectSpec = ProjectSpec {
    name: r"string_partialeq",
    toml: r#"[project]
name = "string_partialeq"
version = "0.1.0"
description = "Std String PartialEq smoke test"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_STRING_PARTIALEQ_FILES,
};

static PROJECT_STRING_SMOKE_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r#"#import std::text::string::String;

main :: () => {
    var s: String = String::from_str("hello");
    s.push(33u as u8);
    const n: u32 = s.len();
    const first: u8 = s.get(0u);
    const last: u8 = s.get(n - 1u);
    const check_len: u32 = n;
    const check_first: u8 = first;
    const check_last: u8 = last;
};
"#,
)];

pub const PROJECT_STRING_SMOKE: ProjectSpec = ProjectSpec {
    name: r"string_smoke",
    toml: r#"[project]
name = "string_smoke"
version = "0.1.0"
description = "Std String from_str/push/len smoke test"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_STRING_SMOKE_FILES,
};

static PROJECT_TRAIT_DEFAULT_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"Counter :: struct {
    n: s32,
};

Zero :: trait {
    zero :: () => Self {
        Counter { n: 0 }
    };
};

Counter :: impl :: Zero { };

main :: () => {
    const c: Counter = Counter::zero();
    const n: s32 = c.n;
    const _ = n;
};
",
)];

pub const PROJECT_TRAIT_DEFAULT: ProjectSpec = ProjectSpec {
    name: r"trait_default",
    toml: r#"[project]
name = "trait_default"
version = "0.1.0"
description = "V0-063 trait default body inherited via empty impl"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_TRAIT_DEFAULT_FILES,
};

static PROJECT_TRAIT_DEFAULT_OVERRIDE_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"Zero :: trait {
    zero :: () => Self {
        0
    };
};

Counter :: struct {
    n: s32,
};

Counter :: impl :: Zero {
    zero :: () => Counter {
        Counter { n: 99 }
    };
};

main :: () => {
    const c: Counter = Counter::zero();
    const n: s32 = c.n;
    const _ = n;
};
",
)];

pub const PROJECT_TRAIT_DEFAULT_OVERRIDE: ProjectSpec = ProjectSpec {
    name: r"trait_default_override",
    toml: r#"[project]
name = "trait_default_override"
version = "0.1.0"
description = "V0-063 explicit impl overrides trait default body"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_TRAIT_DEFAULT_OVERRIDE_FILES,
};

static PROJECT_TRAIT_INTO_FROM_DEFAULT_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::convert::{From, Into};

Wrap :: struct {
    n: s32,
};

Out :: struct {
    v: s32,
};

Out :: impl :: From<Wrap> {
    from :: (value: Wrap) => Out {
        Out { v: value.n }
    };
};

Wrap :: impl :: Into<Out> { };

main :: () => {
    const w: Wrap = Wrap { n: 42 };
    const o: Out = w.into();
    const v: s32 = o.v;
    const _ = v;
};
",
)];

pub const PROJECT_TRAIT_INTO_FROM_DEFAULT: ProjectSpec = ProjectSpec {
    name: r"trait_into_from_default",
    toml: r#"[project]
name = "trait_into_from_default"
version = "0.1.0"
description = "V0-063 Into default body delegates to From"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_TRAIT_INTO_FROM_DEFAULT_FILES,
};

static PROJECT_UNIQUE_PTR_DOUBLE_FREE_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::memory::allocator::{Allocator, Global, Layout};
#import std::core::memory::unique_ptr::UniquePtr;
#import std::core::alloc::alloc_bytes;

main :: () => {
    const layout: Layout = Layout { size: 4u, align: 1u };
    unsafe {
        var saved_ptr: *mut s32 = alloc_bytes(4u) as *mut s32;
        {
            var ptr: UniquePtr<s32> = UniquePtr :: <s32>::new(42, layout, Global {});
            saved_ptr = ptr.as_ptr();
        };
        var mut_alloc: Global = Global {};
        mut_alloc.dealloc(saved_ptr as *mut u8, layout);
    };
};
",
)];

pub const PROJECT_UNIQUE_PTR_DOUBLE_FREE: ProjectSpec = ProjectSpec {
    name: r"unique_ptr_double_free",
    toml: r#"[project]
name = "unique_ptr_double_free"
version = "0.1.0"
description = "UniquePtr double-free via manual dealloc after Drop"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_UNIQUE_PTR_DOUBLE_FREE_FILES,
};

static PROJECT_UNIQUE_PTR_DROP_SMOKE_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::memory::allocator::{Layout, Global};
#import std::core::memory::unique_ptr::UniquePtr;

main :: () => {
    const layout: Layout = Layout { size: 4u, align: 1u };
    {
        const ptr: UniquePtr<s32> = UniquePtr :: <s32>::new(99, layout, Global {});
        const v: s32 = ptr.get();
        const _ = v;
    };
};
",
)];

pub const PROJECT_UNIQUE_PTR_DROP_SMOKE: ProjectSpec = ProjectSpec {
    name: r"unique_ptr_drop_smoke",
    toml: r#"[project]
name = "unique_ptr_drop_smoke"
version = "0.1.0"
description = "V0 UniquePtr Drop — scope exit deallocates via Allocator"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_UNIQUE_PTR_DROP_SMOKE_FILES,
};

static PROJECT_UNIQUE_PTR_MOVE_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::memory::allocator::{Layout, Global};
#import std::core::memory::unique_ptr::UniquePtr;

main :: () => {
    const layout: Layout = Layout { size: 4u, align: 1u };
    var a: UniquePtr<s32> = UniquePtr :: <s32>::new(42, layout, Global {});
    var b: UniquePtr<s32> = a;
    const v: s32 = b.get();
    const _ = v;
};
",
)];

pub const PROJECT_UNIQUE_PTR_MOVE: ProjectSpec = ProjectSpec {
    name: r"unique_ptr_move",
    toml: r#"[project]
name = "unique_ptr_move"
version = "0.1.0"
description = "UniquePtr move — only destination Drop runs at scope exit"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_UNIQUE_PTR_MOVE_FILES,
};

static PROJECT_UNIQUE_PTR_MOVE_IN_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::memory::allocator::{Layout, Global};
#import std::core::memory::unique_ptr::UniquePtr;

main :: () => {
    const layout: Layout = Layout { size: 4u, align: 1u };
    var a: UniquePtr<s32> = UniquePtr :: <s32>::new(42, layout, Global {});
    var b: UniquePtr<s32> = a;
    const _ = a.get();
};
",
)];

pub const PROJECT_UNIQUE_PTR_MOVE_IN: ProjectSpec = ProjectSpec {
    name: r"unique_ptr_move_in",
    toml: r#"[project]
name = "unique_ptr_move_in"
version = "0.1.0"
description = "UniquePtr use-after-move compile error"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_UNIQUE_PTR_MOVE_IN_FILES,
};

static PROJECT_UNIQUE_PTR_NESTED_DROP_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::memory::allocator::{Layout, Global};
#import std::core::memory::unique_ptr::UniquePtr;

// MVP heap deref is primitive-only; validates multiple UniquePtr drops at scope exit.
// Wrapper nested element drop awaits struct pointer casts (same as DynamicArray).
main :: () => {
    const layout: Layout = Layout { size: 4u, align: 1u };
    {
        var a: UniquePtr<s32> = UniquePtr :: <s32>::new(1, layout, Global {});
        var b: UniquePtr<s32> = UniquePtr :: <s32>::new(2, layout, Global {});
        const x: s32 = a.get();
        const y: s32 = b.get();
        const _ = x + y;
    };
};
",
)];

pub const PROJECT_UNIQUE_PTR_NESTED_DROP: ProjectSpec = ProjectSpec {
    name: r"unique_ptr_nested_drop",
    toml: r#"[project]
name = "unique_ptr_nested_drop"
version = "0.1.0"
description = "UniquePtr nested Drop — inner element drop before dealloc"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_UNIQUE_PTR_NESTED_DROP_FILES,
};

static PROJECT_UNIQUE_PTR_SMOKE_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::memory::allocator::{Layout, Global};
#import std::core::memory::unique_ptr::UniquePtr;

main :: () => {
    const layout: Layout = Layout { size: 4u, align: 1u };
    {
        const ptr: UniquePtr<s32> = UniquePtr :: <s32>::new(42, layout, Global {});
        const v: s32 = ptr.get();
        const _ = v;
    };
};
",
)];

pub const PROJECT_UNIQUE_PTR_SMOKE: ProjectSpec = ProjectSpec {
    name: r"unique_ptr_smoke",
    toml: r#"[project]
name = "unique_ptr_smoke"
version = "0.1.0"
description = "V0 UniquePtr smoke — allocate, deref, scope-exit Drop"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_UNIQUE_PTR_SMOKE_FILES,
};

static PROJECT_UNIQUE_PTR_UAF_FILES: &[(&str, &str)] = &[(
    r"src/main.phx",
    r"#import std::core::alloc::alloc_bytes;
#import std::core::memory::allocator::{Layout, Global};
#import std::core::memory::unique_ptr::UniquePtr;
#import std::core::slice::slice_from_raw_parts;

main :: () => {
    const layout: Layout = Layout { size: 4u, align: 1u };
    unsafe {
        var saved_ptr: *mut s32 = alloc_bytes(4u) as *mut s32;
        {
            var ptr: UniquePtr<s32> = UniquePtr :: <s32>::new(42, layout, Global {});
            saved_ptr = ptr.as_ptr();
        };
        const sl: [s32] = slice_from_raw_parts(saved_ptr, 1u);
        const v: s32 = sl[0];
        const _ = v;
    };
};
",
)];

pub const PROJECT_UNIQUE_PTR_UAF: ProjectSpec = ProjectSpec {
    name: r"unique_ptr_uaf",
    toml: r#"[project]
name = "unique_ptr_uaf"
version = "0.1.0"
description = "UniquePtr use-after-free via saved pointer after drop"
type = "bin"
module_src = "src"

[build]
dir = "build"
"#,
    files: PROJECT_UNIQUE_PTR_UAF_FILES,
};

pub const PROJECTS: &[ProjectSpec] = &[
    PROJECT_ALLOCATOR_SMOKE,
    PROJECT_ALLOCATOR_SMOKE_UNSAFE_FAIL,
    PROJECT_APP_DEP,
    PROJECT_BAD_DEP_KEY,
    PROJECT_BIN_MISSING_MAIN,
    PROJECT_DYNAMIC_ARRAY_DOUBLE_FREE,
    PROJECT_DYNAMIC_ARRAY_DROP_SMOKE,
    PROJECT_DYNAMIC_ARRAY_GROW,
    PROJECT_DYNAMIC_ARRAY_INDEX_OOB,
    PROJECT_DYNAMIC_ARRAY_MOVE_IN,
    PROJECT_DYNAMIC_ARRAY_NESTED_DROP,
    PROJECT_DYNAMIC_ARRAY_POP,
    PROJECT_DYNAMIC_ARRAY_SMOKE,
    PROJECT_DYNAMIC_ARRAY_UAF,
    PROJECT_EXTERN_C,
    PROJECT_HEAP_ALLOC,
    PROJECT_HEAP_ALLOC_OOM,
    PROJECT_HEAP_ALLOC_UNSAFE,
    PROJECT_HEAP_DEALLOC,
    PROJECT_HEAP_DEALLOC_DOUBLE,
    PROJECT_HEAP_DEALLOC_UNSAFE,
    PROJECT_HEAP_DROP_DEALLOC,
    PROJECT_HEAP_SLICE,
    PROJECT_HEAP_SLICE_NESTED_INDEX,
    PROJECT_HEAP_SLICE_OOB,
    PROJECT_HEAP_SLICE_STORE,
    PROJECT_HEAP_SLICE_UNSAFE,
    PROJECT_HEAP_UAF,
    PROJECT_NESTED_TRAP,
    PROJECT_HELLO_PRINT,
    PROJECT_LIB_WITH_MAIN,
    PROJECT_LINK_REBASE,
    PROJECT_LINT_DENY_PROJECT,
    PROJECT_LINT_STD_RESULT_DISCARD,
    PROJECT_LINT_STD_OPTION_DISCARD,
    PROJECT_MATH_LIB,
    PROJECT_MODULES_BIN_BARREL,
    PROJECT_MODULES_TRAP,
    PROJECT_MODULES_MISSING_MOD,
    PROJECT_MODULES_ORPHAN_FILE,
    PROJECT_MVP_ACCEPTANCE,
    PROJECT_NO_BUNDLE_STD,
    PROJECT_PRIMITIVE_DISPLAY,
    PROJECT_PRINT_S32,
    PROJECT_PROJECT,
    PROJECT_STD_CONVERT,
    PROJECT_STD_DERIVE,
    PROJECT_STD_ERRORS,
    PROJECT_STD_GENERIC_DERIVE,
    PROJECT_STD_ITER,
    PROJECT_STD_PLATFORM_SMOKE,
    PROJECT_STD_PRELUDE,
    PROJECT_STD_PRELUDE_OFF,
    PROJECT_STD_RESULT_MATCH,
    PROJECT_STD_RESULT_MATCH_NON_EXHAUSTIVE,
    PROJECT_STD_SMOKE,
    PROJECT_STD_TRAITS,
    PROJECT_STD_TRY,
    PROJECT_STD_TRY_FROM,
    PROJECT_STD_TRY_FROM_MISSING,
    PROJECT_STD_TRY_OK_MISMATCH,
    PROJECT_STRING_CLONE,
    PROJECT_STRING_FMT,
    PROJECT_STRING_FMT_BOOL,
    PROJECT_STRING_PARTIALEQ,
    PROJECT_STRING_SMOKE,
    PROJECT_TRAIT_DEFAULT,
    PROJECT_TRAIT_DEFAULT_OVERRIDE,
    PROJECT_TRAIT_INTO_FROM_DEFAULT,
    PROJECT_UNIQUE_PTR_DOUBLE_FREE,
    PROJECT_UNIQUE_PTR_DROP_SMOKE,
    PROJECT_UNIQUE_PTR_MOVE,
    PROJECT_UNIQUE_PTR_MOVE_IN,
    PROJECT_UNIQUE_PTR_NESTED_DROP,
    PROJECT_UNIQUE_PTR_SMOKE,
    PROJECT_UNIQUE_PTR_UAF,
];

pub fn project_by_name(name: &str) -> Option<&'static ProjectSpec> {
    PROJECTS.iter().find(|p| p.name == name)
}
