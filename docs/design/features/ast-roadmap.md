# AST roadmap (syntax front end)

Reserved and planned AST shapes referenced by compiler reviews. Implementation order follows [mvp.md](../mvp.md).

## Identifier spans and node ids

- **Done (MVP):** [`Ident`](../../source/phx-syntax/src/ast/ident.rs) and [`TypeName`](../../source/phx-syntax/src/ast/ident.rs) carry `symbol` + `span` at use sites.
- **Done:** [`AstNodeId`](../../source/phx-syntax/src/ast/node_id.rs) on [`Node<T>`](../../source/phx-syntax/src/ast/node.rs) and identifiers; resolver [`ResolutionKey`](../../source/phx-compiler/src/resolver/mod.rs) keys on `node_id`.

## Expression variants (planned)

| Variant | Purpose |
|---------|---------|
| `Expr::EnumCtor` | Unit/tuple/struct enum constructors without overloading `Postfix` |
| `Expr::Range` | Parsed; typeck rejects until std range types |
| `Expr::Lambda` | Parsed; resolver capture table + `DefKind::Closure`; typeck/lowering TBD |
| `Expr::RuntimeDirective` | Parsed `@spawn` / `@send` / …; post-MVP runtime |

## Statement variants (planned)

| Variant | Purpose |
|---------|---------|
| `Stmt::ForIn` | Lowered via `IntoIter` + `Iterator` protocol ([V0-055](../language-v0.md#v0-055--iterator-protocol-and-for-lowering)) |
| Labeled `break` / `continue` | Not in grammar yet |

## Unicode identifiers

MVP lexer uses ASCII-only identifier rules and byte cursor advancement. Unicode identifiers require `chars()` iteration and updated `Ident` policy — see [grammar-deferred.md](grammar-deferred.md).
