# Name resolution

Resolver behavior for Phoenix MVP and near-term closure/generics work.

## Resolution keys (`AstNodeId`)

Every [`Node<T>`](../../../source/phx-syntax/src/ast/node.rs) and identifier use site ([`Ident`](../../../source/phx-syntax/src/ast/ident.rs), [`TypeName`](../../../source/phx-syntax/src/ast/ident.rs)) receives an [`AstNodeId`](../../../source/phx-syntax/src/ast/node_id.rs) at parse time.

[`ResolutionKey`](../../../source/phx-compiler/src/resolver/mod.rs) maps **`(module, node_id)` → [`DefId`]**, not span + symbol. `AstNodeId` values are unique per source file parse; the module disambiguates across a multi-file crate.

## Closures (parse-time AST, resolve-time captures)

Lambdas are parsed per [grammar-deferred.md](grammar-deferred.md); full typing and lowering remain deferred.

The resolver already:

- Introduces [`DefKind::Closure`](../../../source/phx-compiler/src/resolver/def_id.rs) for each `Expr::Lambda`.
- Tracks [`ResolvedProgram::closures`](../../../source/phx-compiler/src/resolver/mod.rs): parent closure link and [`ClosureUpvar`](../../../source/phx-compiler/src/resolver/mod.rs) list per closure def.
- Uses `scope_depth` on [`Def`](../../../source/phx-compiler/src/resolver/def_id.rs) to detect outer bindings referenced from a lambda body.

Typeck still reports `UnsupportedFeature` for lambdas until closure types and `MakeClosure` lowering exist.

## Generics and traits (scaffold)

MVP resolver checks (not full inference or Rust-style coherence):

| Check | Error |
|-------|--------|
| Duplicate generic parameter names on a decl | `DuplicateDefinition` |
| Generic parameter used as a value ident | `GenericParamInValue` (E1016) |
| Second `Type :: impl :: Trait` for same type + trait | `DuplicateTraitImpl` (E1017) |

Trait bound names are resolved as types. Full generic inference, orphan rules, and associated-type defaults are post-MVP — see [traits.md](traits.md).

## Related

| Topic | Document |
|-------|----------|
| AST roadmap | [ast-roadmap.md](ast-roadmap.md) |
| Deferred lambda semantics | [grammar-deferred.md](grammar-deferred.md) |
| Review findings | [../../finished-review/03-resolver.md](../../finished-review/03-resolver.md) |
