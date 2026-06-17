//! Usage and help text.

use crate::args::SubcommandName;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Prints top-level usage.
pub fn print_usage() {
    eprintln!(
        "phx {VERSION} — Phoenix compiler\n\
         \n\
         usage:\n\
           phx [--color auto|always|never] [-v] <command> [options]\n\
         \n\
         commands:\n\
           help [command]       Show usage (or help for one command)\n\
           version              Print version\n\
           explain <code>       Explain a diagnostic code (e.g. E2001)\n\
           check <file.phx>     Type-check a source file\n\
           build [entry.phx]    Build a phoenix.toml project\n\
           compile <file.phx>   Compile a file to .phx0 (-o required)\n\
           run [file.phx]       Compile and run on the VM\n\
         \n\
         Run `phx help <command>` for command-specific options.\n\
         \n\
         Without phoenix.toml, check/run require an explicit file; use --module-src and --dep\n\
         to link modules. Inside a project directory, project rules apply (main.phx, module_src)."
    );
}

/// Prints help for a specific subcommand.
pub fn print_command_help(sub: SubcommandName) {
    match sub {
        SubcommandName::Check => eprintln!(
            "phx check — type-check a Phoenix source file\n\
             \n\
             usage:\n\
               phx check [--module-src <dir>] [--package-name <name>] [--dep name=path] <file.phx>\n\
             \n\
             options:\n\
               --module-src <dir>         Module root for #import resolution\n\
               --package-name <name>      Package name override (default: parent directory name)\n\
               --dep name=path            Path dependency for cross-package imports (repeatable)\n\
               --emit-interface-only      Write build/pxi and manifest only (project mode)\n\
               --deny[=deprecated,...]    Fail on lint warnings (overrides phoenix.toml [lint] deny)"
        ),
        SubcommandName::Build => eprintln!(
            "phx build — build a phoenix.toml project\n\
             \n\
             usage:\n\
               phx build [--project-root <dir>] [--build] [--emit-interface-only] [entry.phx]\n\
             \n\
             options:\n\
               --project-root <dir>       Project root containing phoenix.toml\n\
               --build                    Force a full rebuild\n\
               --emit-interface-only      Write build/pxi and manifest only; skip link\n\
               --deny[=deprecated,...]    Fail on lint warnings (overrides phoenix.toml [lint] deny)"
        ),
        SubcommandName::Compile => eprintln!(
            "phx compile — compile a file to PHX0 bytecode\n\
             \n\
             usage:\n\
               phx compile [--module-src <dir>] [--package-name <name>] [--dep name=path] <file.phx> -o <out.phx0>\n\
             \n\
             options:\n\
               -o <path>             Output .phx0 path (required)\n\
               --module-src <dir>    Module root for #import resolution\n\
               --package-name <name> Package name override\n\
               --dep name=path       Path dependency (repeatable)\n\
               --deny[=deprecated,...] Fail on lint warnings"
        ),
        SubcommandName::Run => eprintln!(
            "phx run — compile and execute on the VM\n\
             \n\
             usage:\n\
               phx run [--project-root <dir>] [--build] [--no-build] [entry.phx]\n\
               phx run [--module-src <dir>] [--package-name <name>] [--dep name=path] <file.phx>\n\
             \n\
             options:\n\
               --project-root <dir>  Project root containing phoenix.toml\n\
               --module-src <dir>    Module root for standalone multi-file programs\n\
               --package-name <name> Package name override for standalone mode\n\
               --dep name=path       Path dependency for standalone mode (repeatable)\n\
               --build               Force rebuild (project mode)\n\
               --no-build            Skip build and load existing artifact (project mode)\n\
               --heap-cap <size>     VM heap byte cap (e.g. 64mb, 1gb); overrides phoenix.toml\n\
               --deny[=deprecated,...] Fail on lint warnings (overrides phoenix.toml [lint] deny)\n\
               --dump-main           Print `main` local slots to stderr after run (MVP debug channel)\n\
             \n\
             In a project directory, `phx run` without a file runs the project entry.\n\
             Passing a non-entry file in a project directory is an error."
        ),
        SubcommandName::Explain => eprintln!(
            "phx explain — show a short explanation for a diagnostic code\n\
             \n\
             usage:\n\
               phx explain <code>\n\
             \n\
             example:\n\
               phx explain E2001"
        ),
    }
}

/// Prints the package version.
pub fn print_version() {
    eprintln!("phx {VERSION}");
}
