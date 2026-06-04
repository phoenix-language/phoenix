# Review: Resolver (`phx-compiler/resolver/` + `modules/`)

## Summary

Name resolution for **single-file snippets and multi-file crates** is in good shape: split value/type scopes, dense session `DefId`, two-phase crate resolve with import binding, **`ResolutionKey` on `AstNodeId`**, closure capture scaffolding, MVP generic/trait checks, and **`.pxi`-filtered export surfaces** when interfaces are fresh. Remaining work is **link-stable ids across builds**, skipping dependency re-parse when only `.pxi` is needed, and full closure typing/lowering.

## Findings

1. **`compile_source` cannot resolve imports** — **Addressed (documented API)**
   `compile_source` rustdoc states `#import` requires [`compile_source_with_module_root`](../../../source/phx-compiler/src/compile.rs) or project build APIs. Single-file `resolve()` keeps `allow_imports: false` by design.

2. **`DefId` is not incremental- or link-stable** — **Partially addressed**
   Session `DefId` remains per-compile; [`stable_export_id`](../../../source/phx-compiler/src/pxi/format.rs) in `.pxi` provides stable **export** strings. Map stable id → session `DefId` at load is follow-up with [`08-modules-and-build.md`](08-modules-and-build.md).

3. **`.pxi` interface loader not wired to `resolve_crate`** — **Partially addressed**
   `exports_for_dependency` restricts imports to fresh `.pxi` export lists while bodies still parse from source. Standalone [`bindings_from_pxi`](../../../source/phx-compiler/src/modules/interface_loader.rs) remains for a future “parse bodies only” path.

4. **Synthetic spans weaken diagnostics** — **Addressed**
   `name_span_ident` / `name_span_type` use AST spans; `main` and `CircularImport` use real spans. Path expressions still resolve under one enclosing span per use (segment merge deferred).

5. **Phase 2 skipped after phase 1 errors** — **Partially addressed**
   Phase 2 skips **modules** that failed phase 1 (`phase1_skip`), not the whole crate—other modules still get body resolution for more errors per compile.

6. **Import graph cycle escape uses arbitrary order** — **Documented (MVP)**
   `topo_sort_with_pxi_escape` documents fresh-`.pxi` cyclic compile order; `CircularImport` when interfaces stale. Test: `cycle_a`/`cycle_b` expects import-site span.

7. **`ResolutionKey` is span+symbol only** — **Addressed**
   Keys are `(module, AstNodeId)`; see [`resolver.md`](../design/features/resolver.md).

8. **Generics: params registered, not fully constrained** — **Partially addressed (MVP scaffold)**
   Resolver rejects duplicate generic names, generic params in value positions (`GenericParamInValue` E1016), and duplicate `Type :: impl :: Trait` (E1017). Inference, orphan rules, and associated types remain post-MVP—[`04-type-system.md`](04-type-system.md).

9. **Closures will need value capture + outer scope chain** — **Partially addressed**
   `DefKind::Closure`, `ClosureUpvar`, and `ResolvedProgram::closures` populated at resolve time; typeck still `UnsupportedFeature` until closure types and lowering exist.

## What's working well

- **Separate value and type namespaces** with inner-to-outer lookup.
- **`#import` graph**: BFS discovery, topological sort, `CircularImport` with spans.
- **Two-phase crate resolve**: collect defs + imports, then `build_import_bindings` with `ImportNotExported` / `DuplicateImport`.
- **`DefKind` is `#[non_exhaustive]`**: compiler updates forced when language grows.
- **Design doc** [`resolver.md`](../design/features/resolver.md) records `AstNodeId`, closures, and MVP generic checks.

## Recommended next actions

1. Wire `bindings_from_pxi` for phase-1 import surface when dependency source parse can be skipped.
2. Session `DefId` ← `stable_export_id` map at crate load (finding 2).
3. Merge per-segment path spans for richer “cannot find” diagnostics.
4. Closure types + `MakeClosure` lowering (coordinate with [`04-type-system.md`](04-type-system.md), [`05-ir-and-lowering.md`](05-ir-and-lowering.md)).

**Cross-references:** Diagnostic formatting—[`02-diagnostics.md`](02-diagnostics.md). Monomorphization and generic calls—[`04-type-system.md`](04-type-system.md). AST ids—[`01-syntax-and-ast.md`](01-syntax-and-ast.md).
