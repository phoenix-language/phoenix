# Review: Diagnostics (`phx-diagnostics` + compiler formatting)

## Summary

Diagnostic **infrastructure is sound for single-file MVP**: half-open byte spans, caret rendering with capped width, and a strong `UseAfterMove` dual-span note. **Needs attention** for production UX: bags collect multiple errors but the CLI formats only the first with carets; many resolve/build errors use `Span::new(0, 0)`; resolve messages expose `sym#N` instead of names; and multi-file `source_for_span` routing can attach the wrong file. Warning codes, `--explain`, and LSP-shaped payloads are not started—add stable codes now without locking message text.

## Findings

1. **Bags collect errors; CLI shows one caret**
   `DiagnosticBag` / `TypeCheckBag` aggregate errors (`resolve_error.rs:228–275`, `type_error.rs:282–328`), but `format_resolve_bag` / `format_typecheck_bag` only call `format_span_message` for `errors()[0]` (`compile.rs:73–97`).
   **Severity:** `[high]`
   **Recommendation:** Loop all bag errors through `format_*_error` with a `---` separator; add an integration test expecting two carets from one compile.

2. **Multi-file span → wrong source buffer**
   `source_for_span` picks the first module where `span.end <= m.source.len()` (`compile.rs:101–114`), which can mis-attribute when files differ in length.
   **Severity:** `[high]`
   **Recommendation:** Store `file_id` or module index on each `Span` (or parallel `SpanFile` map); until then, tag each diagnostic with `module` from `Def::module` at emission sites.

3. **Stub spans (`0, 0`) on common failures**
   `CircularImport` (`graph.rs:73–76`), loader I/O errors, `MissingMain`, `InvalidMainSignature` params (`walk.rs:653–656`), import scope seeding (`walk.rs:46–49`) use zero spans.
   **Severity:** `[high]`
   **Recommendation:** Thread `#import` directive spans from AST into `ResolveError::CircularImport { span }`; require `main` keyword span for `MissingMain`.

4. **`sym#{index}` in resolve `Display`**
   `ResolveError` formatting uses raw symbol indices unless callers intern for display (`resolve_error.rs`).
   **Severity:** `[medium]`
   **Recommendation:** Pass `&Interner` into `format_resolve_error(source, interner, err)` and print `interner.resolve(symbol)` for all variants.

5. **`DuplicateDefinition` ignores `first_span`**
   The variant carries `first_span` but formatters only show the redefinition site.
   **Severity:** `[medium]`
   **Recommendation:** Mirror `UseAfterMove` with `format_span_message_with_note` (“previous definition here”).

6. **Lexer errors lack caret helper**
   `LexError` uses offsets in `Display` but no `format_lex_error(source, err)` (`lex_error.rs`).
   **Severity:** `[medium]`
   **Recommendation:** Add `LexError::span()` and a formatter in `phx-diagnostics`; route `CompileError::Parse` lex failures through it in `compile.rs`.

7. **Column computation is character-based, caret length is byte-based**
   `line_col` counts Unicode chars (`format.rs:67–83`) but caret width uses `span.end - span.start` bytes (`format.rs:14–19`).
   **Severity:** `[low]` (MVP is ASCII-first)
   **Recommendation:** Use byte columns consistently or document ASCII-only diagnostics until unified.

8. **No diagnostic codes or severity**
   Errors are free-form strings only; no `E00xx` or warning level.
   **Severity:** `[future]`
   **Recommendation:** Add `DiagnosticCode(&'static str)` on each error enum variant now; keep messages mutable. Reserve `W` prefix for warnings before any ship.

9. **Parse pipeline is single-error**
   Parser returns `Result<_, ParseError>` only—bags unused (`parse_error.rs:140–162`).
   **Severity:** `[medium]`
   **Recommendation:** Blocked on parse recovery in [`01-syntax-and-ast.md`](01-syntax-and-ast.md); once added, unify on `DiagnosticBag` at CLI boundary.

## What's working well

- **`format_typecheck_error` + `UseAfterMove` note**: actionable two-site rendering (`format.rs:28–49`, tested at `format.rs:99–110`).
- **Bag types with `into_errors`**: resolve and typeck continue collecting after individual failures.
- **Re-export surface** (`lib.rs`): small, stable API for compiler crates.

## Recommended next actions

1. Format all bag errors with carets in `compile.rs`.
2. Fix `source_for_span` or add module id to spans (cross-cut with [`08-modules-and-build.md`](08-modules-and-build.md)).
3. Replace `sym#` with interner names in resolve formatting.
4. Add `first_span` note for duplicate definitions.
5. Add `DiagnosticCode` newtype to error enums (no behavior change yet).

**Cross-references:** Stub spans overlap resolver gaps in [`03-resolver.md`](03-resolver.md). `compile_source` never passes `modules` to formatters—[`08-modules-and-build.md`](08-modules-and-build.md).
