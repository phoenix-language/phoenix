# Review: Syntax and AST (`phx-syntax`)

## Summary

The front end is **production-ready for MVP ASCII programs**: lexer coverage is regression-tested, parsing collects multiple errors via [`ParseBag`](../../../source/phx-diagnostics/src/parse_error.rs), deferred grammar is **parsed then rejected in later passes** per [`grammar-deferred.md`](../design/features/grammar-deferred.md), and the AST carries **`Ident` / `TypeName` use-site spans** plus [`AstNodeId`](../../../source/phx-syntax/src/ast/node_id.rs) for resolver keys. Remaining gaps are documented limits (ASCII-only identifiers, struct literals from `snake_case` paths without generic args), and post-MVP items (`Expr::EnumCtor`, Unicode identifiers).

## Findings

1. **No parser error recovery** — **Addressed**
   `parse_with_interner` enables recovery, accumulates into `ParseBag`, and returns partial AST when possible (`parser/mod.rs`). CLI formats all parse errors (`format_parse_bag` in `compile.rs`).

2. **Deferred grammar rejected at parse, not parse-then-defer** — **Addressed**
   `for`, lambdas, ranges, `@` directives, and `#derive` parse into AST (`Stmt::ForIn`, `Expr::Lambda`, `Expr::Range`, …); typeck/resolver report `UnsupportedFeature` or deferred errors. Policy aligned with `grammar-deferred.md` and parser tests (`deferred_parse_*`).

3. **Struct literal generics parsed then dropped** — **Partially addressed**
   `TypeIdent` struct literals store `generics` from `parse_path_or_struct_literal` (`parser/expr.rs`). Plain `ident` path struct literals still use `generics: None` (no `Foo::<T>` on snake_case path). Typeck can reject until generic struct use sites land.

4. **ASCII-only identifiers; byte cursor UTF-8-unsafe** — **Deferred `[future]`**
   Unchanged for MVP; document ASCII-only in user-facing docs when published.

5. **Interner: linear dedup and silent `u32::MAX` overflow** — **Addressed**
   `HashMap<String, u32>` dedup; [`InternError::TableFull`](../../../source/phx-syntax/src/intern.rs) instead of silent clamp.

6. **Breaking AST changes needed for closures, generics, strings, `for`** — **Partially addressed**
   `Ident` / `TypeName` + `AstNodeId`; `Expr::Lambda`, `Stmt::ForIn`, `Expr::Range` in AST. Still to add: `Expr::EnumCtor`, labeled `break`/`continue`, string types. See [`ast-roadmap.md`](../design/features/ast-roadmap.md).

7. **`while` condition parsed at assignment precedence** — **Addressed**
   `parse_while_stmt` uses `parse_logical_or_expr` like `if`.

8. **`unreachable!` in expression parser** — **Addressed**
   `parse_equality_expr` uses `error_unexpected` on unexpected tokens.

9. **Integer lex errors reported as `InvalidFloat`** — **Addressed**
   `LexError::InvalidInt` (E0006) with lexer tests updated.

## What's working well

- **`TokenKind` + lexer tests**: table-driven coverage per token and `LexError` variant.
- **`Node<T>` + half-open `Span`**: suitable for diagnostics and LSP mapping.
- **`ParseBag` recovery**: multi-error CLI output with `---` separators.
- **Deferred-syntax staging**: parse real AST, fail in typeck/resolver with clear errors.
- **Borrowed lexemes + intern table**: `SourceFile` owns `Interner`; no raw strings in nodes.

## Recommended next actions

1. Store struct-literal `generics` on snake_case `ident` path when `::<…>` appears (finding 3 tail).
2. Add `Expr::EnumCtor` before enum constructor syntax grows further.
3. Post-MVP: Unicode identifier policy + `chars()` lexer iteration (finding 4).

**Cross-references:** Multi-error formatting—[`02-diagnostics.md`](02-diagnostics.md). `AstNodeId` resolution keys—[`03-resolver.md`](03-resolver.md). Generic calls—[`04-type-system.md`](04-type-system.md).
