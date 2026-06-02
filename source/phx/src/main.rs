//! `phx` command-line driver.
//!
//! Subcommands:
//! - `help` — usage
//! - `check <file>` — parse, resolve, and type-check
//! - `compile <file> -o <out>` — emit verified PHX0 bytecode
//! - `run <file>` — compile, verify bytecode, execute `main`

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use phx_bytecode::verify;
use phx_compiler::{CompileError, check_file, compile_to_module};

fn print_usage() {
    eprintln!(
        "phx — Phoenix compiler (v0.1.0)\n\
         \n\
         usage:\n\
           phx help\n\
           phx check <file.phx>              type-check only\n\
           phx compile <file.phx> -o <out>   emit PHX0 bytecode\n\
           phx run <file.phx>                compile, verify bytecode, execute main"
    );
}

fn read_source(path: &Path) -> Result<String, CompileError> {
    fs::read_to_string(path).map_err(CompileError::Io)
}

fn report_compile_error(err: &CompileError, source: Option<&str>) {
    eprintln!("{}", err.format_with_source(source));
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
            let path = Path::new(&path);
            let source = read_source(path).ok();
            match check_file(path) {
                Ok(_unit) => {}
                Err(e) => {
                    report_compile_error(&e, source.as_deref());
                    process::exit(1);
                }
            }
        }
        "compile" => {
            let Some(path) = args.next() else {
                print_usage();
                process::exit(1);
            };
            let mut out_path: Option<PathBuf> = None;
            while let Some(arg) = args.next() {
                if arg == "-o" {
                    out_path = args.next().map(PathBuf::from);
                } else {
                    print_usage();
                    process::exit(1);
                }
            }
            let Some(out) = out_path else {
                eprintln!("compile requires -o <output.phx0>");
                process::exit(1);
            };
            let path = Path::new(&path);
            let source = read_source(path).ok();
            match compile_to_module(path) {
                Ok(module) => {
                    if let Err(e) = verify(&module) {
                        eprintln!("verify error: {e}");
                        process::exit(1);
                    }
                    let bytes = module.encode();
                    if let Err(e) = fs::write(&out, bytes) {
                        eprintln!("I/O error: {e}");
                        process::exit(1);
                    }
                }
                Err(e) => {
                    report_compile_error(&e, source.as_deref());
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
            let source = read_source(path).ok();
            match compile_to_module(path) {
                Ok(module) => {
                    if let Err(e) = verify(&module) {
                        eprintln!("verify error: {e}");
                        process::exit(1);
                    }
                    if let Err(e) = phx_vm::run(&module) {
                        eprintln!("runtime error: {e}");
                        process::exit(1);
                    }
                }
                Err(e) => {
                    report_compile_error(&e, source.as_deref());
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
