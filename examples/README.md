# Phoenix demonstration programs

Small, contributor-facing examples for Language v0 (V0-052). Each `main.phx` begins with a one-line README comment.

MVP has no standard I/O. Use `phx run --dump-main` to print `main` local slots to stderr after a successful run (documented VM debug channel).

## hello

`str` literal and byte indexing.

```bash
just phx run examples/hello/src/main.phx --dump-main
```

## modules

Binary package with a path dependency on a library (`math`).

```bash
just phx build --project-root examples/modules/app
just phx run --no-build --project-root examples/modules/app
```

## generics

Generic enum, `match`, and generic function monomorphization. Trait-bound monomorphization with bundled `std` is also shown in [tests/cli/fixtures/std_traits/](../tests/cli/fixtures/std_traits/).

```bash
just phx build --project-root examples/generics
just phx run --no-build --project-root examples/generics
```

## errors

`Result`, `match`, `?`, and `From` conversion across `std::error` types.

```bash
just phx build --project-root examples/errors
just phx run --no-build --project-root examples/errors
```

See [docs/contributing.md](../docs/contributing.md) for the full contributor guide.
