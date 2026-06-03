# Modules and imports

Phoenix has no `mod { ... }` or `use` blocks. **Files are modules.** Folders are packages (directories of modules). Imports use the `#import` compile-time directive.

---

## Milestones

| Phase | Behavior |
|-------|----------|
| **M1** | Whole-program compile: load all reachable `.phx` files, `#import`, `pub`, cycle rejection, one PHX0 output |
| **M2** | `phoenix.toml` project root, `build/` artifacts, `.pxi` interfaces, incremental rebuild, PHX0 linker, path dependencies, `phx build` / `phx run` |

**Deferred (post-M2):** relative `./` / `../` imports, `import { x as y }` aliases, qualified paths without `#import`, registry / URL dependencies.

---

## Artifacts and formats

| Artifact | Extension | Format | Role |
|----------|-----------|--------|------|
| Source | `.phx` | Phoenix source | Authoring; one file = one module body |
| Interface | `.pxi` | JSON v1 (**PXI** = Phoenix Interface) | `pub` exports, signatures, `source_hash`, import graph for incremental / separate compile |
| Object bytecode | `.phx0` | Binary **PHX0** ([vm-linear.md](vm-linear.md)) | Per-module object; verifier + linker input |
| Linked image | `.phx0` in `build/bin/` or `build/lib/` | PHX0 | Runnable (`bin`) or linkable library image (`lib`) |
| Manifest | `manifest.json` | JSON | Stale detection, artifact paths |

### Two different “path” names

| Concept | Where | Example |
|---------|-------|---------|
| Filesystem source root | `project.module_src` in `phoenix.toml` | `src/` |
| Logical module identity | `logical_module` in `.pxi` JSON | `math::utils` |

There is no `module_path` field in TOML or PXI (use `module_src` vs `logical_module`).

---

## Package model

- **`project.name`** — package name and **first segment** of every logical module path in that package.
- **`project.type`** — `bin` or `lib` (required).
- **`module_src`** — directory of `.phx` sources (relative to project root).

### Special source files (only these three)

| File | `type = bin` | `type = lib` | Logical module |
|------|--------------|--------------|----------------|
| `main.phx` at `module_src` root | **Required** | — | `{name}` (not `{name}::main`) |
| `lib.phx` at `module_src` root | — | **Required** | `{name}` (not `{name}::lib`) |
| `dir/mod.phx` | Optional | Optional | `{name}::{dir…}` (parent dirs only; no `::mod` suffix) |

All other `.phx` files map by relative path + stem under `module_src`, prefixed with `{name}::`. For example, with package `math`:

| File | Logical module |
|------|----------------|
| `common.phx` | `math::common` |
| `utils/mod.phx` | `math::utils` |
| `utils/index.phx` | `math::utils::index` (**not** aliased to `math::utils`) |

`index.phx` has no special meaning.

### `#import` resolution

- **Same package** — omit the package prefix (e.g. `#import utils::add` inside package `myapp` resolves to `myapp::utils`).
- **Full path** — `#import myapp::utils::add` is allowed when the first segment is this package’s `name`.
- **Dependency** — first segment is the dependency’s `project.name` (e.g. `#import math::utils::add`); sources live under that package’s `module_src`, artifacts under `build/deps/<name>/`.

Path resolution for logical module `math::utils` in package `math`:

1. `{module_src}/utils/mod.phx`
2. Else `{module_src}/utils.phx` (if present)

Path resolution for package root `math`:

1. `{module_src}/lib.phx` when `type = lib`
2. `{module_src}/main.phx` when `type = bin`

- Items at file scope are **private** unless marked `pub`.
- `pub` marks an item exportable to other modules via `#import`.
- Paths use `::` as separator.

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

- **`type = bin`** — requires `main :: () => { … }` in the package root module (`{name}` from `main.phx`).
- **`type = lib`** — `main` is **forbidden** in any module in the package.
- Other modules may omit `main`.
- **M1:** circular `#import` graphs are rejected with a cycle trace.
- **M2:** cycles may compile when every module in the SCC has a **fresh** `.pxi` (interface-only for importers).

---

## Project configuration (`phoenix.toml`)

The project root is the directory containing `phoenix.toml`. `phx build` and `phx run` require this file.

```toml
[project]
name = "myapp"
version = "0.1.0"
description = "My application"
type = "bin"
module_src = "src"

[dependencies]
math = { path = "../math" }

[build]
dir = "build"
```

| Field | Default | Meaning |
|-------|---------|---------|
| `project.name` | required | Package id, namespace root, output stem |
| `project.version` | `"0.0.0"` | Package metadata |
| `project.description` | `""` | Package metadata |
| `project.type` | required | `bin` or `lib` |
| `project.module_src` | `"src"` | Source root (relative to project root) |
| `dependencies.<key>.path` | — | Filesystem path to dependency root (must contain `phoenix.toml`) |
| `build.dir` | `"build"` | Artifact root |

**Validation:**

- `module_src` exists under the project root.
- `type = bin` → `{module_src}/main.phx` exists.
- `type = lib` → `{module_src}/lib.phx` exists.
- Each dependency: `type = lib` only; `[dependencies]` key **must** equal depended `project.name`.
- Default compile entry: `{module_src}/main.phx` (`bin`) or `{module_src}/lib.phx` (`lib`).

**Outputs:**

- `type = bin` → linked `build/bin/{project.name}.phx0` for `phx run`.
- `type = lib` → linked `build/lib/{project.name}.phx0` (not executed by `phx run`).

`module_src` comes from `phoenix.toml` or `--module-src` on the CLI.

---

## Build directory layout (M2)

All compiler outputs live under `{build.dir}` (default `build/`):

| Path | Content |
|------|---------|
| `build/manifest.json` | Module graph, hashes, dependency records |
| `build/pxi/<path>.pxi` | Workspace interfaces (`a::b::c` → `build/pxi/a/b/c.pxi`) |
| `build/phx0/<path>.phx0` | Workspace per-module object bytecode |
| `build/bin/{project.name}.phx0` | Linked executable (`type = bin`) |
| `build/lib/{project.name}.phx0` | Linked library image (`type = lib`) |
| `build/deps/{dep_name}/pxi/`, `.../phx0/`, `.../lib/` | Prebuilt dependency artifacts |

Logical module `myapp::util::math` maps to `build/pxi/myapp/util/math.pxi` and `build/phx0/myapp/util/math.phx0`.

---

## `.pxi` interface format (v1)

`format_version` is **`1`** until post-MVP stabilization.

```json
{
  "format_version": 1,
  "logical_module": "math::utils",
  "source_hash": "<hex digest of .phx bytes>",
  "origin": null,
  "exports": [
    { "name": "add", "kind": "fn", "signature": "(s32, s32) => s32" }
  ],
  "dependencies": [
    { "logical_module": "math::common", "pxi_hash": "<hex digest of .pxi file>" }
  ]
}
```

- **`logical_module`** — full logical path including package name.
- **`source_hash`** — digest of source bytes; stale when source changes.
- **`exports`** — `pub` items only; `signature` is a stable type string for cross-module checking.
- **`dependencies`** — direct imports for incremental invalidation.
- **`origin`** — optional; reserved for future registry packages.

Legacy field `module_path` is not accepted.

---

## Linker contract (M2)

**Input:** workspace `build/phx0/*.phx0`, required `build/deps/*/phx0` objects, export tables from `.pxi`, entry package root module.

**Output:** one [`BytecodeModule`](vm-linear.md) at `build/bin/{name}.phx0` or `build/lib/{name}.phx0` with:

- Remapped `function_id`, type ids, and constant indices across modules
- `entry_function_id` = linked id of `main` in the root module (`bin` only)
- Cross-module calls resolved via `pub` exports in `.pxi`

**Errors:** `InterfaceMismatch`, duplicate global symbol, missing `main` (`bin`), `main` in `lib` package.

---

## Incremental builds

`build/manifest.json` records per-module `source_hash`, `pxi_hash`, paths, and dependency build metadata.

Rebuild module **M** when:

- `hash(M.source) != pxi(M).source_hash`, or
- any direct dependency’s `pxi_hash` changed.

Transitive importers are rebuilt in reverse dependency order.

---

## CLI (M2)

| Command | Behavior |
|---------|----------|
| `phx build [entry.phx]` | Requires `phoenix.toml`; writes `build/` artifacts and manifest |
| `phx run [entry.phx]` | `type = bin` only; loads `build/bin/{project.name}.phx0` |
| `phx check` | M1 path (no `build/` required) |

Flags: `--project-root`, `--module-src`, `--no-build`, `--build`, `--emit-interface-only`.

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
