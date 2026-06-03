//! `phx` command-line driver.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use phx_bytecode::verify;
use phx_compiler::{
    CompileError, build_project, check_file_with_module_path,
    compile_to_module_with_module_path, resolve_project, load_project_binary,
};

fn print_usage() {
    eprintln!(
        "phx — Phoenix compiler (v0.1.0)\n\
         \n\
         usage:\n\
           phx help\n\
           phx check [--module-src <dir>] <file.phx>\n\
           phx build [--project-root <dir>] [--build] [entry.phx]\n\
           phx compile [--module-src <dir>] <file.phx> -o <out>\n\
           phx run [--project-root <dir>] [--no-build] [--build] [entry.phx]\n\
         \n\
         `build` and `run` require phoenix.toml at the project root."
    );
}

fn read_source(path: &Path) -> Result<String, CompileError> {
    fs::read_to_string(path).map_err(CompileError::Io)
}

fn report_compile_error(err: &CompileError, entry_source: Option<&str>) {
    eprintln!("{}", err.format_with_source(entry_source));
}

struct ProjectArgs {
    entry: Option<PathBuf>,
    project_root: Option<PathBuf>,
    force_build: bool,
    skip_build: bool,
}

fn parse_project_command(mut args: impl Iterator<Item = String>) -> Result<ProjectArgs, ()> {
    let mut project_root = None;
    let mut force_build = false;
    let mut skip_build = false;
    let mut entry = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--project-root" => project_root = Some(PathBuf::from(args.next().ok_or(())?)),
            "--build" => force_build = true,
            "--no-build" => skip_build = true,
            _ if entry.is_some() => return Err(()),
            _ => entry = Some(PathBuf::from(arg)),
        }
    }
    Ok(ProjectArgs {
        entry,
        project_root,
        force_build,
        skip_build,
    })
}

fn parse_file_command(
    mut args: impl Iterator<Item = String>,
) -> Result<(PathBuf, PathBuf), ()> {
    let mut module_src = None;
    let mut file = None;
    while let Some(arg) = args.next() {
        if arg == "--module-src" {
            module_src = Some(PathBuf::from(args.next().ok_or(())?));
        } else if file.is_some() {
            return Err(());
        } else {
            file = Some(PathBuf::from(arg));
        }
    }
    let path = file.ok_or(())?;
    let module_root = module_src.unwrap_or_else(|| {
        path.parent()
            .unwrap_or(Path::new("."))
            .to_path_buf()
    });
    Ok((path, module_root))
}

fn run_with_project(
    entry: Option<&Path>,
    project_root: Option<&Path>,
    force: bool,
    skip_build: bool,
) {
    let anchor = entry.unwrap_or_else(|| Path::new("."));
    let config = match resolve_project(anchor, project_root) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            process::exit(1);
        }
    };
    if !skip_build {
        if let Err(e) = build_project(&config, entry, force) {
            eprintln!("{}", e.to_message());
            process::exit(1);
        }
    }
    match load_project_binary(&config) {
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
            eprintln!("{}", e.to_message());
            process::exit(1);
        }
    }
}

fn main() {
    let mut args = env::args().skip(1);
    let Some(cmd) = args.next() else {
        print_usage();
        process::exit(1);
    };

    match cmd.as_str() {
        "help" => print_usage(),
        "check" => {
            let Ok((path, module_root)) = parse_file_command(args) else {
                print_usage();
                process::exit(1);
            };
            let source = read_source(&path).ok();
            if let Err(e) = check_file_with_module_path(&path, &module_root) {
                report_compile_error(&e, source.as_deref());
                process::exit(1);
            }
        }
        "build" => {
            let Ok(pa) = parse_project_command(args) else {
                print_usage();
                process::exit(1);
            };
            let anchor = pa.entry.as_deref().unwrap_or_else(|| Path::new("."));
            let config = match resolve_project(anchor, pa.project_root.as_deref()) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("{e}");
                    process::exit(1);
                }
            };
            match build_project(&config, pa.entry.as_deref(), pa.force_build) {
                Ok(result) => eprintln!("built {}", result.output_path.display()),
                Err(e) => {
                    eprintln!("{}", e.to_message());
                    process::exit(1);
                }
            }
        }
        "compile" => {
            let rest: Vec<String> = args.collect();
            let mut out_path = None;
            let mut path = None;
            let mut module_root = None;
            let mut i = 0;
            while i < rest.len() {
                match rest[i].as_str() {
                    "-o" => {
                        i += 1;
                        out_path = Some(PathBuf::from(rest.get(i).ok_or(()).unwrap_or(&rest[0])));
                    }
                    "--module-src" => {
                        i += 1;
                        module_root = Some(PathBuf::from(rest.get(i).ok_or(()).unwrap_or(&rest[0])));
                        i += 1;
                        if i < rest.len() && path.is_none() {
                            path = Some(PathBuf::from(&rest[i]));
                        }
                    }
                    _ if path.is_none() => path = Some(PathBuf::from(&rest[i])),
                    _ => {}
                }
                i += 1;
            }
            let path = match path {
                Some(p) => p,
                None => {
                    print_usage();
                    process::exit(1);
                }
            };
            let out = match out_path {
                Some(o) => o,
                None => {
                    eprintln!("compile requires -o <output.phx0>");
                    process::exit(1);
                }
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
                    if let Err(e) = fs::write(&out, module.encode()) {
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
            let Ok(pa) = parse_project_command(args) else {
                print_usage();
                process::exit(1);
            };
            let anchor = pa.entry.as_deref().unwrap_or(Path::new("."));
            if resolve_project(anchor, pa.project_root.as_deref()).is_ok() {
                run_with_project(
                    pa.entry.as_deref(),
                    pa.project_root.as_deref(),
                    pa.force_build,
                    pa.skip_build,
                );
            } else {
                let Some(entry) = pa.entry.as_ref() else {
                    print_usage();
                    process::exit(1);
                };
                let module_root = entry
                    .parent()
                    .unwrap_or(Path::new("."))
                    .to_path_buf();
                let source = read_source(entry).ok();
                match compile_to_module_with_module_path(entry, &module_root) {
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
        }
        _ => {
            eprintln!("unknown command: {cmd}");
            print_usage();
            process::exit(1);
        }
    }
}
