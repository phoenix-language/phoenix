# Modules and imports

Phoenix has no `mod { ... }` or `use` blocks. **Files are modules.** Folders are packages (directories of modules). Imports use the `#import` compile-time directive.

---

## Milestones

| Phase | Behavior |
|-------|----------|
| **M1** | Whole-program compile: load all reachable `.phx` files, `#import`, `pub`, cycle rejection, one PHX0 output |
| **M2** | `phoenix.toml` project root, `build/` artifacts, `.pxi` interfaces, incremental rebuild, PHX0 linker, `phx build` / `phx run` |

**Deferred (post-M2):** relative `./` / `../` imports, `import { x as y }` aliases, qualified paths without `#import`.

---

## File paths and module names

The compiler maps each source file to a logical module path from the project layout:

- `math/common.phx` → module `math::common`
- `math/common/index.phx` → module `math::common` (fallback when `math/common.phx` is absent)

Path resolution for `#import math::common`:

1. `{module_root}/math/common.phx`
2. Else `{module_root}/math/common/index.phx`

`module_root` comes from `phoenix.toml` `[project] module_path` or `--module-path`.

- Items at file scope are **private** unless marked `pub`
- `pub` marks an item exportable to other modules via `#import`
- Paths use `::` as separator: `std::collections::HashMap`

---

## Import forms

| Form | Effect |
|------|--------|
| `#import path::Item;` | `Item` in scope (last segment is the symbol) |
| `#import path::{A, B, C};` | Multiple `pub` items from `path` |
| `#import path::*;` | All `pub` items from module `path` |

Importing a non-`pub` item is a compile error. Duplicate names from globs or multiple imports are reported.

---

## Entry point

- Executable builds require `main :: () => { … }` in the **entry** module.
- Other modules may omit `main`.
- **M1:** circular `#import` graphs are rejected with a cycle trace.
- **M2:** cycles may compile when every module in the SCC has a **fresh** `.pxi` (interface-only for importers).

---

## Project configuration (`phoenix.toml`)

The project root is the directory containing `phoenix.toml`. `phx build` and `phx run` require this file.

```toml
[project]
name = "myapp"
module_path = "src"

[build]
dir = "build"
entry = "app/main"
```

| Field | Default | Meaning |
|-------|---------|---------|
| `project.name` | required | Project name (diagnostics) |
| `project.module_path` | `"."` | Root for source modules and `#import` |
| `build.dir` | `"build"` | All compiler outputs |
| `build.entry` | CLI entry file | Logical module path for `main` (e.g. `app/main`) |

---

## Build directory layout (M2)

All compiler outputs live under `{build.dir}` (default `build/`):

| Path | Content |
|------|---------|
| `build/manifest.json` | Module graph, source/pxi hashes, artifact paths |
| `build/pxi/<path>.pxi` | Interface for logical module `a::b::c` → `build/pxi/a/b/c.pxi` |
| `build/phx0/<path>.phx0` | Per-module object bytecode |
| `build/bin/<name>.phx0` | Linked executable for `phx run` |

Logical path `util::math` maps to `build/pxi/util/math.pxi` and `build/phx0/util/math.phx0`.

---

## `.pxi` interface format (v1)

JSON file, `format_version: 1`:

```json
{
  "format_version": 1,
  "module_path": "util::math",
  "source_hash": "<hex digest of .phx bytes>",
  "origin": null,
  "exports": [
    { "name": "add", "kind": "fn", "signature": "(s32, s32) => s32" }
  ],
  "dependencies": [
    { "module_path": "other", "pxi_hash": "<hex digest of .pxi file>" }
  ]
}
```

- **`source_hash`** — digest of source bytes; stale when source changes.
- **`exports`** — `pub` items only; `signature` is a stable type string for cross-module checking.
- **`dependencies`** — direct imports for incremental invalidation.
- **`origin`** — optional; reserved for future packages.

Importers may type-check against `.pxi` when compiling separately and dependency `.pxi` is fresh.

---

## Linker contract (M2)

**Input:** ordered per-module `build/phx0/*.phx0`, export tables from `.pxi`, entry module logical path.

**Output:** one [`BytecodeModule`](vm-linear.md) at `build/bin/<name>.phx0` with:

- Remapped `function_id`, type ids, and constant indices across modules
- `entry_function_id` = linked id of entry module `main`
- Cross-module calls resolved via `pub` exports in `.pxi`

**Errors:** `InterfaceMismatch`, duplicate global symbol, missing `main` in entry module.

---

## Incremental builds

`build/manifest.json` records per-module `source_hash`, `pxi_hash`, and paths.

Rebuild module **M** when:

- `hash(M.source) != pxi(M).source_hash`, or
- any direct dependency’s `pxi_hash` changed.

Transitive importers are rebuilt in reverse dependency order.

---

## CLI (M2)

| Command | Behavior |
|---------|----------|
| `phx build [entry.phx]` | Requires `phoenix.toml`; writes `build/` artifacts and manifest |
| `phx run [entry.phx]` | Loads `build/bin/*.phx0`; rebuilds when stale unless `--no-build` |
| `phx check` | Unchanged M1 path (no `build/` required) |

Flags: `--project-root`, `--module-path`, `--no-build`, `--build`, `--emit-interface-only`.

---

## Top-level side effects

Top-level executable side effects are disallowed. Use `init :: () => { … }` or call from `main`. Module-level `const` / `var` runtime is not part of MVP modules.

---

## Related documents

| Topic | Document |
|-------|----------|
| Formal grammar | [grammar.ebnf](../grammar.ebnf) |
| MVP boundary | [mvp.md](../mvp.md) |
| Bytecode / VM | [vm-linear.md](vm-linear.md) |
