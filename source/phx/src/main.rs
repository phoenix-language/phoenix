//! `phx` command-line driver.

#![allow(clippy::print_stderr, clippy::single_match_else)]

use std::panic::{self, AssertUnwindSafe};
use std::process;

use phx_cli::exit::CliExit;

const INTERNAL_ERROR_MSG: &str = "internal compiler error: please report with a minimal repro";

fn main() {
    let prev_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));

    let code = match panic::catch_unwind(AssertUnwindSafe(phx_cli::run)) {
        Ok(exit) => exit,
        Err(_) => {
            eprintln!("{INTERNAL_ERROR_MSG}");
            CliExit::Internal
        }
    };

    panic::set_hook(prev_hook);
    process::exit(code.as_i32());
}
