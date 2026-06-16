# Language items (`#[lang_item]`)

Compiler-known standard library definitions (VM intrinsics, `Option`/`Result` sugar, core traits) are declared in **source** via `#[lang_item(name = "...", kind = "...")]` and mirrored in **`.pxi`** exports. The compiler resolves these markers to `DefId`s at type-check time instead of hard-coded module path tables.

---

## Attribute form

```phoenix
#[lang_item(name = "alloc_bytes", kind = "intrinsic")]
pub alloc_bytes :: (size: u32) => *mut u8 { ... };

#[lang_item(name = "Option", kind = "enum")]
pub Option :: <t> enum { ... };

#[lang_item(name = "Copyable", kind = "trait")]
Copyable :: trait { };
```

Both `name` and `kind` are required string arguments. Unknown keys are ignored with a warning in debug builds only; missing required fields are compile errors.

---

## Closed `kind` values (v1)

| `kind` | Applies to | Example `name` |
|--------|------------|----------------|
| `intrinsic` | `fn` | `alloc_bytes`, `dealloc_bytes`, `slice_from_raw_parts`, `len`, `size_of` |
| `enum` | `enum` | `Option`, `Result` |
| `variant` | enum variant (optional; see below) | `Some`, `None`, `Ok`, `Err` |
| `trait` | `trait` | `Copyable`, `Clone`, `Drop`, `PartialEq`, `Eq`, `Debug`, `Iterator`, `IntoIter`, `From` |

### Variants

Grammar does not attach item attributes to enum variants in v1. When a module defines an enum with `#[lang_item(name = "Option", kind = "enum")]`, the compiler registers variant constructors **`Some`** and **`None`** in that module by name (same for `Result` / `Ok` / `Err`). Optional `kind = "variant"` markers are reserved for a future grammar extension.

---

## Trust model

- `#[lang_item]` is honored **only** in modules whose `logical_path` starts with `std::` (bundled std and path-dependency `std`).
- User crates that attach `#[lang_item]` receive a compile error: language items are reserved for the standard library.

---

## Uniqueness and validation

- Program-wide: at most one `DefId` per `(kind, name)` pair. Duplicates report both declaration spans.
- Unknown `kind` or unknown `name` for that `kind` in `std::` is a compile error.
- Required std markers must be present when std is linked; missing markers are hard errors (no silent path-based fallback).

Markers declare **identity only** — not proc macros, not user hooks, not custom intrinsics.

---

## `.pxi` export field

Each export may include an optional `lang_item` object (v2-compatible optional field; no `format_version` bump):

```json
"lang_item": { "name": "alloc_bytes", "kind": "intrinsic" }
```

Consumers merge markers from parsed std source **and** from dependency `.pxi` when building the registry. **Source wins** on conflict; `.pxi` fills gaps when source bodies are not re-parsed.

---

## Stability

Language item `(kind, name)` pairs are **compiler ABI**. Changing a name or which definition carries a marker is a breaking change for separate compilation and the VM intrinsic contract.

---

## Related

- [compiler-directives.md](compiler-directives.md) — `#[lang_item]` under item attributes
- [pxi-format.md](pxi-format.md) — optional `lang_item` on exports
- [modules.md](modules.md) — trusted `std::` marker policy
- [type-system.md](type-system.md) — `Option` / `Result` as std enums
