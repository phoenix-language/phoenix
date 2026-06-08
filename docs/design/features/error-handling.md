# Error handling

Phoenix has no `throw` or `catch`. Functions that can fail return `Result<T, E>`; absence uses `Option<T>`. Both are **std generic enums** (post-MVP), not MVP compiler builtins. See [type-system.md](type-system.md#phased-option-and-result-language--std).

---

## Handling failures

Callers must handle both cases — via `match`, `given`, or `?` inside another `Result`-returning function.

```
connect :: (addr: [u8]) => Result<Connection, Error>
{
  // ...
};

handle :: () =>
{
  match connect([108u, 111u, 99u, 97u, 108u])
  {
    Ok(conn)  => run(conn),
    Err(e)    => log(e),
  };
};
```

Errors are values, not control-flow exceptions. Failure paths stay visible in types.

---

## The `?` operator (V0-042, error conversion V0-059)

`?` is postfix sugar over std `Option` / `Result` (requires `#import std::core::…` or prelude when `prelude = true`). Every call site that can fail shows propagation explicitly. See [runtime-transparency.md](runtime-transparency.md).

### Rules

| Enclosing function return | `expr` type | `expr?` result type | On failure |
|---|---|---|---|
| `Result<T, E>` | `Result<T, E>` (same `T` and `E`) | `T` | `return Err(e)` |
| `Result<T, E_out>` | `Result<T, E_in>` (same `T`, different `E`) | `T` | `return Err(E_out::from(e))` — requires `From<E_in>` for `E_out` ([V0-059](../language-v0.md#v0-059--with-from-error-conversion)) |
| `Option<T>` | `Option<T>` (same `T`) | `T` | `return None` |

- `?` is valid only inside a function body with a compatible return type (`main :: () =>` cannot use `?`).
- **Ok payload types must unify** — `?` does not convert success values in v1.
- `Result`-returning functions cannot `?` an `Option`, and vice versa (no `Try` trait bridge in Language v0).
- Types must be the std definitions from `std::core::option` / `std::core::result` — not user enums with the same variant names.
- Missing `From` impl when `E_in ≠ E_out` is a compile error with a note to implement `From<E_in>` for `E_out`.

**V0-042 (shipped):** identical `Result<T, E>` only. **V0-059 (shipped):** adds the second row (`From` conversion when `E_in ≠ E_out`).

### Example (same error type)

```
read_bytes :: (path: [u8]) => Result<s32, Error> { /* … */ };

read_config :: (path: [u8]) => Result<Config, Error>
{
  const text = read_bytes(path)?;
  parse(text)?
};
```

### Example (layered types via `From`)

```
read_bytes :: (path: [u8]) => Result<s32, LeafError> { /* … */ };

read_config :: (path: [u8]) => Result<Config, AppError>
{
  const text = read_bytes(path)?;   // LeafError → AppError via From
  parse(text)?
};
```

Leaf and app error types are **crate-defined**; std ships only the base `Error` enum (see [Std error vocabulary](#std-error-vocabulary--v0-060)).

Inside `Option`-returning functions, `?` propagates `None` the same way.

### Desugaring

`expr?` in a `Result` function lowers to: evaluate `expr`; on `Err`, load the error payload; if `E_in ≠ E_out`, call monomorphized `From::from(e)`; construct `Err(converted)` and `return`; on `Ok`, bind the payload and continue. When `E_in` and `E_out` are identical, skip the `From` call.

`Option` functions use `None` / `Some` analogously — no conversion path. See [type-system.md](type-system.md#syntactic-sugar).

## Std enum constructors (V0-042)

`Some`, `None`, `Ok`, and `Err` resolve to std enum variant constructors when imported from `std::core::option` / `std::core::result`. Generic args may be inferred from the expected type (binding annotation, function return type) or call-site arguments; explicit `:: <…>` remains valid.

---

## Error conversion (`From` / `Into`) — V0-058

Phoenix does **not** convert errors with `as` or implicit coercions. Conversions are **std trait methods** with static dispatch (monomorphization), same as other generic behavior. See [traits.md](traits.md#conversion-traits-from--into).

| Mechanism | When | Returns |
|---|---|---|
| `From<Source>` | Infallible value conversion | `Target` |
| `Into<Target>` | Convenience inverse (manual or default body) | `Target` |
| `TryFrom<Source>` | Fallible conversion (parse, bounds, UTF-8) | `Result<Target, E>` |

**Policy:**

- **`as` never converts errors** — struct/enum punning is forbidden ([type-system.md](type-system.md#tier-c--forbidden-via-as)).
- **`?` is the ergonomic boundary** — leaf functions may return precise local error types; application layers return a wider enum; `From` bridges at each `?` site when `E_in ≠ E_out`.
- **No compiler `Ty::Error` builtin** — all error types are ordinary std enums/structs, like `Option` and `Result`.
- **Orphan rule applies** — `From` impls for std error types live in the crate that defines the source or target type ([traits.md](traits.md#trait-impl-scope-and-orphans)).

Primitive numeric conversions remain **`expr as Type`** (truncating/wrapping). `From`/`TryFrom` for numerics are separate, explicit APIs — Phoenix does not silently widen or narrow at call sites.

### Orphan rule for std error `From` impls (V0-058)

Enforcement is **documented only** for V0-058; the resolver does not yet reject orphan violations.

| Rule | Detail |
|---|---|
| Std `Error` | Single expandable enum in `std::error` (V0-060); std adds variants when real subsystems ship — no placeholder leaf types. |
| Leaf error types | Crate-local structs/enums for precise failure semantics; `From<Leaf> for AppError` in the owning crate. |
| Downstream crates | Must not add `From` (or `TryFrom`) impls whose **source or target** is a std type they do not own (orphan rule). |
| User types | `From<UserLeaf> for UserError` in the same crate is fine. |

See also [traits.md](traits.md#trait-impl-scope-and-orphans).

---

## Std error vocabulary — V0-060

**Implemented** in `std/src/error/mod.phx`. Acceptance fixtures: `tests/cli/fixtures/std_errors/` (`Result` + `?`), `std_try_from/` (layered `From`).

Std owns one **base** `Error` enum — expandable as I/O, parsing, and threading land. The compiler only recognizes std `Result`/`Option` for `?` sugar — error enums are not special-cased (same policy as [Phased: Option and Result](type-system.md#phased-option-and-result-language--std)).

### Module layout

```
std/
  core/
    convert.phx     # From, Into, TryFrom, TryInto
    result.phx
    option.phx
  error/
    mod.phx         # pub Error enum
```

### Std v0 default — minimal expandable enum

```phoenix
pub Error :: enum {
  Unknown(s32),
}
```

Add variants (`Io(…)`, `Parse(…)`, …) when those subsystems exist. Crate-local leaf types and `From` bridges remain the pattern for application layering until std owns those domains.

### Pattern B — opaque newtypes (post–V0-057)

Distinct nominal wrappers (`UserId`-style) for domain errors and a structured `Error` type. Better long-term API evolution; same `From`/`?` mechanics.

### Source chains and `dyn Error` (deferred)

Language v0 uses concrete errors + `Debug` / `Display` traits only. Rust-style `Error::source()` returning trait objects waits for `dyn Trait` ([type-system.md](type-system.md#deferred-dyn-trait)). Until then, optional `context: str` fields or inherent `with_context` methods on `Error` are sufficient for demos.

---

## Layered error design

| Layer | Return type | Role |
|---|---|---|
| Leaf (syscall wrapper, parser) | `Result<T, LeafError>` (crate-local) | Precise, local failure semantics |
| Module / service | `Result<T, Error>` or app enum | Composes leaf errors via `From` when types differ |
| `main` | `()` + `match` on `Result` | No `?` in zero-`Result` entry |

Cross-cutting concerns (schedulable I/O, actor mailboxes) will surface failure in types at call sites when those runtimes land — same `Result` model, not exceptions. See [runtime-transparency.md](runtime-transparency.md).

---

## Non-goals (Language v0)

- `Option` `?` inside `Result` functions (and vice versa).
- Implicit error coercion without a visible `From` impl.
- `as Error` or enum layout punning for conversions.
- Compiler builtins for std error enums.
- `dyn Error` / boxed trait-object error chains.
