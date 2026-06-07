# Phoenix standard library (`std`)

The repo-root `std/` package is a **`type = lib`** Phoenix project. V0-040 establishes the package layout; V0-041+ adds `Option`, `Result`, traits, and submodule content in Phoenix source.

## Build locally

From the repository root:

```bash
phx build --project-root std
# or
just phx build --project-root std
```

**Output:** `std/build/lib/std.phx0` (linkable library image).

Other artifacts under `std/build/`:

| Path | Role |
|------|------|
| `build/pxi/std.pxi` | Public export interface for cross-crate type-checking |
| `build/phx0/std.phx0` | Per-module object bytecode |
| `build/manifest.json` | Incremental rebuild metadata |

Library packages are not runnable with `phx run`.

## Consume via path dependency

In your app's `phoenix.toml`, declare std as a path dependency. The dependency **key must equal** std's `project.name` (`std`):

```toml
[dependencies]
std = { path = "../std" }
```

In Phoenix source:

```phoenix
#import std::version;

main :: () => {
    const v = version();
};
```

The consumer build places prebuilt std artifacts under `build/deps/std/` (`.pxi`, `.phx0`, linked `lib/std.phx0`).

See [`tests/cli/fixtures/std_smoke/`](../tests/cli/fixtures/std_smoke/) for a minimal bin consumer in this repo.

## Module layout (planned)

| Logical module | File | Status |
|----------------|------|--------|
| `std` | `src/lib.phx` | V0-040 placeholder (`version`) |
| `std::core` | `src/core/mod.phx` | Layout stub (V0-041+ content) |

Future std modules (V0-041+, per `docs/design/language-v0.md`):

- `core::alloc` — allocation wrappers over VM intrinsics
- `core::option` / `core::result` — generic enums in std source
- `core::clone`, `core::copyable`, `core::cmp`, `core::fmt` — traits
- `collections::vec`, `text::string`, `text::fmt` — collections and text

## Non-goals (V0-040)

- No implicit prelude — apps must `#import` or declare a prelude (V0-044)
- No compiler builtins for `Option` / `Result` — rejected until V0-041
- No std I/O — requires scheduler (post Language v0)

## Test fixtures vs real std

[`tests/cli/fixtures/math_lib/`](../tests/cli/fixtures/math_lib/) is a **test double** for path-dependency integration tests. This `std/` directory is the **real** standard library package for the Phoenix project.
