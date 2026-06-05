# Review: Modules and Build (`phx-compiler/modules/`, `build/`, `link/`, `project/`, `pxi/`)

## Summary

Multi-file M2 builds are in good shape: import graphs, path dependencies, incremental manifests, per-module `.phx0` + link, and cycle detection with `.pxi` escape. This review closed **dependency `.pxi` routing**, **per-module incremental codegen**, **project-aware `phx check`**, **link contract tests/docs**, and **`phoenix.toml` forward-compat fields**. **Deferred:** registry/semver, workspace members, parsing dependency bodies only, subset typeck, `phx test` on libs.

## Findings

1. **`.pxi` used for staleness, not for resolve/typeck** — **Partially addressed**
   [`module_artifacts_resolved`](../../source/phx-compiler/src/project/layout.rs) reads path-dep interfaces from `build/deps/{name}/pxi/`. [`exports_for_dependency`](../../source/phx-compiler/src/modules/resolve_crate.rs) and [`import_types`](../../source/phx-compiler/src/typeck/check.rs) use fresh `.pxi` for export surfaces and structured types. [`bindings_from_pxi`](../../source/phx-compiler/src/modules/interface_loader.rs) remains for a future skip-parse path.

2. **`phoenix.toml` minimal surface** — **Partially addressed**
   [`project/config.rs`](../../source/phx-compiler/src/project/config.rs) parses `version`, `description`, `edition`, and `module_roots` (reserved). Workspace members and registry deps remain future work.

3. **Link merges pools; assumes global `function_id` map** — **Addressed**
   Documented on [`link_modules`](../../source/phx-compiler/src/link/mod.rs). Test: [`tests/link.rs`](../../source/phx-compiler/tests/link.rs). Design updated in [`modules.md`](../design/features/modules.md).

4. **Circular import: tested in fixtures, weak diagnostics** — **Addressed**
   [`graph.rs`](../../source/phx-compiler/src/modules/graph.rs) reports at import-edge span. CLI [`check.sh`](../../tests/cli/check.sh) expects `cycle` and `E1008` / `circular module import`.

5. **`compile_source` / CLI check without module root** — **Addressed**
   [`check_file`](../../source/phx-compiler/src/compile.rs) discovers `phoenix.toml` and calls [`check_project_file`](../../source/phx-compiler/src/compile.rs). CLI `phx check` mirrors this when `--module-src` is omitted.

6. **Lib packages: build but no `load_project_binary` run** — **Documented (MVP)**
   [`load_project_binary`](../../source/phx-compiler/src/build/driver.rs) rejects non-bin; documented in [`modules.md`](../design/features/modules.md).

7. **No package registry / semver** — **Documented (deferred)**
   Path dependencies only; lockfile/registry deferred.

8. **Workspace-scale limitation: single package per `phoenix.toml`** — **Documented (deferred)**
   No `[workspace.members]` yet.

9. **Incremental manifest: source + pxi hash** — **Addressed**
   [`build/driver.rs`](../../source/phx-compiler/src/build/driver.rs) skips lower/codegen when [`module_is_up_to_date`](../../source/phx-compiler/src/build/manifest.rs) and reuses manifest `.phx0`. Integration: [`incremental_build.rs`](../../tests/integration/tests/incremental_build.rs).

## What's working well

- **Loader BFS + topo sort** with explicit cycle error and `.pxi` SCC escape.
- **Two-phase `resolve_crate`** with `.pxi`-filtered exports and `import_types`.
- **`canonicalize_import` / `ModulePath`** filesystem mapping.
- **Path dependency project** ([`run_dep_build.rs`](../../tests/integration/tests/run_dep_build.rs), `app_dep` fixture).
- **`.pxi` v2 JSON** structured export types without external deps.

## Recommended next actions

1. Wire `bindings_from_pxi` when dependency source parse can be skipped.
2. Session `DefId` ← `stable_export_id` at crate load (with [`03-resolver.md`](03-resolver.md)).
3. `[workspace.members]` in `phoenix.toml` before a second in-repo package.
4. `phx test` entry for `type = lib` crates.

**Cross-references:** Resolver — [`03-resolver.md`](03-resolver.md). Diagnostics — [`02-diagnostics.md`](02-diagnostics.md). Tests — [`09-testing-strategy.md`](09-testing-strategy.md).
