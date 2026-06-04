# Review: Type System (`phx-compiler/typeck/`)

## Summary

MVP type checking matches design docs for **literal defaults, no implicit numeric widening, enum/bool/int `match` exhaustiveness, block-scoped use-after-move, explicit-args monomorphization for generic functions, and rejection of unresolved/`Option`/`Result` types**. Remaining gaps are documented MVP limits (move only through bare identifiers), display-only alias expansion, and post-MVP items (`str`, trait objects, global inference, `.pxi` mangling).

## Findings

1. **Ownership tracker ignores block shadowing** — **Addressed**
   Block-scoped `OwnershipTracker` with pop on scope exit; innermost lookup via `.rfind()`. [`ownership.md`](../design/features/ownership.md) documents MVP move-via-call limit.

2. **Move only on bare identifier RHS** — **Documented (MVP)**
   No compiler change; documented in [`ownership.md`](../design/features/ownership.md#mvp-move-detection-scope).

3. **Non-enum `match` / `given` without exhaustiveness** — **Addressed**
   `bool` requires `true`/`false` or `_`; integer primitives require `_`. Uses `NonExhaustiveMatch` (E2008).

4. **Recursive type alias cycles silent** — **Addressed**
   `TypeCheckError::RecursiveTypeAlias` (E2021) after alias collection.

5. **Unknown named type lowers to `()`** — **Addressed**
   `Ty::Error` + `UnknownType` (E2002) at use sites; user `Result`/`Option` type names still allowed when defined in source.

6. **`Ty::Var` unused — generics scaffold** — **Partially addressed (MVP mono)**
   Explicit `:: <…>` calls on generic functions; [`monomorphize`](../../../source/phx-compiler/src/typeck/mono.rs) before lowering. No global inference; generic struct literals still `UnsupportedFeature`. See [type-system.md](../design/features/type-system.md#monomorphization-mvp).

7. **Trait dispatch: static only** — **Deferred `[future]`**
   Unchanged; compatible with future `dyn Trait`.

8. **Type aliases: normalize on equality, not on display** — **Deferred `[low]`**

9. **`Option` / `Result` at typeck** — **Addressed**
   Unresolved `Option`/`Result` type names get `UnsupportedFeature`; user-defined enums named `Result` remain valid.

10. **Adding `str` / string slice** — **Deferred `[future]`**

## What's working well

- **Literal typing** matches MVP: `s32`/`u32`/`f32`/`f64`.
- **No implicit widening** in binops; explicit `as` via `check_cast`.
- **Enum exhaustiveness** with duplicate/unreachable arm detection.
- **Primitive `match`/`given` exhaustiveness** for `bool` and integer scrutinees.
- **`UseAfterMove`** with move site span; block shadowing for ownership.
- **`ExprId` + `FunctionLayout`**: deterministic binding of types to expressions for lowering.
- **Monomorphization v1**: `id :: <t> (…) => …` with `name :: <s32> (args)` call syntax.

## Recommended next actions

1. Enable generic struct/enum use sites with substitution (remove struct-literal generic `UnsupportedFeature` when ready).
2. `.pxi` mangling for specialized symbols (coordinate with [`08-modules-and-build.md`](08-modules-and-build.md)).
3. Optional alias expansion in diagnostics only (finding 8).
4. Post-MVP: path-sensitive moves, `str` design, trait objects.

**Cross-references:** Ownership erased in IR—[`05-ir-and-lowering.md`](05-ir-and-lowering.md). `TrapGivenMismatch` should no longer trigger for bool/int `given` after exhaustiveness—[`06-codegen-and-bytecode.md`](06-codegen-and-bytecode.md), [`07-vm.md`](07-vm.md).
