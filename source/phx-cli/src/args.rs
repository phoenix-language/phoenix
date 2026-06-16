//! Argument parsing for the `phx` CLI.

use std::env;
use std::path::PathBuf;

use crate::color::ColorChoice;
use crate::exit::CliExit;

/// Global CLI options parsed before the subcommand.
#[derive(Debug, Clone)]
pub struct CliOptions {
    /// Color output preference.
    pub color: ColorChoice,
    /// Print pipeline status messages.
    pub verbose: bool,
}

impl Default for CliOptions {
    fn default() -> Self {
        Self {
            color: ColorChoice::Auto,
            verbose: false,
        }
    }
}

/// Parsed subcommand.
#[derive(Debug, Clone)]
pub enum Command {
    /// Print usage.
    Help(Option<SubcommandName>),
    /// Print version.
    Version,
    /// Explain a diagnostic code.
    Explain(String),
    /// Type-check only.
    Check(FileCommandArgs),
    /// Build a project.
    Build(ProjectCommandArgs),
    /// Compile to `.phx0`.
    Compile(CompileCommandArgs),
    /// Compile and run.
    Run(RunCommandArgs),
}

/// Subcommand names for targeted help.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubcommandName {
    /// `phx check`
    Check,
    /// `phx build`
    Build,
    /// `phx compile`
    Compile,
    /// `phx run`
    Run,
    /// `phx explain`
    Explain,
}

impl SubcommandName {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "check" => Some(Self::Check),
            "build" => Some(Self::Build),
            "compile" => Some(Self::Compile),
            "run" => Some(Self::Run),
            "explain" => Some(Self::Explain),
            _ => None,
        }
    }
}

/// Shared flags for file-based commands.
#[derive(Debug, Clone, Default)]
pub struct FileCommandArgs {
    /// Entry `.phx` file.
    pub file: Option<PathBuf>,
    /// Module root for `#import`.
    pub module_src: Option<PathBuf>,
    /// Path dependencies (`name=path`).
    pub deps: Vec<(String, PathBuf)>,
    /// Workspace package name override.
    pub package_name: Option<String>,
    /// Emit `.pxi` interfaces and manifest only (project check mode).
    pub emit_interface_only: bool,
}

/// `phx build` arguments.
#[derive(Debug, Clone, Default)]
pub struct ProjectCommandArgs {
    /// Optional entry override.
    pub entry: Option<PathBuf>,
    /// Project root directory.
    pub project_root: Option<PathBuf>,
    /// Force rebuild.
    pub force_build: bool,
    /// Emit `.pxi` interfaces and manifest only.
    pub emit_interface_only: bool,
}

/// `phx compile` arguments.
#[derive(Debug, Clone)]
pub struct CompileCommandArgs {
    /// Shared file flags.
    pub file_args: FileCommandArgs,
    /// Output `.phx0` path.
    pub output: Option<PathBuf>,
}

/// `phx run` arguments.
#[derive(Debug, Clone, Default)]
pub struct RunCommandArgs {
    /// Shared file flags.
    pub file_args: FileCommandArgs,
    /// Project root directory.
    pub project_root: Option<PathBuf>,
    /// Force rebuild in project mode.
    pub force_build: bool,
    /// Skip build in project mode.
    pub skip_build: bool,
    /// After run, print `main` local slots to stderr (MVP debug channel).
    pub dump_main: bool,
    /// VM linear heap byte cap override (`--heap-cap`).
    pub heap_cap: Option<usize>,
}

/// Argument parse failure with a user-facing message.
#[derive(Debug, Clone)]
pub struct ParseError {
    /// Error text shown to the user.
    pub message: String,
}

impl ParseError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Parses `std::env::args()` into global options and a subcommand.
///
/// # Errors
///
/// Returns [`ParseError`] on invalid flags or missing required arguments.
pub fn parse_args(args: impl Iterator<Item = String>) -> Result<(CliOptions, Command), ParseError> {
    let mut iter = args.peekable();
    let mut opts = CliOptions::default();

    while let Some(arg) = iter.peek() {
        match arg.as_str() {
            "--color" => {
                iter.next();
                let value = iter
                    .next()
                    .ok_or_else(|| ParseError::new("missing value for --color"))?;
                opts.color = ColorChoice::parse(&value).map_err(ParseError::new)?;
            }
            "--verbose" | "-v" => {
                iter.next();
                opts.verbose = true;
            }
            "--version" | "-V" => {
                iter.next();
                return Ok((opts, Command::Version));
            }
            "--help" | "-h" => {
                iter.next();
                let sub = iter.next().and_then(|s| SubcommandName::parse(&s));
                return Ok((opts, Command::Help(sub)));
            }
            s if s.starts_with('-') => {
                return Err(ParseError::new(format!("unexpected argument '{s}'")));
            }
            _ => break,
        }
    }

    let cmd = iter
        .next()
        .ok_or_else(|| ParseError::new("missing subcommand (try `phx help`)"))?;
    if cmd == "help" {
        let sub = iter.next().and_then(|s| SubcommandName::parse(&s));
        return Ok((opts, Command::Help(sub)));
    }
    if cmd == "--help" || cmd == "-h" {
        return Ok((opts, Command::Help(None)));
    }

    if matches!(
        cmd.as_str(),
        "check" | "build" | "compile" | "run" | "explain"
    ) && let Some(flag) = iter.peek()
        && (flag == "--help" || flag == "-h")
    {
        iter.next();
        if let Some(sub) = SubcommandName::parse(&cmd) {
            return Ok((opts, Command::Help(Some(sub))));
        }
    }

    let command = match cmd.as_str() {
        "version" => Command::Version,
        "explain" => {
            let code = iter
                .next()
                .ok_or_else(|| ParseError::new("missing diagnostic code (e.g. E2001)"))?;
            if iter.next().is_some() {
                return Err(ParseError::new("unexpected extra arguments to `explain`"));
            }
            Command::Explain(code)
        }
        "check" => {
            let (file_args, _) = parse_file_flags(&mut iter, false)?;
            if iter.next().is_some() {
                return Err(ParseError::new("unexpected extra arguments to `check`"));
            }
            Command::Check(file_args)
        }
        "build" => {
            let args = parse_project_flags(&mut iter)?;
            if iter.next().is_some() {
                return Err(ParseError::new("unexpected extra arguments to `build`"));
            }
            Command::Build(args)
        }
        "compile" => {
            let (file_args, output) = parse_file_flags(&mut iter, true)?;
            if iter.next().is_some() {
                return Err(ParseError::new("unexpected extra arguments to `compile`"));
            }
            Command::Compile(CompileCommandArgs { file_args, output })
        }
        "run" => {
            let args = parse_run_flags(&mut iter)?;
            if iter.next().is_some() {
                return Err(ParseError::new("unexpected extra arguments to `run`"));
            }
            Command::Run(args)
        }
        other => {
            if other.starts_with('-') {
                return Err(ParseError::new(format!("unexpected argument '{other}'")));
            }
            return Err(ParseError::new(format!("unknown command '{other}'")));
        }
    };

    Ok((opts, command))
}

fn parse_file_flags(
    iter: &mut impl Iterator<Item = String>,
    allow_output: bool,
) -> Result<(FileCommandArgs, Option<PathBuf>), ParseError> {
    let mut args = FileCommandArgs::default();
    let mut output = None;
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--module-src" => {
                let path = iter
                    .next()
                    .ok_or_else(|| ParseError::new("missing value for --module-src"))?;
                args.module_src = Some(PathBuf::from(path));
            }
            "--package-name" => {
                let name = iter
                    .next()
                    .ok_or_else(|| ParseError::new("missing value for --package-name"))?;
                args.package_name = Some(name);
            }
            "--dep" => {
                let spec = iter.next().ok_or_else(|| {
                    ParseError::new("missing value for --dep (expected name=path)")
                })?;
                let (name, path) = parse_dep_spec(&spec)?;
                args.deps.push((name, path));
            }
            "--emit-interface-only" => args.emit_interface_only = true,
            "-o" if allow_output => {
                let path = iter
                    .next()
                    .ok_or_else(|| ParseError::new("missing value for -o"))?;
                output = Some(PathBuf::from(path));
            }
            s if s.starts_with("--") => {
                return Err(ParseError::new(format!("unexpected argument '{s}'")));
            }
            _ if args.file.is_none() => args.file = Some(PathBuf::from(arg)),
            other => return Err(ParseError::new(format!("unexpected argument '{other}'"))),
        }
    }
    Ok((args, output))
}

fn parse_project_flags(
    iter: &mut impl Iterator<Item = String>,
) -> Result<ProjectCommandArgs, ParseError> {
    let mut args = ProjectCommandArgs::default();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--project-root" => {
                let path = iter
                    .next()
                    .ok_or_else(|| ParseError::new("missing value for --project-root"))?;
                args.project_root = Some(PathBuf::from(path));
            }
            "--build" => args.force_build = true,
            "--emit-interface-only" => args.emit_interface_only = true,
            s if s.starts_with("--") => {
                return Err(ParseError::new(format!("unexpected argument '{s}'")));
            }
            _ if args.entry.is_none() => args.entry = Some(PathBuf::from(arg)),
            other => return Err(ParseError::new(format!("unexpected argument '{other}'"))),
        }
    }
    Ok(args)
}

fn parse_run_flags(iter: &mut impl Iterator<Item = String>) -> Result<RunCommandArgs, ParseError> {
    let mut args = RunCommandArgs::default();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--project-root" => {
                let path = iter
                    .next()
                    .ok_or_else(|| ParseError::new("missing value for --project-root"))?;
                args.project_root = Some(PathBuf::from(path));
            }
            "--module-src" => {
                let path = iter
                    .next()
                    .ok_or_else(|| ParseError::new("missing value for --module-src"))?;
                args.file_args.module_src = Some(PathBuf::from(path));
            }
            "--package-name" => {
                let name = iter
                    .next()
                    .ok_or_else(|| ParseError::new("missing value for --package-name"))?;
                args.file_args.package_name = Some(name);
            }
            "--dep" => {
                let spec = iter.next().ok_or_else(|| {
                    ParseError::new("missing value for --dep (expected name=path)")
                })?;
                let (name, path) = parse_dep_spec(&spec)?;
                args.file_args.deps.push((name, path));
            }
            "--build" => args.force_build = true,
            "--no-build" => args.skip_build = true,
            "--dump-main" => args.dump_main = true,
            "--heap-cap" => {
                let value = iter
                    .next()
                    .ok_or_else(|| ParseError::new("missing value for --heap-cap"))?;
                args.heap_cap = Some(parse_heap_cap_arg(&value)?);
            }
            s if let Some(value) = s.strip_prefix("--heap-cap=") => {
                args.heap_cap = Some(parse_heap_cap_arg(value)?);
            }
            "--emit-interface-only" => {
                return Err(ParseError::new(
                    "`phx run` does not support --emit-interface-only (no runnable artifact)",
                ));
            }
            s if s.starts_with("--") => {
                return Err(ParseError::new(format!("unexpected argument '{s}'")));
            }
            _ if args.file_args.file.is_none() => {
                args.file_args.file = Some(PathBuf::from(arg));
            }
            other => return Err(ParseError::new(format!("unexpected argument '{other}'"))),
        }
    }
    Ok(args)
}

fn parse_dep_spec(spec: &str) -> Result<(String, PathBuf), ParseError> {
    let (name, path) = spec.split_once('=').ok_or_else(|| {
        ParseError::new(format!("invalid --dep value '{spec}' (expected name=path)"))
    })?;
    if name.is_empty() {
        return Err(ParseError::new(format!(
            "invalid --dep value '{spec}' (package name cannot be empty)"
        )));
    }
    Ok((name.to_owned(), PathBuf::from(path)))
}

fn parse_heap_cap_arg(value: &str) -> Result<usize, ParseError> {
    phx_compiler::parse_byte_size(value).map_err(|e| ParseError::new(format!("--heap-cap: {e}")))
}

/// Parses process arguments (skips the program name).
///
/// # Errors
///
/// Same as [`parse_args`].
pub fn parse_env_args() -> Result<(CliOptions, Command), ParseError> {
    parse_args(env::args().skip(1))
}

/// Maps a parse error to an exit code after printing help when appropriate.
pub fn handle_parse_error(err: ParseError, print_help: bool) -> CliExit {
    eprintln!("error: {}", err.message);
    if print_help {
        eprintln!();
        crate::help::print_usage();
    }
    CliExit::Usage
}
