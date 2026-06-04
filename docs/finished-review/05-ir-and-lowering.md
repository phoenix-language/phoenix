# Review: IR and Lowering (`phx-compiler/ir/`, `lower/`)

## Summary

MVP lowering produces a **verifiable stack IR**: explicit CFG, short-circuit booleans, match chains, loop back-edge patching, and `ExprId`-aligned locals. Recent work fixed **duplicate `Return`**, **loop-control diagnostic spans**, **unresolved callee guards** (`LowerError` E4001), and **defensive empty-match traps**. Ownership stays erased in IR (typeck-only). **Deferred:** per-instruction source spans on `IrInst`, closure/`?` IR, and DCE purity flags.

## Findings

1. **Ownership erased at lowering** — **Documented (MVP)**
   Moves enforced in typeck; IR uses `LoadLocal`/`StoreLocal`. Documented in [`ir/mod.rs`](../../source/phx-compiler/src/ir/mod.rs) and [`ownership.md`](../design/features/ownership.md). Post-MVP may add `IrInst::Move`.

2. **No span on `IrInst`** — **Deferred `[medium]`**
   Follow-up: optional `span` on `IrInst` or parallel table; populate from AST at every `LowerCtx::emit` in [`lower/`](../../source/phx-compiler/src/lower/).

3. **Duplicate `Return` emission** — **Addressed**
   `lower_function_return` skips when tail block already ends with `IrInst::Return`. Test: `explicit_return_emits_single_return` in [`lower.rs`](../../source/phx-compiler/tests/lower.rs).

4. **`break` / `continue` outside loop** — **Addressed**
   Typeck emits `LoopControlOutsideLoop` (E2020) with keyword spans on `Stmt::Break` / `Stmt::Continue`. Lower still no-ops if typeck is bypassed.

5. **`if` expression merge without phi** — **Documented (MVP)**
   Merge-block stack discipline documented in [`ir/mod.rs`](../../source/phx-compiler/src/ir/mod.rs); bytecode verifier enforces stack effects.

6. **`match` empty arms early return** — **Addressed**
   `lower_match` emits `TrapGivenMismatch` when `arms` is empty (defensive; typeck rejects user empty matches).

7. **`resolve_call_callee` fallback `DefId(0)`** — **Addressed**
   `Option<DefId>` + `LowerError::UnresolvedCallee` (E4001); `lower()` returns `LowerBag`; `CompileError::Lower` in compile driver. `debug_assert` on invariant break.

8. **Comparison lowering: `Gt` via swapped `Lt`** — **Addressed**
   Documented pattern; test `greater_than_lowers_via_swapped_lt` in [`codegen.rs`](../../source/phx-compiler/tests/codegen.rs).

9. **Future: closures need `MakeClosure` + upvar loads** — **Deferred `[future]`**
   Resolver scaffold exists; no closure IR yet.

10. **Future: `?` needs branching IR** — **Deferred `[future]`**
    Desugar to `match` first when std `Result` ships.

11. **IR amenable to DCE only after pure annotation** — **Deferred `[future]`**
    No `InstFlags::PURE` on `IrInst` in MVP.

## What's working well

- **Documented lowering contract** in `ir/mod.rs` and `lower/mod.rs`.
- **Short-circuit `&&` / `||`** via CFG.
- **Loop back-edges** with patched placeholder ids.
- **`TrapGivenMismatch`** for failed `given`/match paths.
- **`ExprId` cursor** tied to typeck layout.

## Recommended next actions

1. **IR spans (Phase 2):** add optional spans on `IrInst` for debug/LSP/IR passes.
2. Closure IR: `MakeClosure`, `LoadUpvar`, capture lists when lambdas type-check.
3. Coordinate `.pxi`/link-stable ids with [`08-modules-and-build.md`](08-modules-and-build.md) if present.

**Cross-references:** Stack verification—[`06-codegen-and-bytecode.md`](06-codegen-and-bytecode.md). Ownership/typeck—[`04-type-system.md`](04-type-system.md). Loop-control spans—[`01-syntax-and-ast.md`](01-syntax-and-ast.md), [`02-diagnostics.md`](02-diagnostics.md).
