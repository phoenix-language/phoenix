//! `phx build` handler.

use std::path::Path;

use phx_compiler::{BuildOptions, build_project};

use crate::args::ProjectCommandArgs;
use crate::color::ColorChoice;
use crate::exit::CliExit;
use crate::lints::emit_lint_warnings;
use crate::report::Reporter;
use crate::workflow::resolve_build_project;

/// Runs `phx build`.
pub fn run_build(args: ProjectCommandArgs, color: ColorChoice, verbose: bool) -> CliExit {
    let style = crate::color::diagnostic_style(color);
    let reporter = Reporter::new(&style);

    let anchor = args.entry.as_deref().unwrap_or_else(|| Path::new("."));
    let config = match resolve_build_project(anchor, args.project_root.as_deref()) {
        Ok(c) => c,
        Err(e) => {
            reporter.project_error(&e.to_string());
            return CliExit::Usage;
        }
    };

    reporter.verbose(verbose, "building project...");
    let options = BuildOptions {
        force: args.force_build,
        emit_interface_only: args.emit_interface_only,
    };
    match build_project(&config, args.entry.as_deref(), options) {
        Ok(result) => {
            if let Some(ctx) = &result.lint_context {
                emit_lint_warnings(&result.lints, ctx, &style);
            }
            if args.emit_interface_only {
                reporter.success(&format!(
                    "wrote interfaces to {}",
                    result.output_path.display()
                ));
            } else {
                reporter.success(&format!("built {}", result.output_path.display()));
            }
            CliExit::Ok
        }
        Err(e) => {
            reporter.build_error(&e);
            CliExit::Compile
        }
    }
}
