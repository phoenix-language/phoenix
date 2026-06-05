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
| `array` | `elem`, `len` | `[T; N]` |
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

Generic APIs in source appear in `.pxi` export lists as **monomorphized entries** only: each exported symbol has a mangled `export_id` (for example `sort$s32`) and a fully concrete signature. Parameterized signatures do not appear in v1/v2 `.pxi` today.

Implementing mangled export ids in the build is a separate follow-up coordinated with [modules and build](../finished-review/08-modules-and-build.md).

## Non-goals (v2)

- Trait method tables / associated types (post-MVP).
- Generic inference across crate boundaries (callers must name explicit specializations that appear as separate export entries).
- Embedding PHX0 in `.pxi` (bytecode stays in `.phx0`).
