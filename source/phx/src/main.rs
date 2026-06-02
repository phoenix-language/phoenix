//! `phx` command-line driver.
//!
//! Subcommands:
//! - `check <file>` — parse, resolve, and type-check a Phoenix source file.

use std::env;
use std::path::Path;
use std::process;

use phx_compiler::check_file;

fn main() {
    let mut args = env::args().skip(1);
    let Some(cmd) = args.next() else {
        eprintln!("usage: phx check <file.phx>");
        process::exit(1);
    };
    if cmd != "check" {
        eprintln!("unknown command: {cmd} (supported: check)");
        process::exit(1);
    }
    let Some(path) = args.next() else {
        eprintln!("usage: phx check <file.phx>");
        process::exit(1);
    };
    if args.next().is_some() {
        eprintln!("usage: phx check <file.phx>");
        process::exit(1);
    }

    match check_file(Path::new(&path)) {
        Ok(unit) => {
            dbg!(unit);
        }
        Err(e) => {
            eprintln!("{e}");
            process::exit(1);
        }
    }
}
