//! `phx` command-line driver.

use std::process;

fn main() {
    let code = phx_cli::run();
    process::exit(code.as_i32());
}
