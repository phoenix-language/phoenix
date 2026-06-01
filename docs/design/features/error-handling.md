# Error handling

Phoenix has no `throw` or `catch`. Functions that can fail return `Result<T, E>`. Absence of a value uses `Option<T>`. See [type-system.md](type-system.md#built-in-generic-types).

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

## The `?` operator

`?` is part of **runtime transparency**: every call site that can fail shows propagation explicitly. See [runtime-transparency.md](runtime-transparency.md).

Inside a function returning `Result`, `?` propagates errors early.

```
read_config :: (path: [u8]) => Result<Config, Error>
{
  const text = std::fs::read(path)?;   // returns Err early if read fails
  parse(text)?
};
```

Inside `Option`-returning functions, `?` propagates `None`.

Desugaring: `expr?` in a `Result` function becomes "if `expr` is `Err(e)`, return `Err(e)`; otherwise unwrap `Ok` value." See [type-system.md](type-system.md#syntactic-sugar).
