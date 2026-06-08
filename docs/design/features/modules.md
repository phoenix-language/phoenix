# Modules and imports

Phoenix has no `mod { ... }` or `use` blocks. **Files are modules.** Folders are packages (directories of modules). Imports use the `#import` compile-time directive.

---

## Milestones

| Phase | Behavior |
|-------|----------|
| **M1** | Whole-program compile: load all reachable `.phx` files, `#import`, `pub`, cycle rejection, one PHX0 output |
| **M1 (MVP modules)** | **Block-scoped `#import`** — same forms as file scope; names visible only inside the enclosing block (see [Scoped imports](#scoped-imports-mvp)) |
| **M2** | `phoenix.toml` project root, `build/` artifacts, `.pxi` interfaces, incremental rebuild, PHX0 linker, path dependencies, `phx build` / `phx run` |

**Deferred (post-M2):** relative `./` / `../` imports, `import { x as y }` aliases, [module namespace import values](#module-namespace-import-values-post-mvp), [qualified paths without `#import`](#qualified-paths-without-import), registry / URL dependencies.

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
| `dir/mod.phx` | **Required** when `dir/` contains other `.phx` files | **Required** when `dir/` contains other `.phx` files | `{name}::{dir…}` (parent dirs only; no `::mod` suffix) |

**V0-061 (Rust-style module entries):** applies uniformly to workspace `bin`, workspace `lib`, path dependencies, and bundled `std`. The `module_src` root never uses `mod.phx` — only `main.phx` (`bin`) or `lib.phx` (`lib`).

| Rule | Behavior |
|------|----------|
| Submodule entry | Nested module `pkg::seg` resolves to `seg.phx` **or** `seg/mod.phx` under the parent directory |
| Flat vs dir | `seg.phx` and `seg/mod.phx` are **mutually exclusive** for the same logical module |
| Child files | A `.phx` under a subdirectory is **not** loaded unless the parent entry declares `mod name;` |
| `pub mod name` | Child is importable across package boundaries as `pkg::parent::name::…` |
| Private child | `mod name` without `pub` — visible only inside the same package's module tree |
| Discovery | Tree walk from package root (`main.phx` / `lib.phx`), following `mod` declarations; cross-package edges still follow `#import` |
| Orphan files | Undeclared `.phx` files under a subdirectory (not at `module_src` root) are compile errors |

### `mod` and barrel syntax

Rust-style child module registration (distinct from `dir/mod.phx` entry files):

```phoenix
mod helpers;                // private child (loads helpers.phx)
pub mod math;               // public child — importers may use `pkg::parent::math::…`

pub reexport :: Widget;            // re-export pub item defined in this file
pub reexport :: math::add;         // re-export from registered child module `math`
```

- `pub` on `mod` marks the whole child module public to other packages; private `mod` stays in-package only.
- `pub reexport` without `pub` is a resolve error (`ReexportRequiresPub`).
- External `#import pkg::dir::Item` resolves only through the **parent export map** (`pub` defs + `pub reexport`).
- `#import pkg::dir::child::Item` requires `child` to be a **`pub mod`** when crossing package boundaries.

Logical module mapping examples (package `math`):

| File | Logical module |
|------|----------------|
| `utils/mod.phx` | `math::utils` |
| `utils/math.phx` (with `pub mod math` in `utils/mod.phx`) | `math::utils::math` |
| `utils.phx` | `math::utils` (**mutually exclusive** with `utils/mod.phx`) |

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

### Standard library package (`std/`)

The repository ships a first-class std lib package at **`std/`** (repo root):

- `type = lib`, `project.name = "std"`, entry `std/src/lib.phx` → logical module `std`
- `phx build --project-root std` → `std/build/lib/std.phx0`
- Submodule files follow the same mapping (e.g. `std/src/core/mod.phx` → `std::core`)

**Path dependency (local development):**

```toml
[dependencies]
std = { path = "../std" }   # key MUST equal std's project.name
```

```phoenix
#import std::version;
#import std::core::option::Option;
#import std::core::result::Result;
```

**Bundled std (V0-041):** `bundle_std = true` by default in `phoenix.toml` injects the repo `std` path dependency when `[dependencies] std` is absent. Set `bundle_std = false` for minimal projects or tests that must not link std. Override discovery with `PHOENIX_STD`.

**Prelude (V0-044):** When std is bundled, `prelude = true` by default injects `std::core::*` exports (`Option`, `Result`, core traits) into workspace module scope via the compiler (`modules/prelude.rs`). Set `prelude = false` to require explicit `#import`. Prelude is independent of bundling but only applies when std is linked. Smoke: `tests/cli/fixtures/std_prelude/` (on), `std_prelude_off/` (off).

**`std::core` convention:** language-foundation types and traits live under `std::core::*` (e.g. `std::core::option::Option`), not at the `std` package root. The root module (`std`) stays thin (`version` only for now).

Contributor workflow: [`std/README.md`](../../../std/README.md). Smoke consumer: `tests/cli/fixtures/std_smoke/`.

---

## Import forms

| Form | Effect |
|------|--------|
| `#import path::Item;` | `Item` in scope (last segment is the symbol) |
| `#import path::{A, B, C};` | Multiple `pub` items from `path` |
| `#import path::{*};` | All `pub` items from module `path` (glob in braces; see [grammar.ebnf](../grammar.ebnf)) |

Importing a non-`pub` item is a compile error. Duplicate names from globs or multiple imports are reported.

---

## Scoped imports (MVP)

**Goal:** Local name binding without polluting the whole file — e.g. import a helper only inside one function.

`#import` is a **compile-time** directive (`#`). It may appear at **file scope** (today) or inside any **`{ … }` block** (MVP modules milestone). Block placement does **not** make imports dynamic: there is no runtime `import()` and no module loader at execution time.

### Syntax

Same forms as [Import forms](#import-forms):

```phoenix
main :: () => {
  #import utils::math::add;
  const sum = add(1, 2);

  {
    #import utils::math::{add, mul};
    const _ = add(1, mul(2, 3));
  };
};
```

Glob in a block uses brace form per grammar: `#import utils::math::{*};`.

### Semantics

| Rule | Behavior |
|------|----------|
| **Visibility** | Names introduced by a block `#import` are in scope only in that block and nested blocks (shadowing follows normal block rules). |
| **Module graph** | Every `#import` (file or block) adds edges for **whole-program module loading** — a block import can pull in a module even when there is no file-top import of that path. |
| **Exports** | Same as file scope: only `pub` items bind; private imports are errors. |
| **Duplicates** | Same rules as file scope (`DuplicateImport` when the same symbol is imported twice into one scope). |
| **Resolution time** | Fully resolved at compile time; lowers to static `Call` / existing cross-module `DefId` binding — no runtime module handle. |

### Not in scoped-import MVP

- **`const math = #import utils::math;`** — module as a first-class namespace **value** ([Module namespace import values](#module-namespace-import-values-post-mvp)).
- **`math.add` as a function pointer value** — requires [function pointers (Layer 2)](type-system.md#layer-2--function-pointers-post-mvp--ffi-phase-design-now) and `IndirectCall` ([V0-053](../language-v0.md) in the roadmap).
- **Qualified paths without `#import`** — e.g. `utils::math::add(1, 2)` with no import line ([Qualified paths without `#import`](#qualified-paths-without-import)).

---

## Import evolution (phased)

Three tiers; implement in order. Do not skip design-doc updates before coding.

### Tier 1 — Block-scoped `#import` (MVP modules)

Scoped **name introduction** only — see [Scoped imports (MVP)](#scoped-imports-mvp). Delivers most “I don’t want top-of-file imports” ergonomics with no new types and no fn-pointer values.

**Roadmap:** [V0-014](../language-v0.md) in [language-v0.md](../language-v0.md).

### Tier 2 — Module namespace import values (post-MVP)

Bind a module path as a compile-time namespace handle, then select members:

```phoenix
const math = #import utils::math;   // compile-time namespace; not a runtime heap object
const add = math.add;               // fn pointer value (Layer 2) once V0-053 ships
const sum = add(1, 2);
```

| Concern | Design direction |
|---------|------------------|
| Type of `math` | Compile-time **module ref** / export table — rodata or static indices, not GC |
| `math.add` | Function pointer when target is `pub fn`; static `Call` when callee is fully known (devirtualize) |
| `math.Point` | **Type** namespace — distinct from value members; syntax TBD |
| Generics | `math.sort :: <s32>(…)` needs type args on the member access path |
| Cross-crate | Importers use `.pxi` export lists, not source parse order |

Depends on **V0-053** (function pointers + `IndirectCall`). Syntax may stay `#import` on the RHS of `const` or use a dedicated form (e.g. `module utils::math`) — lock in this doc before implementation.

### Tier 3 — Qualified paths without `#import`

Call or refer with a full path and no import line:

```phoenix
main :: () => {
  const sum = utils::math::add(1, 2);   // static Call; no fn pointer required
};
```

Lower friction for one-off use; still compile-time and static-by-default. Can ship independently of Tier 2.

---

## Entry point

- **`type = bin`** — requires `main :: () => { … }` in the package root module (`{name}` from `main.phx`).
- **`type = lib`** — `main` is **forbidden** in any module in the package.
- Other modules may omit `main`.
- **M1:** circular `#import` graphs are rejected with a cycle trace.
- **M2:** cycles may compile when every module in the SCC has a **fresh** `.pxi` (interface-only for importers). Compile order within the SCC is undefined; importers use `.pxi` export lists, not source parse order.

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
| `project.edition` | `""` | Reserved (parsed, not enforced in MVP) |
| `project.module_roots` | — | Reserved list (parsed; MVP uses `module_src` only) |
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

## `.pxi` interface format (v1 and v2)

MVP emit uses **`format_version` `2`**. Readers accept v1 and v2.

**v1** — `signature` string per export (stable for human diff and manifest checks).

**v2** — adds structured **`ty`** JSON per export for cross-module type-checking (`import_types`); keeps `signature` as a fallback string.

```json
{
  "format_version": 2,
  "logical_module": "math::utils",
  "source_hash": "<hex digest of .phx bytes>",
  "origin": null,
  "exports": [
    {
      "export_id": "math::utils::add::fn",
      "name": "add",
      "kind": "fn",
      "signature": "(s32, s32) => s32",
      "ty": { "fn": { "params": ["s32", "s32"], "ret": "s32" } }
    }
  ],
  "dependencies": [
    { "logical_module": "math::common", "pxi_hash": "<hex digest of .pxi file>" }
  ]
}
```

- **`logical_module`** — full logical path including package name.
- **`source_hash`** — digest of source bytes; stale when source changes.
- **`exports`** — `pub` items only; `signature` is a stable type string; optional **`ty`** for structured types (v2).
- **`dependencies`** — direct imports for incremental invalidation.
- **`origin`** — optional; reserved for future registry packages.

Path-dependency `.pxi` files live under `build/deps/{dep_name}/pxi/` (not the workspace `build/pxi/` tree).

Legacy field `module_path` is not accepted.

---

## Linker contract (M2)

**Input:** workspace `build/phx0/*.phx0`, required `build/deps/*/phx0` objects, export tables from `.pxi`, entry package root module.

**Output:** one [`BytecodeModule`](vm-linear.md) at `build/bin/{name}.phx0` or `build/lib/{name}.phx0` with:

- **Globally unique `function_id`** assigned at per-module codegen (linker does not rewrite `Call` operands)
- Remapped **constant** and **type** indices in merged code
- `entry_function_id` = linked id of `main` in the root module (`bin` only)
- Cross-module calls use pre-assigned ids; export names/types come from `.pxi`

**Errors:** `InterfaceMismatch`, duplicate global symbol, missing `main` (`bin`), `main` in `lib` package.

---

## Incremental builds

`build/manifest.json` records per-module `source_hash`, `pxi_hash`, paths, and dependency build metadata.

Rebuild module **M** when:

- `hash(M.source) != pxi(M).source_hash`, or
- any direct dependency’s `pxi_hash` changed.

Transitive importers are rebuilt in reverse dependency order.

**MVP contract:** incrementalism applies to **artifact emission** (`.pxi`, per-module `.phx0`, linked output). When any module is stale, the driver still runs whole-program resolve and type-check before emitting artifacts. Path dependencies under `build/deps/{name}/` are rebuilt when their manifest or source hashes are stale (not merely when `lib/{name}.phx0` exists).

---

## CLI (M2)

| Command | Behavior |
|---------|----------|
| `phx build [entry.phx]` | Requires `phoenix.toml`; writes `build/` artifacts and manifest |
| `phx run [entry.phx]` | Requires `phoenix.toml` and `type = bin`; loads `build/bin/{project.name}.phx0`. Library packages (`type = lib`) produce `build/lib/{name}.phx0` for linking only — not executed by `phx run`. |
| `phx check [file.phx]` | Type-check only. When `phoenix.toml` is found (walk parents from the file), uses `module_src` and path deps like `phx build`. Otherwise uses the file’s parent or `--module-src`. No `build/` required. |

| Flag | Commands | Behavior |
|------|----------|----------|
| `--project-root <dir>` | `build`, `run` | Project root containing `phoenix.toml` |
| `--module-src <dir>` | `check`, `run` (standalone) | Module root for `#import` |
| `--build` | `build`, `run` | Force full rebuild |
| `--no-build` | `run` | Skip build; load existing `build/bin` artifact |
| `--emit-interface-only` | `build`, `check` | After successful type-check, write `build/pxi/*.pxi` and `manifest.json` only; skip per-module `.phx0` codegen and link. `phx run` rejects this flag. |

When fresh `.pxi` files exist under `build/` or `build/deps/`, importers seed cross-module types from v2 structured `type` objects instead of re-parsing dependency bodies.

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
