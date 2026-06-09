# `.pxi` interface format

Phoenix modules emit a JSON **`.pxi`** file alongside `.phx0` for incremental builds and separate compilation. The compiler reads dependency `.pxi` files when their `source_hash` matches the dependency source (or when only the interface is available in a prebuilt package).

## Versions

| `format_version` | Status |
|----------------|--------|
| `1` | Signature strings only (`signature` per export). |
| `2` | Adds optional structured `type` per export for type-checking without parsing dependency bodies. |

Readers must accept v1 and v2. Writers emit v2 from the current compiler.

## File shape (v1 and v2)

```json
{
  "format_version": 2,
  "logical_module": "pkg::util",
  "source_hash": "<sha256 of .phx source>",
  "origin": null,
  "exports": [ { ... } ],
  "dependencies": [ { "logical_module": "...", "pxi_hash": "..." } ]
}
```

- **`logical_module`**: stable module path (`modules.md`).
- **`source_hash`**: digest of the `.phx` file at emit time; stale interfaces trigger rebuild.
- **`export_id`**: `logical_module::name::kind` (stable across link maps).
- **`function_id`** (v2, `fn` exports only): global PHX0 `function_id` assigned at per-module codegen; consumers use this for cross-package `Call` operands when linking prebuilt dependency objects.
- **`signature`**: human-readable type string (unchanged from v1; used for manifest diff).
- **`type`** (v2 only): structured type tree (below).

## Structured `type` (v2)

Each export may include a `"type"` object. Kinds:

| `kind` | Fields | Use |
|--------|--------|-----|
| `primitive` | `name` (`s32`, `bool`, …) | Primitives |
| `unit` | — | `()` |
| `named` | `path` (string), `args` (array) | User types; `path` is `logical_module::TypeName` for cross-module refs |
| `tuple` | `elems` | Tuple types |
| `array` | `elem`, `len` | **Array** `[T; N]` |
| `slice` | `elem` | `[T]` |
| `ref` | `mut`, `inner` | `&T` / `&mut T` |
| `ptr` | `mut`, `inner` | `*T` / `*mut T` |
| `fn` | `params`, `ret` | Function types |
| `struct` | `fields`: `[{ "name", "type" }]` | Struct exports |
| `enum` | `variants`: `[{ "name", "tag", "payload" }]` | Enum exports; `payload` is `null`, `"tuple"`, or field list |
| `alias` | `inner` | Type alias target |

Function exports use `kind: "fn"` at the export level and `type.kind: "fn"`. Struct/enum exports include full field/variant metadata so importers need not parse source.

## Consumption

When a module imports from a dependency whose `.pxi` is fresh:

1. Resolver still binds symbols to `DefId` (existing rules).
2. Type checker seeds `value_types` (and struct layout hints where applicable) from v2 `type` objects.
3. v1-only `.pxi` files continue to work; only `signature` is available for diagnostics.

## Cross-crate generics

Generic APIs in source appear in `.pxi` export lists as:

- **Templates** — unmangled export name (e.g. `id`), structured type from the generic signature, **`function_id` omitted** (not linkable).
- **Specializations** — mangled export name and `export_id` (e.g. `id$s32`, `math::id$s32::fn`), fully concrete signature/`type`, **`function_id`** for cross-crate `Call` linking.

Callers import the generic template symbol (e.g. `#import math::id`) and call with explicit `:: <T>` at the use site; the consumer build reconciles a monomorphization worklist and rebuilds path dependencies so missing mangled exports appear in `build/deps/*/pxi/` before link.

### Implemented: `.pxi` mangling for generics (V0-024)

**Status:** done — path-dependency builds emit and consume mangled fn exports; generic templates stay importable without `function_id`.

**Still out of scope:**

- Cross-crate generic inference (explicit specializations only).
- `.pxi`-only dependency loading without parsing dep source (templates still loaded from source during consumer compile).
- Generic struct/enum cross-crate export mangling beyond fn exports.

See [type-system.md](type-system.md#pxi-export-mangling-v0-024).

## Non-goals (v2)

- Trait method tables / associated types (post-MVP).
- Generic inference across crate boundaries (callers must name explicit specializations that appear as separate export entries).
- Embedding PHX0 in `.pxi` (bytecode stays in `.phx0`).
