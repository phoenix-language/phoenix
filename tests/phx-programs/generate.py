#!/usr/bin/env python3
"""One-shot generator: embed CLI fixtures and diagnostic goldens into phx-programs Rust modules."""

from __future__ import annotations

import os
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
FIXTURES = ROOT / "tests" / "cli" / "fixtures"
DIAG = ROOT / "tests" / "integration" / "diagnostics"
OUT = Path(__file__).resolve().parent / "src"

SMOKE = [
    "sample.phx", "control_flow.phx", "continue_in_if.phx", "logical.phx",
    "match_int.phx", "match_ident.phx", "match_bool.phx", "struct_point.phx",
    "struct_assign.phx", "enum_match.phx", "enum_match_struct.phx", "struct_method.phx",
    "cast_width.phx", "compare_unary.phx", "deep_logical_chain.phx",
    "deep_logical_or_chain.phx", "mod_bitwise.phx", "array_index.phx", "tuple_lit.phx",
    "if_const_struct.phx", "trait_eq.phx", "trait_inherent.phx", "primitives_float.phx",
    "primitives_width.phx", "primitives_i128.phx", "primitives_u128.phx",
    "shift_width_mask.phx", "byte_string.phx", "string_literal.phx",
    "byte_string_as_str.phx", "ref_local.phx", "ref_fn_param.phx", "mut_ref_local.phx",
    "deref_ptr.phx", "slice_from_array.phx", "factorial.phx",
    "if_const_enum_single_variant.phx", "if_const_enum_non_exhaustive.phx",
    "if_var_reassign.phx", "if_const_else.phx", "generic_fn.phx", "generic_struct.phx",
    "generic_enum.phx", "generic_infer.phx", "generic_enum_infer.phx", "generic_enum_match.phx",
    "generic_impl_method.phx", "fn_pointer.phx", "drop.phx", "derive_partialeq.phx",
    "derive_enum_partialeq.phx", "derive_generic_struct.phx", "derive_generic_enum.phx",
    "attr_bracket_derive.phx", "millimeters.phx", "tuple_struct_two_field.phx",
]

NEGATIVE = [
    ("bad_type.phx", "type mismatch"),
    ("missing_main.phx", "main"),
    ("use_after_move.phx", "moved"),
    ("mixed_width.phx", "invalid"),
    ("invalid_utf8_byte_as_str.phx", "invalid cast"),
    ("match_unreachable_arm.phx", "unreachable"),
    ("trait_impl_incomplete.phx", "trait method"),
    ("deferred_break_value.phx", "break"),
    ("deferred_at_send.phx", "@send"),
    ("extern_unsafe.phx", "unsafe"),
    ("for_in_bad.phx", "IntoIter"),
    ("derive_bad.phx", "unsupported derive trait"),
    ("derive_generic_bad.phx", "trait bound"),
    ("newtype_bad.phx", "type mismatch"),
]


def rust_ident(name: str) -> str:
    s = re.sub(r"[^a-zA-Z0-9_]", "_", name)
    s = re.sub(r"_+", "_", s).strip("_").lower()
    if s and s[0].isdigit():
        s = f"_{s}"
    return s or "unnamed"


def raw_string(content: str) -> str:
    n = 0
    while f'{"#" * n}"' in content:
        n += 1
    delim = "#" * n
    return f'r{delim}"{content}"{delim}'


def write_file(path: Path, body: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(body, encoding="utf-8")


def gen_lib() -> None:
    write_file(
        OUT / "lib.rs",
        """//! Embedded Phoenix test programs (generated from CLI fixtures).
#![allow(missing_docs, dead_code)]

mod diagnostics;
pub mod modules;
mod negative;
pub mod projects;
pub mod single;
mod smoke;

pub use diagnostics::DIAGNOSTIC_CASES;
pub use modules::MODULE_TREES;
pub use negative::NEGATIVE_CASES;
pub use projects::PROJECTS;
pub use single::SINGLE_FILES;
pub use smoke::SMOKE_PROGRAMS;

/// Multi-file `#import` program rooted at a directory.
#[derive(Debug, Clone, Copy)]
pub struct ModuleTree {
    /// Stable name for diagnostics.
    pub name: &'static str,
    /// Entry file path relative to module root.
    pub entry: &'static str,
    /// `(relative_path, source)` pairs.
    pub files: &'static [(&'static str, &'static str)],
}

/// `phoenix.toml` project layout.
#[derive(Debug, Clone, Copy)]
pub struct ProjectSpec {
    /// Stable name (fixture directory name).
    pub name: &'static str,
    /// `phoenix.toml` contents.
    pub toml: &'static str,
    /// Project files as `(relative_path, source)`.
    pub files: &'static [(&'static str, &'static str)],
}

/// Single-file program at fixture root.
#[derive(Debug, Clone, Copy)]
pub struct SingleFile {
    /// File name (e.g. `sample.phx`).
    pub name: &'static str,
    /// Phoenix source.
    pub source: &'static str,
}

/// Positive smoke program.
#[derive(Debug, Clone, Copy)]
pub struct SmokeProgram {
    pub name: &'static str,
    pub source: &'static str,
}

/// Negative check case.
#[derive(Debug, Clone, Copy)]
pub struct NegativeCase {
    pub name: &'static str,
    pub source: &'static str,
    pub needle: &'static str,
}

/// Golden diagnostic case.
#[derive(Debug, Clone, Copy)]
pub struct DiagnosticCase {
    pub name: &'static str,
    pub kind: DiagnosticKind,
    /// Source for `CompileSource` / diagnostic-only single files; otherwise `None`.
    pub source: Option<&'static str>,
    pub expected: &'static str,
}

/// How to produce diagnostics for a golden case.
#[derive(Debug, Clone, Copy)]
pub enum DiagnosticKind {
    /// Single-file `check_file` on embedded source written to `name.phx`.
    SingleFile,
    /// `compile_source` on embedded source (no file path).
    CompileSource,
    /// Module tree; `entry` is relative path within the tree.
    ModuleTree { tree_name: &'static str },
    /// Project; `project_name` and `entry` relative to project root.
    Project {
        project_name: &'static str,
        entry: &'static str,
    },
}
""",
    )


def gen_smoke() -> None:
    lines = [
        "//! Positive smoke programs.\n",
        "use super::SmokeProgram;\n",
        "use super::single::*;\n\n",
    ]
    entries = []
    for name in SMOKE:
        path = FIXTURES / name
        source = path.read_text(encoding="utf-8")
        ident = rust_ident(name.replace(".phx", ""))
        const_name = f"SMOKE_{ident.upper()}"
        lines.append(f"pub const {const_name}: SmokeProgram = SmokeProgram {{")
        lines.append(f'    name: {raw_string(name)},')
        lines.append(f"    source: {raw_string(source)},")
        lines.append("};\n")
        entries.append(const_name)
    lines.append("pub const SMOKE_PROGRAMS: &[SmokeProgram] = &[")
    for e in entries:
        lines.append(f"    {e},")
    lines.append("];")
    write_file(OUT / "smoke.rs", "\n".join(lines) + "\n")


def gen_single() -> None:
    """All top-level .phx files not under a project directory."""
    project_names = {p.parent.name for p in FIXTURES.glob("*/phoenix.toml")}
    files = sorted(
        p
        for p in FIXTURES.glob("*.phx")
        if p.is_file()
    )
    lines = ["//! Single-file programs at fixture root.\n", "use super::SingleFile;\n\n"]
    entries = []
    for path in files:
        name = path.name
        source = path.read_text(encoding="utf-8")
        ident = rust_ident(name.replace(".phx", ""))
        const_name = f"SINGLE_{ident.upper()}"
        lines.append(f"pub const {const_name}: SingleFile = SingleFile {{")
        lines.append(f'    name: {raw_string(name)},')
        lines.append(f"    source: {raw_string(source)},")
        lines.append("};\n")
        entries.append((name, const_name))
    lines.append("pub const SINGLE_FILES: &[SingleFile] = &[")
    for _, e in entries:
        lines.append(f"    {e},")
    lines.append("];")
    lines.append("\npub fn single_by_name(name: &str) -> Option<&'static SingleFile> {")
    lines.append("    SINGLE_FILES.iter().find(|f| f.name == name)")
    lines.append("}")
    write_file(OUT / "single.rs", "\n".join(lines) + "\n")


def gen_negative() -> None:
    lines = ["//! Negative check programs.\n", "use super::NegativeCase;\n", "use super::single::*;\n\n"]
    entries = []
    for name, needle in NEGATIVE:
        path = FIXTURES / name
        if not path.is_file():
            # try single module
            sf = rust_ident(name.replace(".phx", ""))
            entries.append(
                f'    NegativeCase {{ name: {raw_string(name)}, source: SINGLE_{sf.upper()}.source, needle: {raw_string(needle)} }},'
            )
            continue
        source = path.read_text(encoding="utf-8")
        ident = rust_ident(name.replace(".phx", ""))
        const_name = f"NEG_{ident.upper()}"
        lines.append(f"pub const {const_name}: NegativeCase = NegativeCase {{")
        lines.append(f'    name: {raw_string(name)},')
        lines.append(f"    source: {raw_string(source)},")
        lines.append(f"    needle: {raw_string(needle)},")
        lines.append("};\n")
        entries.append(f"    {const_name},")
    lines.append("pub const NEGATIVE_CASES: &[NegativeCase] = &[")
    for e in entries:
        lines.append(e)
    lines.append("];")
    write_file(OUT / "negative.rs", "\n".join(lines) + "\n")


def collect_module_files(modules_dir: Path) -> list[tuple[str, str]]:
    files = []
    for path in sorted(modules_dir.rglob("*")):
        if path.is_file() and path.suffix in (".phx",):
            rel = path.relative_to(modules_dir).as_posix()
            files.append((rel, path.read_text(encoding="utf-8")))
    return files


def gen_modules() -> None:
    modules_dir = FIXTURES / "modules"
    # Group by entry files at top level
    entries = sorted(modules_dir.glob("*.phx"))
    trees: list[tuple[str, str, list]] = []
    all_files = collect_module_files(modules_dir)
    for entry in entries:
        name = rust_ident(entry.stem)
        trees.append((name, entry.name, all_files))
    lines = ["//! Multi-file `#import` module trees.\n", "use super::ModuleTree;\n\n"]
    consts = []
    for tree_name, entry, files in trees:
        cname = f"MODULE_{tree_name.upper()}"
        lines.append(f"static {cname}_FILES: &[(&str, &str)] = &[")
        for rel, src in files:
            lines.append(f"    ({raw_string(rel)}, {raw_string(src)}),")
        lines.append("];\n")
        lines.append(f"pub const {cname}: ModuleTree = ModuleTree {{")
        lines.append(f'    name: {raw_string(tree_name)},')
        lines.append(f'    entry: {raw_string(entry)},')
        lines.append(f"    files: {cname}_FILES,")
        lines.append("};\n")
        consts.append(cname)
    lines.append("pub const MODULE_TREES: &[ModuleTree] = &[")
    for c in consts:
        lines.append(f"    {c},")
    lines.append("];")
    lines.append("\npub fn module_tree_by_name(name: &str) -> Option<&'static ModuleTree> {")
    lines.append("    MODULE_TREES.iter().find(|t| t.name == name)")
    lines.append("}")
    write_file(OUT / "modules.rs", "\n".join(lines) + "\n")


def gen_projects() -> None:
    toml_paths = sorted(FIXTURES.glob("*/phoenix.toml"))
    lines = ["//! `phoenix.toml` project fixtures.\n", "use super::ProjectSpec;\n\n"]
    consts = []
    for toml_path in toml_paths:
        project_dir = toml_path.parent
        name = project_dir.name
        ident = rust_ident(name)
        cname = f"PROJECT_{ident.upper()}"
        toml = toml_path.read_text(encoding="utf-8")
        files = []
        for path in sorted(project_dir.rglob("*")):
            if path.is_file() and path.name != "phoenix.toml" and not path.name.startswith("."):
                rel = path.relative_to(project_dir).as_posix()
                if path.suffix in (".phx",) or rel.endswith(".gitkeep"):
                    if path.suffix == ".phx":
                        files.append((rel, path.read_text(encoding="utf-8")))
                    else:
                        files.append((rel, ""))
        flist = f"{cname}_FILES"
        lines.append(f"static {flist}: &[(&str, &str)] = &[")
        for rel, src in files:
            lines.append(f"    ({raw_string(rel)}, {raw_string(src)}),")
        lines.append("];\n")
        lines.append(f"pub const {cname}: ProjectSpec = ProjectSpec {{")
        lines.append(f'    name: {raw_string(name)},')
        lines.append(f"    toml: {raw_string(toml)},")
        lines.append(f"    files: {flist},")
        lines.append("};\n")
        consts.append(cname)
    lines.append("pub const PROJECTS: &[ProjectSpec] = &[")
    for c in consts:
        lines.append(f"    {c},")
    lines.append("];")
    lines.append("\npub fn project_by_name(name: &str) -> Option<&'static ProjectSpec> {")
    lines.append("    PROJECTS.iter().find(|p| p.name == name)")
    lines.append("}")
    write_file(OUT / "projects.rs", "\n".join(lines) + "\n")


def gen_diagnostics() -> None:
    # Map from diagnostics.rs test cases
    cases = [
        ("bad_type", "SingleFile", "bad_type.phx", None, None),
        ("use_after_move", "SingleFile", "use_after_move.phx", None, None),
        ("missing_main", "SingleFile", "missing_main.phx", None, None),
        ("import_cycle", "ModuleTree", "cycle_a", None, None),
        ("multi_resolve_duplicate", "CompileSource", None, None, None),
        ("return_local_str", "DiagFile", "return_local_str.phx", None, None),
        ("pow_unsupported", "DiagFile", "pow_unsupported.phx", None, None),
        ("hash_derive_invalid", "DiagFile", "hash_derive_invalid.phx", None, None),
        ("unique_ptr_use_after_move", "Project", None, "unique_ptr_move_in", "src/main.phx"),
        ("trait_impl_incomplete", "SingleFile", "trait_impl_incomplete.phx", None, None),
        ("extern_unsafe", "SingleFile", "extern_unsafe.phx", None, None),
        ("invalid_utf8_byte_as_str", "SingleFile", "invalid_utf8_byte_as_str.phx", None, None),
        ("match_unreachable_arm", "SingleFile", "match_unreachable_arm.phx", None, None),
        ("try_ok_mismatch", "Project", None, "std_try_ok_mismatch", "src/main.phx"),
        ("try_from_missing", "Project", None, "std_try_from_missing", "src/main.phx"),
        ("discarded_std_result", "Project", None, "lint_std_result_discard", "src/main.phx"),
        ("discarded_std_option", "Project", None, "lint_std_option_discard", "src/main.phx"),
        ("loop_move_use_after_loop", "DiagFile", "loop_move_use_after_loop.phx", None, None),
        ("if_branch_sibling_no_false_uam", "DiagFile", "if_branch_sibling_no_false_uam.phx", None, None),
        ("if_branch_untaken_no_move", "DiagFile", "if_branch_untaken_no_move.phx", None, None),
        ("invalid_cast", "DiagFile", "invalid_cast.phx", None, None),
        ("double_mut_borrow", "DiagFile", "double_mut_borrow.phx", None, None),
        ("shared_mut_borrow", "DiagFile", "shared_mut_borrow.phx", None, None),
        ("if_arm_overlapping_mut", "DiagFile", "if_arm_overlapping_mut.phx", None, None),
        ("loop_overlapping_mut", "DiagFile", "loop_overlapping_mut.phx", None, None),
    ]
    lines = [
        "//! Golden diagnostic cases.\n",
        "use super::{DiagnosticCase, DiagnosticKind};\n",
        "use super::single::*;\n",
        "use super::modules::*;\n",
        "use super::projects::*;\n\n",
    ]
    entries = []
    for case_name, kind, file_or_tree, project, entry in cases:
        stderr_path = DIAG / f"{case_name}.stderr"
        expected = stderr_path.read_text(encoding="utf-8") if stderr_path.is_file() else ""
        cname = f"DIAG_{rust_ident(case_name).upper()}"
        if kind == "SingleFile":
            sf = rust_ident(file_or_tree.replace(".phx", ""))
            dk = f"DiagnosticKind::SingleFile"
            extra = ""
        elif kind == "CompileSource":
            src_path = DIAG / file_or_tree if file_or_tree else DIAG / f"{case_name}.phx"
            if not (DIAG / f"{case_name}.phx").is_file():
                src_path = DIAG / f"{case_name}.phx"
            source = (DIAG / f"{case_name}.phx").read_text(encoding="utf-8")
            lines.append(f"const {cname}_SOURCE: &str = {raw_string(source)};\n")
            dk = "DiagnosticKind::CompileSource"
            extra = ""
        elif kind == "ModuleTree":
            dk = f'DiagnosticKind::ModuleTree {{ tree_name: {raw_string(file_or_tree)} }}'
            extra = ""
        elif kind == "DiagFile":
            source = (DIAG / file_or_tree).read_text(encoding="utf-8")
            ident = rust_ident(file_or_tree.replace(".phx", ""))
            lines.append(f"const {cname}_SOURCE: &str = {raw_string(source)};\n")
            dk = "DiagnosticKind::SingleFile"
            extra = f"// uses {cname}_SOURCE via name {raw_string(file_or_tree)}"
        elif kind == "Project":
            dk = f'DiagnosticKind::Project {{ project_name: {raw_string(project)}, entry: {raw_string(entry)} }}'
            extra = ""
        else:
            raise ValueError(kind)
        lines.append(f"pub const {cname}: DiagnosticCase = DiagnosticCase {{")
        lines.append(f'    name: {raw_string(case_name)},')
        lines.append(f"    kind: {dk},")
        if kind == "CompileSource":
            lines.append(f"    source: Some({cname}_SOURCE),")
        elif kind == "DiagFile":
            lines.append(f"    source: Some({cname}_SOURCE),")
        else:
            lines.append("    source: None,")
        lines.append(f"    expected: {raw_string(expected)},")
        lines.append("};\n")
        entries.append(cname)
    lines.append("pub const DIAGNOSTIC_CASES: &[DiagnosticCase] = &[")
    for e in entries:
        lines.append(f"    {e},")
    lines.append("];")
    write_file(OUT / "diagnostics.rs", "\n".join(lines) + "\n")


def main() -> None:
    gen_lib()
    gen_single()
    gen_smoke()
    gen_negative()
    gen_modules()
    gen_projects()
    gen_diagnostics()
    print(f"Generated phx-programs sources in {OUT}")


if __name__ == "__main__":
    main()
