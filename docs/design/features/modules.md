# Modules and imports

Phoenix has no `mod { ... }` or `use` blocks. **Files are modules.** Folders are packages (directories of modules). Imports use the `#import` compile-time directive.

---

## Milestones

| Phase | Behavior |
|-------|----------|
| **M1 (current target)** | Whole-program compile: load all reachable `.phx` files, `#import`, `pub`, cycle rejection, one PHX0 output |
| **M2 (planned)** | `.pxi` interface files, content-hash incremental rebuild, separate compilation in cycles, linker artifacts |

M1 does **not** include: relative `./` / `../` imports, `import { x as y }` aliases, `.pxi` files, or per-module object linking.

---

## File paths and module names

The compiler maps each source file to a logical module path from the project layout:

- `math/common.phx` → module `math::common`
- `math/common/index.phx` → module `math::common` (fallback when `math/common.phx` is absent)

Path resolution for `#import math::common`:

1. `{module_root}/math/common.phx`
2. Else `{module_root}/math/common/index.phx`

`module_root` is set by `--module-path` (defaults to the entry file’s directory).

- Items at file scope are **private** unless marked `pub`
- `pub` marks an item exportable to other modules via `#import`
- Paths use `::` as separator: `std::collections::HashMap`

---

## Defining and consuming code

```phoenix
// std/http/request.phx

pub Request :: struct
{
  method: [u8; 4],
  path: [u8; 8],
}

handle :: (req: Request) => Response
{
  // ...
};
```

```phoenix
// app/main.phx

#import std::http::Request
#import std::http::handle

main :: () =>
{
  const req = Request { method: [71u, 69u, 84u, 0u], path: [47u, 104u, 101u, 97u, 108u, 116u, 104u, 0u] };
  handle(req);
};
```

Qualified paths (`std::http::Request`) may resolve without `#import` when unambiguous; `#import` brings names into the importer’s scope.

---

## Import forms

| Form | Effect |
|------|--------|
| `#import path::Item` | `Item` in scope (last path segment is the symbol; preceding segments are the module) |
| `#import path::{A, B, C}` | Multiple `pub` items from `path` |
| `#import path::*` | All `pub` items from module `path` |

Importing a non-`pub` item is a compile error. Duplicate names from globs or multiple imports are reported.

---

## Entry point

- Executable builds require `main :: () => { … }` in the **entry** module (the file passed to `phx check` / `phx run`).
- Other modules may omit `main`.
- Circular `#import` graphs are rejected in M1 (with a cycle trace).

---

## Top-level side effects (M1)

Top-level executable side effects are disallowed. Use `init :: () => { … }` (or call from `main`) instead. Module-level `const` / `var` runtime is not part of M1.

---

## M2: interfaces and incremental builds (planned)

From the separate-compilation design:

- **`.pxi` files** — per-module exported symbol metadata and a content hash/checksum
- **Incremental compile** — recompile when a dependency’s `.pxi` hash changes
- **Import cycles** — may compile only when every module in the cycle has a current `.pxi` (interfaces only, no body forwarding)
- **Linker** — combine object/bytecode artifacts; link only symbols listed as `pub` in `.pxi`
- **Future syntax** — `./` and `../` in import paths; `import { cos as c }`
- **Optional `origin` field** in `.pxi` for future package/version identifiers

---

## Related documents

| Topic | Document |
|-------|----------|
| Formal grammar | [grammar.ebnf](../grammar.ebnf) |
| MVP boundary | [mvp.md](../mvp.md) |
