# Phoenix — Feature design

Deep dives for language features that go beyond surface syntax. Start with [grammer.md](../grammer.md) (overview) and [grammar.ebnf](../grammar.ebnf) (formal EBNF).

**Work sequencing:** [language-v0.md](../language-v0.md) — the ordered checklist from MVP through Language v0.

| Document | Topic |
|---|---|
| [type-system.md](type-system.md) | Primitives vs std vs sugar; `Option`, `Result`, tuples, unit |
| [traits.md](traits.md) | `Name :: trait`, `Type :: impl`, generics bounds, iteration protocol |
| [grammar-deferred.md](grammar-deferred.md) | Parse-only syntax and not-yet-designed grammar |
| [ownership.md](ownership.md) | Moves, borrows, `Copyable`, std `Clone` |
| [messages.md](messages.md) | Post-MVP actor message ownership model |
| [error-handling.md](error-handling.md) | `Result`, `?`, no exceptions |
| [runtime-transparency.md](runtime-transparency.md) | Runtime transparency principle; pure vs schedulable I/O vs actors |
| [modules.md](modules.md) | File-based modules, `pub`, `#import`, build layout, [distribution evolution](modules.md#distribution-and-packaging-evolution) |
| [compiler-directives.md](compiler-directives.md) | `#` compile-time vs `@` runtime directives |
| [concurrency.md](concurrency.md) | Scheduler, schedulable I/O, explicit actors |
| [../research/concurrency-models-research.md](../research/concurrency-models-research.md) | Cross-language concurrency research (informing post-MVP design) |
| [vm-linear.md](vm-linear.md) | MVP bytecode format and VM contract |
| [wide-integers.md](wide-integers.md) | Why 256/512-bit types are deferred |
