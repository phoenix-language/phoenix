//! `phx` command-line driver.
//!
//! Subcommands:
//! - `help` — usage
//! - `check <file>` — parse, resolve, and type-check
//! - `run <file>` — compile, verify bytecode, execute `main`

use std::env;
use std::path::Path;
use std::process;

use phx_bytecode::verify;
use phx_compiler::{check_file, compile_to_module};
use phx_vm::run;

fn print_usage() {
    eprintln!(
        "phx — Phoenix compiler (v0.1.0)\n\
         \n\
         usage:\n\
           phx help\n\
           phx check <file.phx>   type-check only\n\
           phx run <file.phx>     compile, verify bytecode, execute main"
    );
}

fn main() {
    let mut args = env::args().skip(1);
    let Some(cmd) = args.next() else {
        print_usage();
        process::exit(1);
    };

    match cmd.as_str() {
        "help" => {
            if args.next().is_some() {
                print_usage();
                process::exit(1);
            }
            print_usage();
        }
        "check" => {
            let Some(path) = args.next() else {
                print_usage();
                process::exit(1);
            };
            if args.next().is_some() {
                print_usage();
                process::exit(1);
            }
            match check_file(Path::new(&path)) {
                Ok(_unit) => {}
                Err(e) => {
                    eprintln!("{e}");
                    process::exit(1);
                }
            }
        }
        "run" => {
            let Some(path) = args.next() else {
                print_usage();
                process::exit(1);
            };
            if args.next().is_some() {
                print_usage();
                process::exit(1);
            }
            let path = Path::new(&path);
            match compile_to_module(path) {
                Ok(module) => {
                    if let Err(e) = verify(&module) {
                        eprintln!("verify error: {e}");
                        process::exit(1);
                    }
                    if let Err(e) = run(&module) {
                        eprintln!("runtime error: {e}");
                        process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("{e}");
                    process::exit(1);
                }
            }
        }
        _ => {
            eprintln!("unknown command: {cmd}");
            print_usage();
            process::exit(1);
        }
    }
}
