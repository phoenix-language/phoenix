//! `phx check` handler.

use std::fs;

use phx_compiler::{check_project_file, check_standalone_with_context};

use crate::args::FileCommandArgs;
use crate::color::ColorChoice;
use crate::exit::CliExit;
use crate::report::Reporter;
use crate::workflow::{CompileMode, resolve_check_mode};

/// Runs `phx check`.
pub fn run_check(file_args: FileCommandArgs, color: ColorChoice, verbose: bool) -> CliExit {
    let style = crate::color::diagnostic_style(color);
    let reporter = Reporter::new(&style);

    let file = match &file_args.file {
        Some(f) => f.clone(),
        None => {
            reporter.usage_error("missing required argument <file.phx>");
            return CliExit::Usage;
        }
    };

    let mode = match resolve_check_mode(&file, &file_args, None) {
        Ok(m) => m,
        Err(e) => {
            reporter.usage_error(&e.to_string());
            return CliExit::Usage;
        }
    };

    reporter.verbose(verbose, &format!("checking {}", file.display()));

    let source = match fs::read_to_string(&file) {
        Ok(s) => s,
        Err(e) => {
            reporter.io_error(&e.to_string());
            return CliExit::Io;
        }
    };

    let result = match mode {
        CompileMode::Project { config } => check_project_file(&file, &config).map(|_| ()),
        CompileMode::Standalone { options } => {
            let ctx = match options.load_context() {
                Ok(c) => c,
                Err(e) => {
                    reporter.project_error(&e.to_string());
                    return CliExit::Usage;
                }
            };
            check_standalone_with_context(&options, &ctx)
        }
    };

    if let Err(e) = result {
        reporter.compile_error(&e, Some(&source));
        return CliExit::Compile;
    }
    CliExit::Ok
}
