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
#import std::core::result::Err;
```

Generic enum constructors need explicit type arguments today, e.g. `Some :: <s32> (n)`.

See [`tests/cli/fixtures/std_smoke/`](../tests/cli/fixtures/std_smoke/) for a bundled-std bin consumer.

## Module layout (`std::core`)

| Logical module | File | Status |
|----------------|------|--------|
| `std` | `src/lib.phx` | `version` only |
| `std::core` | `src/core/mod.phx` | Namespace anchor (no re-exports yet) |
| `std::core::option` | `src/core/option.phx` | `pub Option :: <t> enum` |
| `std::core::result` | `src/core/result.phx` | `pub Result :: <ok, err> enum` |

Future under `std::core` (V0-043+): `clone`, `copyable`, `cmp`, `fmt`, `alloc`.

Future top-level siblings (post-core): `std::collections::*`, `std::text::*`.

## Non-goals (current)

- No implicit prelude — apps must `#import` (V0-044)
- No `?` sugar until V0-042
- No std I/O — requires scheduler (post Language v0)

## Test fixtures vs real std

[`tests/cli/fixtures/math_lib/`](../tests/cli/fixtures/math_lib/) is a **test double** for path-dependency integration tests. This `std/` directory is the **real** standard library package.
