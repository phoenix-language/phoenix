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
use phx_compiler::{
    CompileError, check_file_with_module_path, compile_to_module_with_module_path,
};

fn print_usage() {
    eprintln!(
        "phx — Phoenix compiler (v0.1.0)\n\
         \n\
         usage:\n\
           phx help\n\
           phx check [--module-path <dir>] <file.phx>\n\
           phx compile [--module-path <dir>] <file.phx> -o <out>\n\
           phx run [--module-path <dir>] <file.phx>\n\
         \n\
         --module-path sets the root for #import resolution (default: entry file directory)"
    );
}

fn read_source(path: &Path) -> Result<String, CompileError> {
    fs::read_to_string(path).map_err(CompileError::Io)
}

fn report_compile_error(
    err: &CompileError,
    entry_source: Option<&str>,
    modules: Option<&[phx_compiler::SourceModule]>,
) {
    eprintln!("{}", err.format_with_modules(entry_source, modules));
}

/// Parses trailing args: optional `--module-path <dir>`, then exactly one file path.
fn parse_file_command(
    mut args: impl Iterator<Item = String>,
) -> Result<(PathBuf, PathBuf), ()> {
    let mut module_path: Option<PathBuf> = None;
    let mut file: Option<PathBuf> = None;
    while let Some(arg) = args.next() {
        if arg == "--module-path" {
            module_path = args.next().map(PathBuf::from);
            if module_path.is_none() {
                return Err(());
            }
        } else if file.is_some() {
            return Err(());
        } else {
            file = Some(PathBuf::from(arg));
        }
    }
    let Some(path) = file else {
        return Err(());
    };
    let module_root = module_path.unwrap_or_else(|| {
        path.parent()
            .unwrap_or(Path::new("."))
            .to_path_buf()
    });
    Ok((path, module_root))
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
            let Ok((path, module_root)) = parse_file_command(args) else {
                print_usage();
                process::exit(1);
            };
            let source = read_source(&path).ok();
            match check_file_with_module_path(&path, &module_root) {
                Ok(unit) => {
                    let _ = unit;
                }
                Err(e) => {
                    report_compile_error(&e, source.as_deref(), None);
                    process::exit(1);
                }
            }
        }
        "compile" => {
            let rest: Vec<String> = args.collect();
            let mut out_path: Option<PathBuf> = None;
            let mut path: Option<PathBuf> = None;
            let mut module_root: Option<PathBuf> = None;
            let mut i = 0;
            while i < rest.len() {
                match rest[i].as_str() {
                    "-o" => {
                        i += 1;
                        if i >= rest.len() {
                            print_usage();
                            process::exit(1);
                        }
                        out_path = Some(PathBuf::from(&rest[i]));
                    }
                    "--module-path" => {
                        i += 1;
                        if i >= rest.len() {
                            print_usage();
                            process::exit(1);
                        }
                        module_root = Some(PathBuf::from(&rest[i]));
                        i += 1;
                        if i >= rest.len() {
                            print_usage();
                            process::exit(1);
                        }
                        path = Some(PathBuf::from(&rest[i]));
                    }
                    _ => {
                        if path.is_some() {
                            print_usage();
                            process::exit(1);
                        }
                        path = Some(PathBuf::from(&rest[i]));
                    }
                }
                i += 1;
            }
            let Some(path) = path else {
                print_usage();
                process::exit(1);
            };
            let Some(out) = out_path else {
                eprintln!("compile requires -o <output.phx0>");
                process::exit(1);
            };
            let module_root = module_root.unwrap_or_else(|| {
                path.parent()
                    .unwrap_or(Path::new("."))
                    .to_path_buf()
            });
            let source = read_source(&path).ok();
            match compile_to_module_with_module_path(&path, &module_root) {
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
                    report_compile_error(&e, source.as_deref(), None);
                    process::exit(1);
                }
            }
        }
        "run" => {
            let Ok((path, module_root)) = parse_file_command(args) else {
                print_usage();
                process::exit(1);
            };
            let source = read_source(&path).ok();
            match compile_to_module_with_module_path(&path, &module_root) {
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
                    report_compile_error(&e, source.as_deref(), None);
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
