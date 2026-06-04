# Review: Diagnostics (`phx-diagnostics` + compiler formatting)

## Summary

Diagnostic **UX matches MVP multi-file needs**: stable `DiagnosticCode` (E/W prefixes), module-aware `[DiagnosticContext](../../../source/phx-compiler/src/compile.rs)`, caret formatting for **all** parse/resolve/typeck bag errors, interner-backed resolve messages, duplicate-definition notes, and lex formatters. Remaining gaps are low-priority (byte vs char columns) and a few **synthetic spans** for impl `self` and similar built-ins.

## Findings

1. **Bags collect errors; CLI shows one caret** — **Addressed**
  `format_parse_bag`, `format_resolve_bag`, and `format_typecheck_bag` loop all errors with `---` separators and per-module source buffers.
2. **Multi-file span → wrong source buffer** — **Addressed**
  `DiagnosticContext` + `source_for_module` route diagnostics by `located.module` / logical path, not heuristics on file length.
3. **Stub spans (`0, 0`) on common failures** — **Partially addressed**
  `CircularImport`, `MissingMain`, and many import/resolve sites use real spans; `impl` receiver (`self`) and a few synthetic defs still use `Span::new(0, 0)` where no source token exists.
4. `**sym#{index}` in resolve `Display`** — **Addressed**
  `format_resolve_error` / `resolve_message` take `&Interner` and print spellings.
5. `**DuplicateDefinition` ignores `first_span`** — **Addressed**
  `format_span_message_with_note` for value and type duplicates (and trait-impl duplicates).
6. **Lexer errors lack caret helper** — **Addressed**
  `format_lex_error`; parse pipeline routes `ParseError::Lex` through it.
7. **Column computation character-based, caret byte-based** — **Deferred `[low]`**
  Acceptable for MVP ASCII; unify when non-ASCII source is in scope.
8. **No diagnostic codes or severity** — **Addressed**
  `DiagnosticCode` on lex/parse/resolve/typeck variants (e.g. E0006, E1016); messages remain mutable.
9. **Parse pipeline is single-error** — **Addressed**
  Parser returns `ParseBag`; blocked-on-recovery item closed with finding 1 in `[01-syntax-and-ast.md](01-syntax-and-ast.md)`.

## What's working well

- `**format_typecheck_error` + `UseAfterMove` note**: two-site rendering with move site.
- **Bag types + `into_errors`**: passes continue collecting after individual failures.
- **Module labels on located errors**: multi-file crates show which file each caret belongs to.
- **Small re-export surface** (`phx-diagnostics/lib.rs`): stable API for compiler crates.

## Recommended next actions

1. Replace remaining `0, 0` spans where a real token exists (audit `walk.rs` import seeding and path merge).
2. Optional: byte-consistent columns for caret width (finding 7).
3. Post-MVP: `--explain Ecode`, warning severity, LSP JSON (no schema lock-in yet).

**Cross-references:** Resolver spans—`[03-resolver.md](03-resolver.md)`. Crate loader API—`[08-modules-and-build.md](08-modules-and-build.md)` (if present in review set).