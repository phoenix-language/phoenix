# Review: Modules and Build (`phx-compiler/modules/`, `build/`, `link/`, `project/`, `pxi/`)

## Summary

Multi-file builds work for **path dependencies, import graphs, incremental manifests, per-module `.phx0` + link**, and cycle detection with a `.pxi` freshness escape. **Needs attention** for workspace scale: `phoenix.toml` is minimal, `.pxi` is not used for cross-module typechecking in the main pipeline, link does not patch `Call` targets (relies on pre-assigned global fn ids), lib packages cannot `phx run`, and there is no registry/version resolution story.

## Findings

1. **`.pxi` used for staleness, not for resolve/typeck**
   Build hashes `.pxi` for incremental skip (`build/driver.rs:108–118`); `interface_loader` not on `resolve_crate` path ([`03-resolver.md`](03-resolver.md)).
   **Severity:** `[high]`
   **Recommendation:** Typecheck imported modules from `.pxi` signatures when source unchanged; parse bodies only for modules being compiled.

2. **`phoenix.toml` minimal surface**
   Hand-rolled parser in `project/config.rs`—package name, type bin/lib, path deps only.
   **Severity:** `[medium]`
   **Recommendation:** Add `version`, `edition`, and `module_roots[]` fields now (ignored if unused) to avoid breaking existing projects later.

3. **Link merges pools; assumes global `function_id` map**
   `build_global_fn_map` (`driver.rs:285–290`) assigns ids before link; linker patches const/type indices only (`link/mod.rs:204–220`).
   **Severity:** `[medium]`
   **Recommendation:** Document invariant; add link test that two modules’ `Call` operands still resolve after merge.

4. **Circular import: tested in fixtures, weak diagnostics**
   `tests/cli/fixtures/modules/cycle_a.phx` / `cycle_b.phx` exist; error span is often `0,0`.
   **Severity:** `[medium]`
   **Recommendation:** CLI test expecting `CircularImport` substring; fix span at import site.

5. **`compile_source` / CLI check without module root**
   `tests/cli/README.md` notes `compile_source(..., None)` cannot `#import`.
   **Severity:** `[high]` for developer UX
   **Recommendation:** `phx check` should pass discovered `module_root` from project layout automatically.

6. **Lib packages: build but no `load_project_binary` run**
   `load_project_binary` rejects non-bin (`driver.rs:261–264`).
   **Severity:** `[low]` (MVP)
   **Recommendation:** Document; add `phx test` on lib crates via test entry symbol later.

7. **No package registry / semver**
   Dependencies are path-only (`config.rs:42–44`).
   **Severity:** `[future]`
   **Recommendation:** When adding registry, require `.pxi` + `.phx0` hash lockfile; stable module path keys already exist in `ModulePath`.

8. **Workspace-scale limitation: single package per `phoenix.toml`**
   No workspace members array.
   **Severity:** `[future]`
   **Recommendation:** Mirror Cargo `[workspace.members]` in TOML schema before second package lands.

9. **Incremental manifest: source + pxi hash**
   `build/manifest.rs` tracks freshness—good foundation.
   **Severity:** `[low]` (positive)
   **Recommendation:** Add manifest integration test: touch one file, only dependent modules rebuild.

## What's working well

- **Loader BFS + topo sort** with explicit cycle error (`loader.rs`, `graph.rs`).
- **Two-phase `resolve_crate`** with import bindings (`resolve_crate.rs`).
- **`canonicalize_import` / `ModulePath`** filesystem mapping (`modules/path.rs`).
- **Path dependency project** tested (`tests/integration/run_dep_build.rs`, `app_dep` fixture).
- **`.pxi` v1 JSON** without external deps (`pxi/format.rs`).

## Recommended next actions

1. Wire `.pxi` into resolver/typeck for dependencies.
2. Extend `.pxi` export schema (v2) for separate compilation.
3. Auto-discover module root in CLI `check`/`run`.
4. Add incremental rebuild test to integration suite.
5. Add `version` field to `phoenix.toml` (documentation only).

**Cross-references:** `DefId` stability—[`03-resolver.md`](03-resolver.md). Diagnostics multi-file—[`02-diagnostics.md`](02-diagnostics.md). Tests—[`09-testing-strategy.md`](09-testing-strategy.md).
