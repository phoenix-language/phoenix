//! Look up embedded test programs by legacy fixture name.

use phx_programs::{
    ModuleTree, ProjectSpec, SingleFile, modules::module_tree_by_name, projects::project_by_name,
    single::single_by_name,
};

/// Look up a single-file program by fixture file name (e.g. `sample.phx`).
pub fn lookup_single(name: &str) -> &'static SingleFile {
    single_by_name(name).unwrap_or_else(|| panic!("unknown single-file program: {name}"))
}

/// Look up a `phoenix.toml` project by fixture directory name.
pub fn lookup_project(name: &str) -> &'static ProjectSpec {
    project_by_name(name).unwrap_or_else(|| panic!("unknown project: {name}"))
}

/// Look up a module tree by stable tree name (e.g. `main`, `cycle_a`).
pub fn lookup_module_tree(name: &str) -> &'static ModuleTree {
    module_tree_by_name(name).unwrap_or_else(|| panic!("unknown module tree: {name}"))
}

/// Module tree for `modules/main.phx`.
pub fn modules_main_tree() -> &'static ModuleTree {
    lookup_module_tree("main")
}

/// Entry file name for the default modules smoke test.
pub const MODULES_MAIN_ENTRY: &str = "main.phx";
