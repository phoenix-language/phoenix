//! `phx` command-line driver.
#![allow(
    clippy::print_stderr,
    clippy::manual_let_else,
    clippy::single_match_else,
    clippy::too_many_lines,
    clippy::collapsible_if
)]

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use phx_bytecode::verify;
use phx_compiler::{
    CompileError, build_project, check_file_with_module_path, check_project_file,
    compile_to_module_with_module_path, discover_project, load_project_binary, resolve_project,
};

fn print_usage() {
    eprintln!(
        "phx — Phoenix compiler (Alpha-Experimental-Build-0.1.0)\n\
         \n\
         usage:\n\
           phx help\n\
           phx check [--module-src <dir>] <file.phx>\n\
           phx build [--project-root <dir>] [--build] [entry.phx]\n\
           phx compile [--module-src <dir>] <file.phx> -o <out>\n\
           phx run [--module-src <dir>] [--project-root <dir>] [--no-build] [--build] [entry.phx]\n\
         \n\
         `build` and `run` require phoenix.toml at the project root.\n\
         Single-file check/run use the file's parent as module root; #import needs --module-src or a project."
    );
}

fn read_source(path: &Path) -> std::io::Result<String> {
    fs::read_to_string(path)
}

fn report_compile_error(err: &CompileError, entry_source: Option<&str>) {
    let (modules, interner) = match err {
        CompileError::Resolve { context, .. } => (
            context.as_ref().map(|c| c.modules.as_slice()),
            context.as_ref().map(|c| &c.interner),
        ),
        CompileError::TypeCheck { context, .. } => {
            (Some(context.modules.as_slice()), Some(&context.interner))
        }
        _ => (None, None),
    };
    eprintln!(
        "{}",
        err.format_with_modules(entry_source, modules, interner)
    );
}

struct ProjectArgs {
    entry: Option<PathBuf>,
    project_root: Option<PathBuf>,
    force_build: bool,
}

struct RunArgs {
    entry: Option<PathBuf>,
    project_root: Option<PathBuf>,
    module_src: Option<PathBuf>,
    force_build: bool,
    skip_build: bool,
}

fn parse_project_command(mut args: impl Iterator<Item = String>) -> Result<ProjectArgs, ()> {
    let mut project_root = None;
    let mut force_build = false;
    let mut entry = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--project-root" => project_root = Some(PathBuf::from(args.next().ok_or(())?)),
            "--build" => force_build = true,
            "--no-build" | "--module-src" => return Err(()),
            _ if entry.is_some() => return Err(()),
            _ => entry = Some(PathBuf::from(arg)),
        }
    }
    Ok(ProjectArgs {
        entry,
        project_root,
        force_build,
    })
}

fn parse_run_command(mut args: impl Iterator<Item = String>) -> Result<RunArgs, ()> {
    let mut project_root = None;
    let mut module_src = None;
    let mut force_build = false;
    let mut skip_build = false;
    let mut entry = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--project-root" => project_root = Some(PathBuf::from(args.next().ok_or(())?)),
            "--module-src" => module_src = Some(PathBuf::from(args.next().ok_or(())?)),
            "--build" => force_build = true,
            "--no-build" => skip_build = true,
            _ if entry.is_some() => return Err(()),
            _ => entry = Some(PathBuf::from(arg)),
        }
    }
    Ok(RunArgs {
        entry,
        project_root,
        module_src,
        force_build,
        skip_build,
    })
}

fn run_single_file(entry: &Path, module_root: &Path) {
    let source = read_source(entry).ok();
    match compile_to_module_with_module_path(entry, module_root) {
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

fn parse_file_command(
    mut args: impl Iterator<Item = String>,
) -> Result<(PathBuf, PathBuf, bool), ()> {
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
    let used_module_src_flag = module_src.is_some();
    let module_root =
        module_src.unwrap_or_else(|| path.parent().unwrap_or(Path::new(".")).to_path_buf());
    Ok((path, module_root, used_module_src_flag))
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
            let Ok((path, module_root, used_module_src_flag)) = parse_file_command(args) else {
                print_usage();
                process::exit(1);
            };
            let source = read_source(&path).ok();
            let result = if used_module_src_flag {
                check_file_with_module_path(&path, &module_root)
            } else if let Ok(config) = discover_project(&path) {
                check_project_file(&path, &config)
            } else {
                check_file_with_module_path(&path, &module_root)
            };
            if let Err(e) = result {
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
                        module_root =
                            Some(PathBuf::from(rest.get(i).ok_or(()).unwrap_or(&rest[0])));
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
            let module_root = module_root
                .unwrap_or_else(|| path.parent().unwrap_or(Path::new(".")).to_path_buf());
            let source = read_source(&path).ok();
            match compile_to_module_with_module_path(&path, &module_root) {
                Ok(module) => {
                    if let Err(e) = verify(&module) {
                        eprintln!("verify error: {e}");
                        process::exit(1);
                    }
                    let bytes = match module.encode() {
                        Ok(b) => b,
                        Err(e) => {
                            eprintln!("encode error: {e}");
                            process::exit(1);
                        }
                    };
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
            let Ok(ra) = parse_run_command(args) else {
                print_usage();
                process::exit(1);
            };
            if let Some(module_root) = ra.module_src {
                let Some(entry) = ra.entry.as_ref() else {
                    print_usage();
                    process::exit(1);
                };
                run_single_file(entry, &module_root);
            } else {
                let anchor = ra.entry.as_deref().unwrap_or(Path::new("."));
                if resolve_project(anchor, ra.project_root.as_deref()).is_ok() {
                    run_with_project(
                        ra.entry.as_deref(),
                        ra.project_root.as_deref(),
                        ra.force_build,
                        ra.skip_build,
                    );
                } else {
                    let Some(entry) = ra.entry.as_ref() else {
                        print_usage();
                        process::exit(1);
                    };
                    let module_root = entry.parent().unwrap_or(Path::new(".")).to_path_buf();
                    run_single_file(entry, &module_root);
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
