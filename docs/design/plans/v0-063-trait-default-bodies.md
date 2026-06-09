# V0-063 — Trait default bodies implementation plan

Status: **Done** (implemented)

**Authority:** [language-v0-completion-roadmap.md](../language-v0-completion-roadmap.md) (lines 172–213)

---

## Summary

Empty or partial `Type :: impl :: Trait { }` blocks inherit trait methods that carry default bodies. Inherited methods are synthesized during typeck into `TypedProgram.inherited_trait_methods`, registered in `program_layout.trait_methods`, type-checked with trait generic substitution, lowered, and emitted like explicit impl methods.

## Key implementation

| Component | Location |
|-----------|----------|
| Synthesis + lookup | `source/phx-compiler/src/typeck/trait_defaults.rs` |
| Collect + check hooks | `source/phx-compiler/src/typeck/check.rs` |
| Mono / lower lookup | `lookup_function()` in `typeck/mod.rs`, used by `mono.rs` and `lower/func.rs` |
| Std `Into` default | `std/src/core/convert.phx` |

## Acceptance (roadmap)

- Trait default + empty impl compiles and runs (`tests/cli/fixtures/trait_default/`)
- Override replaces default (`trait_default_override/`)
- Missing method without default → `MissingTraitMethod`
- `Into` default from `From` (`trait_into_from_default/`)
- `just pre-commit` green
