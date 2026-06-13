# phx-diagnostics

Shared compiler and VM diagnostics: source spans, structured errors, and CLI formatting.

## Spans and modules

[`Span`](src/span.rs) is a half-open byte range into a **single module's** UTF-8 source buffer. Multi-file crates attach a module id via [`LocatedError`](src/located.rs) until spans carry a file id.

## Formatting

- [`format_span_message`](src/format.rs) — line, column, and caret (ASCII-first: columns count Unicode scalar values; caret width uses bytes).
- [`format_resolve_error`](src/format.rs) / [`format_typecheck_error`](src/format.rs) — interned names via [`SymbolNames`](src/symbol_names.rs) (implemented on [`Interner`](../phx-syntax/src/intern.rs)).
- [`format_lex_error`](src/format.rs) — lex failures with carets instead of raw offsets.
- [`format_parse_error`](src/format.rs) / [`parse_message`](src/format.rs) — parse failures (delegates lex errors to the lex formatter).

Stable codes ([`DiagnosticCode`](src/code.rs)) are appended in formatters as `[E####]`; message text may change.

### Resolve codes (E1xxx)

| Code | Variant |
|------|---------|
| E1001 | `UnresolvedIdent` |
| E1002 | `UnresolvedType` |
| E1003 | `DuplicateDefinition` |
| E1004 | `ImportNotSupported` |
| E1005 | `ModuleNotFound` |
| E1006 | `ModuleIo` |
| E1007 | `ModuleParse` |
| E1008 | `CircularImport` |
| E1009 | `ImportNotExported` |
| E1010 | `ImportNotFound` |
| E1011 | `DuplicateImport` |
| E1012 | `MainNotInEntry` |
| E1013 | `MissingMain` |
| E1014 | `MainForbiddenInLib` |
| E1015 | `InvalidMainSignature` |

### Type-check codes (E2xxx)

E2001–E2039 — see [`type_error_registry.rs`](src/type_error_registry.rs) (single source for variant → code mapping).

### Parse codes (E3xxx)

E3001–E3005 — see [`ParseError::code`](src/parse_error.rs).

### Lex codes (E0xxx)

E0001–E0008 — see [`LexError::code`](src/lex_error.rs).

Warning codes (`W####`) are reserved for future use.
