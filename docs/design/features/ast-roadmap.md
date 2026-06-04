# AST roadmap (syntax front end)

Reserved and planned AST shapes referenced by compiler reviews. Implementation order follows [mvp.md](../mvp.md).

## Identifier spans

- **Done (MVP):** [`Ident`](../../source/phx-syntax/src/ast/ident.rs) carries `symbol` + `span` at use sites.
- **Later:** `TypeName` / `PathSegment` spans for richer path diagnostics.

## Expression variants (planned)

| Variant | Purpose |
|---------|---------|
| `Expr::EnumCtor` | Unit/tuple/struct enum constructors without overloading `Postfix` |
| `Expr::Range` | Parsed; typeck rejects until std range types |
| `Expr::Lambda` | Parsed; closure typing/lowering TBD |
| `Expr::RuntimeDirective` | Parsed `@spawn` / `@send` / …; post-MVP runtime |

## Statement variants (planned)

| Variant | Purpose |
|---------|---------|
| `Stmt::ForIn` | Parsed; iterator desugaring TBD |
| Labeled `break` / `continue` | Not in grammar yet |

## Unicode identifiers

MVP lexer uses ASCII-only identifier rules and byte cursor advancement. Unicode identifiers require `chars()` iteration and updated `Ident` policy — see [grammar-deferred.md](grammar-deferred.md).
