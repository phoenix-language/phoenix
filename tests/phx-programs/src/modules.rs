//! Multi-file `#import` module trees.

use super::ModuleTree;

static MODULE_BAD_DUP_FILES: &[(&str, &str)] = &[
    (
        r"bad_dup.phx",
        r"dup :: () => { };
dup :: () => { };
witness :: () => { };
",
    ),
    (
        r"block_import_main.phx",
        r"mod util;

main :: () => {
  #import util::math::add;
  const sum = add(1, 2);
  const _ = sum;
};
",
    ),
    (
        r"block_import_nested.phx",
        r"mod util;

main :: () => {
  {
    #import util::math::{ add, mul };
    const _ = add(1, mul(2, 3));
  };
};
",
    ),
    (
        r"block_import_outside.phx",
        r"mod util;

main :: () => {
  {
    #import util::math::add;
    const _ = add(1, 2);
  };
  const _ = add(0, 0);
};
",
    ),
    (
        r"cmp.phx",
        r"pub PartialEq :: trait {
    eq :: (self: &Self, other: &Self) => bool;
};
",
    ),
    (
        r"cycle_a.phx",
        r"#import cycle_b::helper;

mod cycle_b;

pub helper :: () => { };

main :: () => { helper(); };
",
    ),
    (
        r"cycle_b.phx",
        r"#import cycle_a::helper;

mod cycle_a;

pub helper :: () => { };
",
    ),
    (
        r"derive_import_main.phx",
        r"#import cmp::PartialEq;

mod cmp;

#[derive(PartialEq)]
Container :: <t> struct {
    len: u32,
};

main :: () => {
    const a = Container :: <s32> { len: 1u };
    const b = Container :: <s32> { len: 1u };
    const _: bool = a.eq(&b);
};
",
    ),
    (
        r"import_dup.phx",
        r"#import util::math::{add, add};

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"import_private.phx",
        r"#import util::secret::secret;

mod util;

main :: () => { secret(); };
",
    ),
    (
        r"main.phx",
        r"#import util::math::add;

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"main_bad_import.phx",
        r"#import bad_dup::witness;
#import util::math::add;

mod bad_dup;
mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"main_glob.phx",
        r"#import util::math::{*};

mod util;

main :: () => { const _ = add(1, mul(2, 3)); };
",
    ),
    (
        r"main_list.phx",
        r"#import util::math::{add, mul};

mod util;

main :: () => { const _ = add(1, mul(2, 3)); };
",
    ),
    (
        r"util/math.phx",
        r"pub add :: (a: s32, b: s32) => s32 { a + b };
pub mul :: (a: s32, b: s32) => s32 { a * b };
",
    ),
    (
        r"util/mod.phx",
        r"mod math;
mod secret;
",
    ),
    (
        r"util/secret.phx",
        r"secret :: () => { };
",
    ),
];

pub const MODULE_BAD_DUP: ModuleTree = ModuleTree {
    name: r"bad_dup",
    entry: r"bad_dup.phx",
    files: MODULE_BAD_DUP_FILES,
};

static MODULE_BLOCK_IMPORT_MAIN_FILES: &[(&str, &str)] = &[
    (
        r"bad_dup.phx",
        r"dup :: () => { };
dup :: () => { };
witness :: () => { };
",
    ),
    (
        r"block_import_main.phx",
        r"mod util;

main :: () => {
  #import util::math::add;
  const sum = add(1, 2);
  const _ = sum;
};
",
    ),
    (
        r"block_import_nested.phx",
        r"mod util;

main :: () => {
  {
    #import util::math::{ add, mul };
    const _ = add(1, mul(2, 3));
  };
};
",
    ),
    (
        r"block_import_outside.phx",
        r"mod util;

main :: () => {
  {
    #import util::math::add;
    const _ = add(1, 2);
  };
  const _ = add(0, 0);
};
",
    ),
    (
        r"cmp.phx",
        r"pub PartialEq :: trait {
    eq :: (self: &Self, other: &Self) => bool;
};
",
    ),
    (
        r"cycle_a.phx",
        r"#import cycle_b::helper;

mod cycle_b;

pub helper :: () => { };

main :: () => { helper(); };
",
    ),
    (
        r"cycle_b.phx",
        r"#import cycle_a::helper;

mod cycle_a;

pub helper :: () => { };
",
    ),
    (
        r"derive_import_main.phx",
        r"#import cmp::PartialEq;

mod cmp;

#[derive(PartialEq)]
Container :: <t> struct {
    len: u32,
};

main :: () => {
    const a = Container :: <s32> { len: 1u };
    const b = Container :: <s32> { len: 1u };
    const _: bool = a.eq(&b);
};
",
    ),
    (
        r"import_dup.phx",
        r"#import util::math::{add, add};

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"import_private.phx",
        r"#import util::secret::secret;

mod util;

main :: () => { secret(); };
",
    ),
    (
        r"main.phx",
        r"#import util::math::add;

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"main_bad_import.phx",
        r"#import bad_dup::witness;
#import util::math::add;

mod bad_dup;
mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"main_glob.phx",
        r"#import util::math::{*};

mod util;

main :: () => { const _ = add(1, mul(2, 3)); };
",
    ),
    (
        r"main_list.phx",
        r"#import util::math::{add, mul};

mod util;

main :: () => { const _ = add(1, mul(2, 3)); };
",
    ),
    (
        r"util/math.phx",
        r"pub add :: (a: s32, b: s32) => s32 { a + b };
pub mul :: (a: s32, b: s32) => s32 { a * b };
",
    ),
    (
        r"util/mod.phx",
        r"mod math;
mod secret;
",
    ),
    (
        r"util/secret.phx",
        r"secret :: () => { };
",
    ),
];

pub const MODULE_BLOCK_IMPORT_MAIN: ModuleTree = ModuleTree {
    name: r"block_import_main",
    entry: r"block_import_main.phx",
    files: MODULE_BLOCK_IMPORT_MAIN_FILES,
};

static MODULE_BLOCK_IMPORT_NESTED_FILES: &[(&str, &str)] = &[
    (
        r"bad_dup.phx",
        r"dup :: () => { };
dup :: () => { };
witness :: () => { };
",
    ),
    (
        r"block_import_main.phx",
        r"mod util;

main :: () => {
  #import util::math::add;
  const sum = add(1, 2);
  const _ = sum;
};
",
    ),
    (
        r"block_import_nested.phx",
        r"mod util;

main :: () => {
  {
    #import util::math::{ add, mul };
    const _ = add(1, mul(2, 3));
  };
};
",
    ),
    (
        r"block_import_outside.phx",
        r"mod util;

main :: () => {
  {
    #import util::math::add;
    const _ = add(1, 2);
  };
  const _ = add(0, 0);
};
",
    ),
    (
        r"cmp.phx",
        r"pub PartialEq :: trait {
    eq :: (self: &Self, other: &Self) => bool;
};
",
    ),
    (
        r"cycle_a.phx",
        r"#import cycle_b::helper;

mod cycle_b;

pub helper :: () => { };

main :: () => { helper(); };
",
    ),
    (
        r"cycle_b.phx",
        r"#import cycle_a::helper;

mod cycle_a;

pub helper :: () => { };
",
    ),
    (
        r"derive_import_main.phx",
        r"#import cmp::PartialEq;

mod cmp;

#[derive(PartialEq)]
Container :: <t> struct {
    len: u32,
};

main :: () => {
    const a = Container :: <s32> { len: 1u };
    const b = Container :: <s32> { len: 1u };
    const _: bool = a.eq(&b);
};
",
    ),
    (
        r"import_dup.phx",
        r"#import util::math::{add, add};

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"import_private.phx",
        r"#import util::secret::secret;

mod util;

main :: () => { secret(); };
",
    ),
    (
        r"main.phx",
        r"#import util::math::add;

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"main_bad_import.phx",
        r"#import bad_dup::witness;
#import util::math::add;

mod bad_dup;
mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"main_glob.phx",
        r"#import util::math::{*};

mod util;

main :: () => { const _ = add(1, mul(2, 3)); };
",
    ),
    (
        r"main_list.phx",
        r"#import util::math::{add, mul};

mod util;

main :: () => { const _ = add(1, mul(2, 3)); };
",
    ),
    (
        r"util/math.phx",
        r"pub add :: (a: s32, b: s32) => s32 { a + b };
pub mul :: (a: s32, b: s32) => s32 { a * b };
",
    ),
    (
        r"util/mod.phx",
        r"mod math;
mod secret;
",
    ),
    (
        r"util/secret.phx",
        r"secret :: () => { };
",
    ),
];

pub const MODULE_BLOCK_IMPORT_NESTED: ModuleTree = ModuleTree {
    name: r"block_import_nested",
    entry: r"block_import_nested.phx",
    files: MODULE_BLOCK_IMPORT_NESTED_FILES,
};

static MODULE_BLOCK_IMPORT_OUTSIDE_FILES: &[(&str, &str)] = &[
    (
        r"bad_dup.phx",
        r"dup :: () => { };
dup :: () => { };
witness :: () => { };
",
    ),
    (
        r"block_import_main.phx",
        r"mod util;

main :: () => {
  #import util::math::add;
  const sum = add(1, 2);
  const _ = sum;
};
",
    ),
    (
        r"block_import_nested.phx",
        r"mod util;

main :: () => {
  {
    #import util::math::{ add, mul };
    const _ = add(1, mul(2, 3));
  };
};
",
    ),
    (
        r"block_import_outside.phx",
        r"mod util;

main :: () => {
  {
    #import util::math::add;
    const _ = add(1, 2);
  };
  const _ = add(0, 0);
};
",
    ),
    (
        r"cmp.phx",
        r"pub PartialEq :: trait {
    eq :: (self: &Self, other: &Self) => bool;
};
",
    ),
    (
        r"cycle_a.phx",
        r"#import cycle_b::helper;

mod cycle_b;

pub helper :: () => { };

main :: () => { helper(); };
",
    ),
    (
        r"cycle_b.phx",
        r"#import cycle_a::helper;

mod cycle_a;

pub helper :: () => { };
",
    ),
    (
        r"derive_import_main.phx",
        r"#import cmp::PartialEq;

mod cmp;

#[derive(PartialEq)]
Container :: <t> struct {
    len: u32,
};

main :: () => {
    const a = Container :: <s32> { len: 1u };
    const b = Container :: <s32> { len: 1u };
    const _: bool = a.eq(&b);
};
",
    ),
    (
        r"import_dup.phx",
        r"#import util::math::{add, add};

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"import_private.phx",
        r"#import util::secret::secret;

mod util;

main :: () => { secret(); };
",
    ),
    (
        r"main.phx",
        r"#import util::math::add;

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"main_bad_import.phx",
        r"#import bad_dup::witness;
#import util::math::add;

mod bad_dup;
mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"main_glob.phx",
        r"#import util::math::{*};

mod util;

main :: () => { const _ = add(1, mul(2, 3)); };
",
    ),
    (
        r"main_list.phx",
        r"#import util::math::{add, mul};

mod util;

main :: () => { const _ = add(1, mul(2, 3)); };
",
    ),
    (
        r"util/math.phx",
        r"pub add :: (a: s32, b: s32) => s32 { a + b };
pub mul :: (a: s32, b: s32) => s32 { a * b };
",
    ),
    (
        r"util/mod.phx",
        r"mod math;
mod secret;
",
    ),
    (
        r"util/secret.phx",
        r"secret :: () => { };
",
    ),
];

pub const MODULE_BLOCK_IMPORT_OUTSIDE: ModuleTree = ModuleTree {
    name: r"block_import_outside",
    entry: r"block_import_outside.phx",
    files: MODULE_BLOCK_IMPORT_OUTSIDE_FILES,
};

static MODULE_CMP_FILES: &[(&str, &str)] = &[
    (
        r"bad_dup.phx",
        r"dup :: () => { };
dup :: () => { };
witness :: () => { };
",
    ),
    (
        r"block_import_main.phx",
        r"mod util;

main :: () => {
  #import util::math::add;
  const sum = add(1, 2);
  const _ = sum;
};
",
    ),
    (
        r"block_import_nested.phx",
        r"mod util;

main :: () => {
  {
    #import util::math::{ add, mul };
    const _ = add(1, mul(2, 3));
  };
};
",
    ),
    (
        r"block_import_outside.phx",
        r"mod util;

main :: () => {
  {
    #import util::math::add;
    const _ = add(1, 2);
  };
  const _ = add(0, 0);
};
",
    ),
    (
        r"cmp.phx",
        r"pub PartialEq :: trait {
    eq :: (self: &Self, other: &Self) => bool;
};
",
    ),
    (
        r"cycle_a.phx",
        r"#import cycle_b::helper;

mod cycle_b;

pub helper :: () => { };

main :: () => { helper(); };
",
    ),
    (
        r"cycle_b.phx",
        r"#import cycle_a::helper;

mod cycle_a;

pub helper :: () => { };
",
    ),
    (
        r"derive_import_main.phx",
        r"#import cmp::PartialEq;

mod cmp;

#[derive(PartialEq)]
Container :: <t> struct {
    len: u32,
};

main :: () => {
    const a = Container :: <s32> { len: 1u };
    const b = Container :: <s32> { len: 1u };
    const _: bool = a.eq(&b);
};
",
    ),
    (
        r"import_dup.phx",
        r"#import util::math::{add, add};

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"import_private.phx",
        r"#import util::secret::secret;

mod util;

main :: () => { secret(); };
",
    ),
    (
        r"main.phx",
        r"#import util::math::add;

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"main_bad_import.phx",
        r"#import bad_dup::witness;
#import util::math::add;

mod bad_dup;
mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"main_glob.phx",
        r"#import util::math::{*};

mod util;

main :: () => { const _ = add(1, mul(2, 3)); };
",
    ),
    (
        r"main_list.phx",
        r"#import util::math::{add, mul};

mod util;

main :: () => { const _ = add(1, mul(2, 3)); };
",
    ),
    (
        r"util/math.phx",
        r"pub add :: (a: s32, b: s32) => s32 { a + b };
pub mul :: (a: s32, b: s32) => s32 { a * b };
",
    ),
    (
        r"util/mod.phx",
        r"mod math;
mod secret;
",
    ),
    (
        r"util/secret.phx",
        r"secret :: () => { };
",
    ),
];

pub const MODULE_CMP: ModuleTree = ModuleTree {
    name: r"cmp",
    entry: r"cmp.phx",
    files: MODULE_CMP_FILES,
};

static MODULE_CYCLE_A_FILES: &[(&str, &str)] = &[
    (
        r"bad_dup.phx",
        r"dup :: () => { };
dup :: () => { };
witness :: () => { };
",
    ),
    (
        r"block_import_main.phx",
        r"mod util;

main :: () => {
  #import util::math::add;
  const sum = add(1, 2);
  const _ = sum;
};
",
    ),
    (
        r"block_import_nested.phx",
        r"mod util;

main :: () => {
  {
    #import util::math::{ add, mul };
    const _ = add(1, mul(2, 3));
  };
};
",
    ),
    (
        r"block_import_outside.phx",
        r"mod util;

main :: () => {
  {
    #import util::math::add;
    const _ = add(1, 2);
  };
  const _ = add(0, 0);
};
",
    ),
    (
        r"cmp.phx",
        r"pub PartialEq :: trait {
    eq :: (self: &Self, other: &Self) => bool;
};
",
    ),
    (
        r"cycle_a.phx",
        r"#import cycle_b::helper;

mod cycle_b;

pub helper :: () => { };

main :: () => { helper(); };
",
    ),
    (
        r"cycle_b.phx",
        r"#import cycle_a::helper;

mod cycle_a;

pub helper :: () => { };
",
    ),
    (
        r"derive_import_main.phx",
        r"#import cmp::PartialEq;

mod cmp;

#[derive(PartialEq)]
Container :: <t> struct {
    len: u32,
};

main :: () => {
    const a = Container :: <s32> { len: 1u };
    const b = Container :: <s32> { len: 1u };
    const _: bool = a.eq(&b);
};
",
    ),
    (
        r"import_dup.phx",
        r"#import util::math::{add, add};

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"import_private.phx",
        r"#import util::secret::secret;

mod util;

main :: () => { secret(); };
",
    ),
    (
        r"main.phx",
        r"#import util::math::add;

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"main_bad_import.phx",
        r"#import bad_dup::witness;
#import util::math::add;

mod bad_dup;
mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"main_glob.phx",
        r"#import util::math::{*};

mod util;

main :: () => { const _ = add(1, mul(2, 3)); };
",
    ),
    (
        r"main_list.phx",
        r"#import util::math::{add, mul};

mod util;

main :: () => { const _ = add(1, mul(2, 3)); };
",
    ),
    (
        r"util/math.phx",
        r"pub add :: (a: s32, b: s32) => s32 { a + b };
pub mul :: (a: s32, b: s32) => s32 { a * b };
",
    ),
    (
        r"util/mod.phx",
        r"mod math;
mod secret;
",
    ),
    (
        r"util/secret.phx",
        r"secret :: () => { };
",
    ),
];

pub const MODULE_CYCLE_A: ModuleTree = ModuleTree {
    name: r"cycle_a",
    entry: r"cycle_a.phx",
    files: MODULE_CYCLE_A_FILES,
};

static MODULE_CYCLE_B_FILES: &[(&str, &str)] = &[
    (
        r"bad_dup.phx",
        r"dup :: () => { };
dup :: () => { };
witness :: () => { };
",
    ),
    (
        r"block_import_main.phx",
        r"mod util;

main :: () => {
  #import util::math::add;
  const sum = add(1, 2);
  const _ = sum;
};
",
    ),
    (
        r"block_import_nested.phx",
        r"mod util;

main :: () => {
  {
    #import util::math::{ add, mul };
    const _ = add(1, mul(2, 3));
  };
};
",
    ),
    (
        r"block_import_outside.phx",
        r"mod util;

main :: () => {
  {
    #import util::math::add;
    const _ = add(1, 2);
  };
  const _ = add(0, 0);
};
",
    ),
    (
        r"cmp.phx",
        r"pub PartialEq :: trait {
    eq :: (self: &Self, other: &Self) => bool;
};
",
    ),
    (
        r"cycle_a.phx",
        r"#import cycle_b::helper;

mod cycle_b;

pub helper :: () => { };

main :: () => { helper(); };
",
    ),
    (
        r"cycle_b.phx",
        r"#import cycle_a::helper;

mod cycle_a;

pub helper :: () => { };
",
    ),
    (
        r"derive_import_main.phx",
        r"#import cmp::PartialEq;

mod cmp;

#[derive(PartialEq)]
Container :: <t> struct {
    len: u32,
};

main :: () => {
    const a = Container :: <s32> { len: 1u };
    const b = Container :: <s32> { len: 1u };
    const _: bool = a.eq(&b);
};
",
    ),
    (
        r"import_dup.phx",
        r"#import util::math::{add, add};

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"import_private.phx",
        r"#import util::secret::secret;

mod util;

main :: () => { secret(); };
",
    ),
    (
        r"main.phx",
        r"#import util::math::add;

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"main_bad_import.phx",
        r"#import bad_dup::witness;
#import util::math::add;

mod bad_dup;
mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"main_glob.phx",
        r"#import util::math::{*};

mod util;

main :: () => { const _ = add(1, mul(2, 3)); };
",
    ),
    (
        r"main_list.phx",
        r"#import util::math::{add, mul};

mod util;

main :: () => { const _ = add(1, mul(2, 3)); };
",
    ),
    (
        r"util/math.phx",
        r"pub add :: (a: s32, b: s32) => s32 { a + b };
pub mul :: (a: s32, b: s32) => s32 { a * b };
",
    ),
    (
        r"util/mod.phx",
        r"mod math;
mod secret;
",
    ),
    (
        r"util/secret.phx",
        r"secret :: () => { };
",
    ),
];

pub const MODULE_CYCLE_B: ModuleTree = ModuleTree {
    name: r"cycle_b",
    entry: r"cycle_b.phx",
    files: MODULE_CYCLE_B_FILES,
};

static MODULE_DERIVE_IMPORT_MAIN_FILES: &[(&str, &str)] = &[
    (
        r"bad_dup.phx",
        r"dup :: () => { };
dup :: () => { };
witness :: () => { };
",
    ),
    (
        r"block_import_main.phx",
        r"mod util;

main :: () => {
  #import util::math::add;
  const sum = add(1, 2);
  const _ = sum;
};
",
    ),
    (
        r"block_import_nested.phx",
        r"mod util;

main :: () => {
  {
    #import util::math::{ add, mul };
    const _ = add(1, mul(2, 3));
  };
};
",
    ),
    (
        r"block_import_outside.phx",
        r"mod util;

main :: () => {
  {
    #import util::math::add;
    const _ = add(1, 2);
  };
  const _ = add(0, 0);
};
",
    ),
    (
        r"cmp.phx",
        r"pub PartialEq :: trait {
    eq :: (self: &Self, other: &Self) => bool;
};
",
    ),
    (
        r"cycle_a.phx",
        r"#import cycle_b::helper;

mod cycle_b;

pub helper :: () => { };

main :: () => { helper(); };
",
    ),
    (
        r"cycle_b.phx",
        r"#import cycle_a::helper;

mod cycle_a;

pub helper :: () => { };
",
    ),
    (
        r"derive_import_main.phx",
        r"#import cmp::PartialEq;

mod cmp;

#[derive(PartialEq)]
Container :: <t> struct {
    len: u32,
};

main :: () => {
    const a = Container :: <s32> { len: 1u };
    const b = Container :: <s32> { len: 1u };
    const _: bool = a.eq(&b);
};
",
    ),
    (
        r"import_dup.phx",
        r"#import util::math::{add, add};

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"import_private.phx",
        r"#import util::secret::secret;

mod util;

main :: () => { secret(); };
",
    ),
    (
        r"main.phx",
        r"#import util::math::add;

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"main_bad_import.phx",
        r"#import bad_dup::witness;
#import util::math::add;

mod bad_dup;
mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"main_glob.phx",
        r"#import util::math::{*};

mod util;

main :: () => { const _ = add(1, mul(2, 3)); };
",
    ),
    (
        r"main_list.phx",
        r"#import util::math::{add, mul};

mod util;

main :: () => { const _ = add(1, mul(2, 3)); };
",
    ),
    (
        r"util/math.phx",
        r"pub add :: (a: s32, b: s32) => s32 { a + b };
pub mul :: (a: s32, b: s32) => s32 { a * b };
",
    ),
    (
        r"util/mod.phx",
        r"mod math;
mod secret;
",
    ),
    (
        r"util/secret.phx",
        r"secret :: () => { };
",
    ),
];

pub const MODULE_DERIVE_IMPORT_MAIN: ModuleTree = ModuleTree {
    name: r"derive_import_main",
    entry: r"derive_import_main.phx",
    files: MODULE_DERIVE_IMPORT_MAIN_FILES,
};

static MODULE_IMPORT_DUP_FILES: &[(&str, &str)] = &[
    (
        r"bad_dup.phx",
        r"dup :: () => { };
dup :: () => { };
witness :: () => { };
",
    ),
    (
        r"block_import_main.phx",
        r"mod util;

main :: () => {
  #import util::math::add;
  const sum = add(1, 2);
  const _ = sum;
};
",
    ),
    (
        r"block_import_nested.phx",
        r"mod util;

main :: () => {
  {
    #import util::math::{ add, mul };
    const _ = add(1, mul(2, 3));
  };
};
",
    ),
    (
        r"block_import_outside.phx",
        r"mod util;

main :: () => {
  {
    #import util::math::add;
    const _ = add(1, 2);
  };
  const _ = add(0, 0);
};
",
    ),
    (
        r"cmp.phx",
        r"pub PartialEq :: trait {
    eq :: (self: &Self, other: &Self) => bool;
};
",
    ),
    (
        r"cycle_a.phx",
        r"#import cycle_b::helper;

mod cycle_b;

pub helper :: () => { };

main :: () => { helper(); };
",
    ),
    (
        r"cycle_b.phx",
        r"#import cycle_a::helper;

mod cycle_a;

pub helper :: () => { };
",
    ),
    (
        r"derive_import_main.phx",
        r"#import cmp::PartialEq;

mod cmp;

#[derive(PartialEq)]
Container :: <t> struct {
    len: u32,
};

main :: () => {
    const a = Container :: <s32> { len: 1u };
    const b = Container :: <s32> { len: 1u };
    const _: bool = a.eq(&b);
};
",
    ),
    (
        r"import_dup.phx",
        r"#import util::math::{add, add};

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"import_private.phx",
        r"#import util::secret::secret;

mod util;

main :: () => { secret(); };
",
    ),
    (
        r"main.phx",
        r"#import util::math::add;

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"main_bad_import.phx",
        r"#import bad_dup::witness;
#import util::math::add;

mod bad_dup;
mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"main_glob.phx",
        r"#import util::math::{*};

mod util;

main :: () => { const _ = add(1, mul(2, 3)); };
",
    ),
    (
        r"main_list.phx",
        r"#import util::math::{add, mul};

mod util;

main :: () => { const _ = add(1, mul(2, 3)); };
",
    ),
    (
        r"util/math.phx",
        r"pub add :: (a: s32, b: s32) => s32 { a + b };
pub mul :: (a: s32, b: s32) => s32 { a * b };
",
    ),
    (
        r"util/mod.phx",
        r"mod math;
mod secret;
",
    ),
    (
        r"util/secret.phx",
        r"secret :: () => { };
",
    ),
];

pub const MODULE_IMPORT_DUP: ModuleTree = ModuleTree {
    name: r"import_dup",
    entry: r"import_dup.phx",
    files: MODULE_IMPORT_DUP_FILES,
};

static MODULE_IMPORT_PRIVATE_FILES: &[(&str, &str)] = &[
    (
        r"bad_dup.phx",
        r"dup :: () => { };
dup :: () => { };
witness :: () => { };
",
    ),
    (
        r"block_import_main.phx",
        r"mod util;

main :: () => {
  #import util::math::add;
  const sum = add(1, 2);
  const _ = sum;
};
",
    ),
    (
        r"block_import_nested.phx",
        r"mod util;

main :: () => {
  {
    #import util::math::{ add, mul };
    const _ = add(1, mul(2, 3));
  };
};
",
    ),
    (
        r"block_import_outside.phx",
        r"mod util;

main :: () => {
  {
    #import util::math::add;
    const _ = add(1, 2);
  };
  const _ = add(0, 0);
};
",
    ),
    (
        r"cmp.phx",
        r"pub PartialEq :: trait {
    eq :: (self: &Self, other: &Self) => bool;
};
",
    ),
    (
        r"cycle_a.phx",
        r"#import cycle_b::helper;

mod cycle_b;

pub helper :: () => { };

main :: () => { helper(); };
",
    ),
    (
        r"cycle_b.phx",
        r"#import cycle_a::helper;

mod cycle_a;

pub helper :: () => { };
",
    ),
    (
        r"derive_import_main.phx",
        r"#import cmp::PartialEq;

mod cmp;

#[derive(PartialEq)]
Container :: <t> struct {
    len: u32,
};

main :: () => {
    const a = Container :: <s32> { len: 1u };
    const b = Container :: <s32> { len: 1u };
    const _: bool = a.eq(&b);
};
",
    ),
    (
        r"import_dup.phx",
        r"#import util::math::{add, add};

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"import_private.phx",
        r"#import util::secret::secret;

mod util;

main :: () => { secret(); };
",
    ),
    (
        r"main.phx",
        r"#import util::math::add;

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"main_bad_import.phx",
        r"#import bad_dup::witness;
#import util::math::add;

mod bad_dup;
mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"main_glob.phx",
        r"#import util::math::{*};

mod util;

main :: () => { const _ = add(1, mul(2, 3)); };
",
    ),
    (
        r"main_list.phx",
        r"#import util::math::{add, mul};

mod util;

main :: () => { const _ = add(1, mul(2, 3)); };
",
    ),
    (
        r"util/math.phx",
        r"pub add :: (a: s32, b: s32) => s32 { a + b };
pub mul :: (a: s32, b: s32) => s32 { a * b };
",
    ),
    (
        r"util/mod.phx",
        r"mod math;
mod secret;
",
    ),
    (
        r"util/secret.phx",
        r"secret :: () => { };
",
    ),
];

pub const MODULE_IMPORT_PRIVATE: ModuleTree = ModuleTree {
    name: r"import_private",
    entry: r"import_private.phx",
    files: MODULE_IMPORT_PRIVATE_FILES,
};

static MODULE_MAIN_FILES: &[(&str, &str)] = &[
    (
        r"bad_dup.phx",
        r"dup :: () => { };
dup :: () => { };
witness :: () => { };
",
    ),
    (
        r"block_import_main.phx",
        r"mod util;

main :: () => {
  #import util::math::add;
  const sum = add(1, 2);
  const _ = sum;
};
",
    ),
    (
        r"block_import_nested.phx",
        r"mod util;

main :: () => {
  {
    #import util::math::{ add, mul };
    const _ = add(1, mul(2, 3));
  };
};
",
    ),
    (
        r"block_import_outside.phx",
        r"mod util;

main :: () => {
  {
    #import util::math::add;
    const _ = add(1, 2);
  };
  const _ = add(0, 0);
};
",
    ),
    (
        r"cmp.phx",
        r"pub PartialEq :: trait {
    eq :: (self: &Self, other: &Self) => bool;
};
",
    ),
    (
        r"cycle_a.phx",
        r"#import cycle_b::helper;

mod cycle_b;

pub helper :: () => { };

main :: () => { helper(); };
",
    ),
    (
        r"cycle_b.phx",
        r"#import cycle_a::helper;

mod cycle_a;

pub helper :: () => { };
",
    ),
    (
        r"derive_import_main.phx",
        r"#import cmp::PartialEq;

mod cmp;

#[derive(PartialEq)]
Container :: <t> struct {
    len: u32,
};

main :: () => {
    const a = Container :: <s32> { len: 1u };
    const b = Container :: <s32> { len: 1u };
    const _: bool = a.eq(&b);
};
",
    ),
    (
        r"import_dup.phx",
        r"#import util::math::{add, add};

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"import_private.phx",
        r"#import util::secret::secret;

mod util;

main :: () => { secret(); };
",
    ),
    (
        r"main.phx",
        r"#import util::math::add;

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"main_bad_import.phx",
        r"#import bad_dup::witness;
#import util::math::add;

mod bad_dup;
mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"main_glob.phx",
        r"#import util::math::{*};

mod util;

main :: () => { const _ = add(1, mul(2, 3)); };
",
    ),
    (
        r"main_list.phx",
        r"#import util::math::{add, mul};

mod util;

main :: () => { const _ = add(1, mul(2, 3)); };
",
    ),
    (
        r"util/math.phx",
        r"pub add :: (a: s32, b: s32) => s32 { a + b };
pub mul :: (a: s32, b: s32) => s32 { a * b };
",
    ),
    (
        r"util/mod.phx",
        r"mod math;
mod secret;
",
    ),
    (
        r"util/secret.phx",
        r"secret :: () => { };
",
    ),
];

pub const MODULE_MAIN: ModuleTree = ModuleTree {
    name: r"main",
    entry: r"main.phx",
    files: MODULE_MAIN_FILES,
};

static MODULE_MAIN_BAD_IMPORT_FILES: &[(&str, &str)] = &[
    (
        r"bad_dup.phx",
        r"dup :: () => { };
dup :: () => { };
witness :: () => { };
",
    ),
    (
        r"block_import_main.phx",
        r"mod util;

main :: () => {
  #import util::math::add;
  const sum = add(1, 2);
  const _ = sum;
};
",
    ),
    (
        r"block_import_nested.phx",
        r"mod util;

main :: () => {
  {
    #import util::math::{ add, mul };
    const _ = add(1, mul(2, 3));
  };
};
",
    ),
    (
        r"block_import_outside.phx",
        r"mod util;

main :: () => {
  {
    #import util::math::add;
    const _ = add(1, 2);
  };
  const _ = add(0, 0);
};
",
    ),
    (
        r"cmp.phx",
        r"pub PartialEq :: trait {
    eq :: (self: &Self, other: &Self) => bool;
};
",
    ),
    (
        r"cycle_a.phx",
        r"#import cycle_b::helper;

mod cycle_b;

pub helper :: () => { };

main :: () => { helper(); };
",
    ),
    (
        r"cycle_b.phx",
        r"#import cycle_a::helper;

mod cycle_a;

pub helper :: () => { };
",
    ),
    (
        r"derive_import_main.phx",
        r"#import cmp::PartialEq;

mod cmp;

#[derive(PartialEq)]
Container :: <t> struct {
    len: u32,
};

main :: () => {
    const a = Container :: <s32> { len: 1u };
    const b = Container :: <s32> { len: 1u };
    const _: bool = a.eq(&b);
};
",
    ),
    (
        r"import_dup.phx",
        r"#import util::math::{add, add};

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"import_private.phx",
        r"#import util::secret::secret;

mod util;

main :: () => { secret(); };
",
    ),
    (
        r"main.phx",
        r"#import util::math::add;

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"main_bad_import.phx",
        r"#import bad_dup::witness;
#import util::math::add;

mod bad_dup;
mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"main_glob.phx",
        r"#import util::math::{*};

mod util;

main :: () => { const _ = add(1, mul(2, 3)); };
",
    ),
    (
        r"main_list.phx",
        r"#import util::math::{add, mul};

mod util;

main :: () => { const _ = add(1, mul(2, 3)); };
",
    ),
    (
        r"util/math.phx",
        r"pub add :: (a: s32, b: s32) => s32 { a + b };
pub mul :: (a: s32, b: s32) => s32 { a * b };
",
    ),
    (
        r"util/mod.phx",
        r"mod math;
mod secret;
",
    ),
    (
        r"util/secret.phx",
        r"secret :: () => { };
",
    ),
];

pub const MODULE_MAIN_BAD_IMPORT: ModuleTree = ModuleTree {
    name: r"main_bad_import",
    entry: r"main_bad_import.phx",
    files: MODULE_MAIN_BAD_IMPORT_FILES,
};

static MODULE_MAIN_GLOB_FILES: &[(&str, &str)] = &[
    (
        r"bad_dup.phx",
        r"dup :: () => { };
dup :: () => { };
witness :: () => { };
",
    ),
    (
        r"block_import_main.phx",
        r"mod util;

main :: () => {
  #import util::math::add;
  const sum = add(1, 2);
  const _ = sum;
};
",
    ),
    (
        r"block_import_nested.phx",
        r"mod util;

main :: () => {
  {
    #import util::math::{ add, mul };
    const _ = add(1, mul(2, 3));
  };
};
",
    ),
    (
        r"block_import_outside.phx",
        r"mod util;

main :: () => {
  {
    #import util::math::add;
    const _ = add(1, 2);
  };
  const _ = add(0, 0);
};
",
    ),
    (
        r"cmp.phx",
        r"pub PartialEq :: trait {
    eq :: (self: &Self, other: &Self) => bool;
};
",
    ),
    (
        r"cycle_a.phx",
        r"#import cycle_b::helper;

mod cycle_b;

pub helper :: () => { };

main :: () => { helper(); };
",
    ),
    (
        r"cycle_b.phx",
        r"#import cycle_a::helper;

mod cycle_a;

pub helper :: () => { };
",
    ),
    (
        r"derive_import_main.phx",
        r"#import cmp::PartialEq;

mod cmp;

#[derive(PartialEq)]
Container :: <t> struct {
    len: u32,
};

main :: () => {
    const a = Container :: <s32> { len: 1u };
    const b = Container :: <s32> { len: 1u };
    const _: bool = a.eq(&b);
};
",
    ),
    (
        r"import_dup.phx",
        r"#import util::math::{add, add};

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"import_private.phx",
        r"#import util::secret::secret;

mod util;

main :: () => { secret(); };
",
    ),
    (
        r"main.phx",
        r"#import util::math::add;

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"main_bad_import.phx",
        r"#import bad_dup::witness;
#import util::math::add;

mod bad_dup;
mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"main_glob.phx",
        r"#import util::math::{*};

mod util;

main :: () => { const _ = add(1, mul(2, 3)); };
",
    ),
    (
        r"main_list.phx",
        r"#import util::math::{add, mul};

mod util;

main :: () => { const _ = add(1, mul(2, 3)); };
",
    ),
    (
        r"util/math.phx",
        r"pub add :: (a: s32, b: s32) => s32 { a + b };
pub mul :: (a: s32, b: s32) => s32 { a * b };
",
    ),
    (
        r"util/mod.phx",
        r"mod math;
mod secret;
",
    ),
    (
        r"util/secret.phx",
        r"secret :: () => { };
",
    ),
];

pub const MODULE_MAIN_GLOB: ModuleTree = ModuleTree {
    name: r"main_glob",
    entry: r"main_glob.phx",
    files: MODULE_MAIN_GLOB_FILES,
};

static MODULE_MAIN_LIST_FILES: &[(&str, &str)] = &[
    (
        r"bad_dup.phx",
        r"dup :: () => { };
dup :: () => { };
witness :: () => { };
",
    ),
    (
        r"block_import_main.phx",
        r"mod util;

main :: () => {
  #import util::math::add;
  const sum = add(1, 2);
  const _ = sum;
};
",
    ),
    (
        r"block_import_nested.phx",
        r"mod util;

main :: () => {
  {
    #import util::math::{ add, mul };
    const _ = add(1, mul(2, 3));
  };
};
",
    ),
    (
        r"block_import_outside.phx",
        r"mod util;

main :: () => {
  {
    #import util::math::add;
    const _ = add(1, 2);
  };
  const _ = add(0, 0);
};
",
    ),
    (
        r"cmp.phx",
        r"pub PartialEq :: trait {
    eq :: (self: &Self, other: &Self) => bool;
};
",
    ),
    (
        r"cycle_a.phx",
        r"#import cycle_b::helper;

mod cycle_b;

pub helper :: () => { };

main :: () => { helper(); };
",
    ),
    (
        r"cycle_b.phx",
        r"#import cycle_a::helper;

mod cycle_a;

pub helper :: () => { };
",
    ),
    (
        r"derive_import_main.phx",
        r"#import cmp::PartialEq;

mod cmp;

#[derive(PartialEq)]
Container :: <t> struct {
    len: u32,
};

main :: () => {
    const a = Container :: <s32> { len: 1u };
    const b = Container :: <s32> { len: 1u };
    const _: bool = a.eq(&b);
};
",
    ),
    (
        r"import_dup.phx",
        r"#import util::math::{add, add};

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"import_private.phx",
        r"#import util::secret::secret;

mod util;

main :: () => { secret(); };
",
    ),
    (
        r"main.phx",
        r"#import util::math::add;

mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"main_bad_import.phx",
        r"#import bad_dup::witness;
#import util::math::add;

mod bad_dup;
mod util;

main :: () => { const _ = add(1, 2); };
",
    ),
    (
        r"main_glob.phx",
        r"#import util::math::{*};

mod util;

main :: () => { const _ = add(1, mul(2, 3)); };
",
    ),
    (
        r"main_list.phx",
        r"#import util::math::{add, mul};

mod util;

main :: () => { const _ = add(1, mul(2, 3)); };
",
    ),
    (
        r"util/math.phx",
        r"pub add :: (a: s32, b: s32) => s32 { a + b };
pub mul :: (a: s32, b: s32) => s32 { a * b };
",
    ),
    (
        r"util/mod.phx",
        r"mod math;
mod secret;
",
    ),
    (
        r"util/secret.phx",
        r"secret :: () => { };
",
    ),
];

pub const MODULE_MAIN_LIST: ModuleTree = ModuleTree {
    name: r"main_list",
    entry: r"main_list.phx",
    files: MODULE_MAIN_LIST_FILES,
};

pub const MODULE_TREES: &[ModuleTree] = &[
    MODULE_BAD_DUP,
    MODULE_BLOCK_IMPORT_MAIN,
    MODULE_BLOCK_IMPORT_NESTED,
    MODULE_BLOCK_IMPORT_OUTSIDE,
    MODULE_CMP,
    MODULE_CYCLE_A,
    MODULE_CYCLE_B,
    MODULE_DERIVE_IMPORT_MAIN,
    MODULE_IMPORT_DUP,
    MODULE_IMPORT_PRIVATE,
    MODULE_MAIN,
    MODULE_MAIN_BAD_IMPORT,
    MODULE_MAIN_GLOB,
    MODULE_MAIN_LIST,
];

pub fn module_tree_by_name(name: &str) -> Option<&'static ModuleTree> {
    MODULE_TREES.iter().find(|t| t.name == name)
}
