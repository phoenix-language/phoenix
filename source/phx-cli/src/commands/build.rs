//! `phx build` handler.

use std::path::Path;

use phx_compiler::build_project;

use crate::args::ProjectCommandArgs;
use crate::color::ColorChoice;
use crate::exit::CliExit;
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
    match build_project(&config, args.entry.as_deref(), args.force_build) {
        Ok(result) => {
            reporter.success(&format!("built {}", result.output_path.display()));
            CliExit::Ok
        }
        Err(e) => {
            reporter.build_error(&e);
            CliExit::Compile
        }
    }
}
