# phx

Phoenix command-line interface (`phx` binary): check, build, compile, and run Phoenix programs.

## Commands

| Command | Description |
|---------|-------------|
| `phx check <file.phx>` | Type-check a source file |
| `phx build` | Build a `phoenix.toml` project |
| `phx compile <file.phx> -o <out.phx0>` | Compile a single file to bytecode |
| `phx run [file.phx]` | Compile and execute on the VM |
| `phx explain <code>` | Explain a diagnostic code (e.g. `E2001`) |
| `phx help [command]` | Show usage |

Global flags: `--color auto|always|never`, `-v` / `--verbose`, `--version`.

## Standalone vs project

Without `phoenix.toml`, `check` and `run` require an explicit file. Use `--module-src` for multi-file `#import` graphs and `--dep name=path` for path dependencies.

When `phoenix.toml` is discovered, project rules apply: `phx run` without a file runs the project entry; passing a non-entry file is an error.

Implementation lives in the [`phx-cli`](../phx-cli/) library crate; this directory contains only the thin binary entry point.
