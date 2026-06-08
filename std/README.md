# Phoenix standard library (`std`)

The repo-root `std/` package is a **`type = lib`** Phoenix project. Core language-foundation types live under **`std::core::*`**; the package root (`std`) stays thin.

## Build locally

From the repository root:

```bash
phx build --project-root std
# or
just build-std
```

**Output:** `std/build/lib/std.phx0` (linkable library image).

Other artifacts under `std/build/`:

| Path | Role |
|------|------|
| `build/pxi/std.pxi` | Root module interface (`version`) |
| `build/pxi/std/core/option.pxi` | `Option` / `Some` / `None` exports |
| `build/pxi/std/core/result.pxi` | `Result` / `Ok` / `Err` exports |
| `build/phx0/**` | Per-module object bytecode |
| `build/manifest.json` | Incremental rebuild metadata |

Library packages are not runnable with `phx run`.

## Bundled by default (V0-041)

Every Phoenix project **links the repo `std` package by default** unless opted out:

```toml
[project]
bundle_std = false   # omit bundled std; add explicit [dependencies] if needed
```

Discovery order: `PHOENIX_STD` env → walk parents of the compiler binary / cwd for `std/phoenix.toml` with `project.name = "std"`.

Explicit path dependencies override bundling when present:

```toml
[dependencies]
std = { path = "../std" }   # key MUST equal std's project.name
```

Consumer builds place std artifacts under `build/deps/std/`.

## Imports (V0-041)

```phoenix
#import std::version;
#import std::core::option::Option;
#import std::core::option::Some;
#import std::core::option::None;
#import std::core::result::Result;
#import std::core::result::Ok;
#import std::core::copyable::Copyable;
#import std::core::clone::Clone;
#import std::core::cmp::PartialEq;
#import std::core::fmt::Debug;
#import std::core::fmt::Display;
#import std::error::error::Error;
#import std::error::io::IoError;
```

With **`prelude = true`** (default when std is bundled), the items above plus `Option` / `Result` and their ctors are in scope without explicit `#import`. See [Prelude](#prelude-v0-044). Error types are **not** in the prelude — import explicitly (see [`std_errors/`](../tests/cli/fixtures/std_errors/)).

Generic enum constructors need explicit type arguments today, e.g. `Some :: <s32> (n)`.

See [`tests/cli/fixtures/std_smoke/`](../tests/cli/fixtures/std_smoke/) for a bundled-std bin consumer; [`std_traits/`](../tests/cli/fixtures/std_traits/) and [`std_prelude/`](../tests/cli/fixtures/std_prelude/) for trait bounds and prelude smoke tests; [`std_errors/`](../tests/cli/fixtures/std_errors/) for layered `?` across std error types (V0-060).

## Module layout (`std::core`)

| Logical module | File | Status |
|----------------|------|--------|
| `std` | `src/lib.phx` | `version` only |
| `std::core` | `src/core/mod.phx` | Namespace anchor (no re-exports yet) |
| `std::core::option` | `src/core/option.phx` | `pub Option :: <t> enum` |
| `std::core::result` | `src/core/result.phx` | `pub Result :: <ok, err> enum` |
| `std::core::copyable` | `src/core/copyable.phx` | `pub Copyable :: trait` (empty marker) |
| `std::core::clone` | `src/core/clone.phx` | `pub Clone :: trait` |
| `std::core::cmp` | `src/core/cmp.phx` | `pub PartialEq`, `pub Eq :: trait` |
| `std::core::fmt` | `src/core/fmt.phx` | `pub Debug`, `pub Display :: trait` (fixed `[u8; 32]` buffer) |
| `std::core::convert` | `src/core/convert.phx` | `pub From`, `Into`, `TryFrom`, `TryInto` |
| `std::prelude` | `src/prelude.phx` | Compiler-injected re-exports when `prelude = true` |
| `std::error` | `src/error/mod.phx` | Namespace anchor |
| `std::error::kind` | `src/error/kind.phx` | `pub ErrorKind :: enum` |
| `std::error::io` | `src/error/io.phx` | `pub IoError :: struct` |
| `std::error::parse` | `src/error/parse.phx` | `pub ParseError :: struct` |
| `std::error::thread` | `src/error/thread.phx` | `pub ThreadError :: struct` |
| `std::error::general` | `src/error/general.phx` | `pub GeneralError :: struct` |
| `std::error::error` | `src/error/error.phx` | `pub Error :: enum` (Pattern A sum) |
| `std::error::from_*` | `src/error/from_*.phx` | `From<Leaf> for Error` (one leaf per file) |

Future top-level siblings (post-core): `std::collections::*`, `std::text::*`.

## Prelude (V0-044)

When `bundle_std = true` (default), projects also get **`prelude = true`** by default. The compiler injects bindings from `std::prelude` into workspace modules (not into std internals):

- `Option`, `Some`, `None`, `Result`, `Ok`, `Err`
- `Copyable`, `Clone`, `PartialEq`, `Eq`, `Debug`

Opt out per project:

```toml
[project]
prelude = false   # require explicit #import (see tests/cli/fixtures/std_prelude_off/)
```

Single-file / in-process `compile_source(..., None)` does **not** inject prelude.

## Non-goals (current)

- Per-file `#no_prelude`, glob prelude, or entire std surface in prelude
- Rich formatting (`Debug` / `Display` trait defs only; error-type trait impls deferred until `[u8; N]` return lowering is stable)
- No std I/O — requires scheduler (post Language v0)

## Test fixtures vs real std

[`tests/cli/fixtures/math_lib/`](../tests/cli/fixtures/math_lib/) is a **test double** for path-dependency integration tests. This `std/` directory is the **real** standard library package.
