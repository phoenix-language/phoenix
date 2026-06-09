# V0-067 — Cross-pillar integration (Phase 7 capstone)

Status: **Done** (implemented)

**Authority:** [language-v0-completion-roadmap.md](../language-v0-completion-roadmap.md) (lines 268–282)

---

## Summary

Single fixture [`tests/cli/fixtures/std_platform_smoke/`](../../../tests/cli/fixtures/std_platform_smoke/) exercises Phase 7 gates together:

| Pillar | Feature | In fixture |
|--------|---------|------------|
| V0-064 | `match` on `Result<Config, AppError>` | `read_config()` match arms |
| V0-063 | Trait default via empty impl | `HeapTag :: impl :: ByteMarker { }` inherits `marker_byte` |
| V0-062 | Heap slice over `alloc_bytes` | `slice_from_raw_parts` inside `unsafe` |

Expected VM result: `sum = 42 + 77 = 119` (config value + inherited marker byte read via heap slice).

## Acceptance (roadmap)

- `std_platform_smoke` builds and runs
- `examples/errors` covered by `examples_errors_build_run` in `test-lang`
- Checklist rows for heap slices, trait defaults, and Result match cite `std_platform_smoke`
