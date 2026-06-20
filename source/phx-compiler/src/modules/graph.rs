//! Import dependency graph and topological ordering.
//!
//! ## Pass role
//!
//! Used by [`super::loader`] after all modules are parsed: [`collect_edges`] turns `#import`
//! directives into `(importer, imported, span)` edges, then [`topo_sort_with_pxi_escape`] either
//! validates acyclic order or permits import SCCs when every cyclic module has a **fresh** `.pxi`
//! (source hash matches). Cross-module ABI inside an SCC is frozen by interface files, not parse
//! order. If any cyclic module lacks a fresh `.pxi`, [`ResolveError::CircularImport`] is reported
//! at an `#import` edge in the cycle.
//!
//! ## Entry points
//!
//! - [`import_target_module`] — resolve one `#import` path to a [`ModulePath`] (public helper for tests)
//! - `collect_edges` / `topo_sort_with_pxi_escape` — loader-internal edge build and cycle check

use std::collections::{HashMap, HashSet, VecDeque};

use phx_diagnostics::{DiagnosticBag, ResolveError, Span};
use phx_syntax::Interner;
use phx_syntax::ast::decl::ImportDirective;

use super::loader::{LoadedModule, ModuleId};
use super::path::ModulePath;
use crate::project::BuildLayout;
use crate::pxi::PxiFile;

/// Import edge: `from` module imports `to`, at `#import` span `span`.
pub(crate) type ImportEdge = (ModuleId, ModuleId, Span);

fn topo_sort_inner(module_count: usize, edges: &[ImportEdge]) -> Option<Vec<ModuleId>> {
    let mut indegree = vec![0usize; module_count];
    let mut adj: Vec<Vec<ModuleId>> = vec![Vec::new(); module_count];
    for &(from, to, _) in edges {
        if from.index() as usize >= module_count || to.index() as usize >= module_count {
            continue;
        }
        adj[from.index() as usize].push(to);
        indegree[to.index() as usize] += 1;
    }

    let mut queue: VecDeque<ModuleId> = VecDeque::new();
    for (i, &deg) in indegree.iter().enumerate() {
        if deg == 0 {
            queue.push_back(ModuleId::from_raw(u32::try_from(i).unwrap_or(u32::MAX)));
        }
    }

    let mut order = Vec::with_capacity(module_count);
    while let Some(n) = queue.pop_front() {
        order.push(n);
        for &m in &adj[n.index() as usize] {
            let idx = m.index() as usize;
            indegree[idx] = indegree[idx].saturating_sub(1);
            if indegree[idx] == 0 {
                queue.push_back(m);
            }
        }
    }

    if order.len() != module_count {
        return None;
    }
    Some(order)
}

fn cyclic_module_indices(module_count: usize, edges: &[ImportEdge]) -> HashSet<u32> {
    let mut indegree = vec![0usize; module_count];
    for &(_, to, _) in edges {
        if (to.index() as usize) < module_count {
            indegree[to.index() as usize] += 1;
        }
    }
    indegree
        .iter()
        .enumerate()
        .filter(|(_, d)| **d > 0)
        .map(|(i, _)| u32::try_from(i).unwrap_or(u32::MAX))
        .collect()
}

fn import_adjacency(module_count: usize, edges: &[ImportEdge]) -> Vec<Vec<ModuleId>> {
    let mut adj = vec![Vec::new(); module_count];
    for &(from, to, _) in edges {
        if (from.index() as usize) < module_count && (to.index() as usize) < module_count {
            adj[from.index() as usize].push(to);
        }
    }
    adj
}

fn cycle_edge(edges: &[ImportEdge], module_count: usize) -> Option<(Span, u32)> {
    let cyclic = cyclic_module_indices(module_count, edges);
    for &(from, to, span) in edges {
        if cyclic.contains(&from.index()) && cyclic.contains(&to.index()) {
            return Some((span, from.index()));
        }
    }
    edges.first().map(|&(from, _, span)| (span, from.index()))
}

fn module_display(modules: &[LoadedModule], id: ModuleId) -> String {
    modules.get(id.index() as usize).map_or_else(
        || format!("module#{}", id.index()),
        |m| m.logical_path.display(),
    )
}

fn find_cycle_path(
    adj: &[Vec<ModuleId>],
    start: ModuleId,
    cyclic: &HashSet<u32>,
) -> Option<Vec<ModuleId>> {
    let mut path = Vec::new();
    let mut on_path = HashSet::new();
    let mut found = None;
    find_cycle_path_dfs(
        adj,
        start,
        start,
        cyclic,
        &mut path,
        &mut on_path,
        &mut found,
    );
    found
}

fn find_cycle_path_dfs(
    adj: &[Vec<ModuleId>],
    start: ModuleId,
    node: ModuleId,
    cyclic: &HashSet<u32>,
    path: &mut Vec<ModuleId>,
    on_path: &mut HashSet<u32>,
    found: &mut Option<Vec<ModuleId>>,
) {
    if found.is_some() {
        return;
    }
    path.push(node);
    on_path.insert(node.index());

    for &next in &adj[node.index() as usize] {
        if !cyclic.contains(&next.index()) {
            continue;
        }
        if next == start && path.len() > 1 {
            let mut cycle = path.clone();
            cycle.push(start);
            *found = Some(cycle);
            return;
        }
        if !on_path.contains(&next.index()) {
            find_cycle_path_dfs(adj, start, next, cyclic, path, on_path, found);
        }
    }

    path.pop();
    on_path.remove(&node.index());
}

fn format_cycle_trace(edges: &[ImportEdge], modules: &[LoadedModule], start_module: u32) -> String {
    let module_count = modules.len();
    let cyclic = cyclic_module_indices(module_count, edges);
    let adj = import_adjacency(module_count, edges);
    let start = ModuleId::from_raw(start_module);

    if let Some(cycle) = find_cycle_path(&adj, start, &cyclic) {
        return cycle
            .iter()
            .map(|&id| module_display(modules, id))
            .collect::<Vec<_>>()
            .join(" → ");
    }

    cyclic
        .iter()
        .map(|&idx| module_display(modules, ModuleId::from_raw(idx)))
        .collect::<Vec<_>>()
        .join(" → ")
}

/// Topological order, or identity `0..n` when every cyclic module has a fresh `.pxi`.
///
/// The fallback order is arbitrary; importers must use `.pxi` export lists, not source parse order.
#[allow(clippy::too_many_arguments)]
pub(crate) fn topo_sort_with_pxi_escape(
    module_count: usize,
    edges: &[ImportEdge],
    roots: ModuleId,
    modules: &[LoadedModule],
    layout: Option<&BuildLayout>,
    workspace_package: &str,
    dep_names: &[&str],
    bag: &mut DiagnosticBag,
) -> Option<Vec<ModuleId>> {
    let _ = roots;
    if let Some(order) = topo_sort_inner(module_count, edges) {
        return Some(order);
    }
    if let Some(layout) = layout
        && cycle_modules_have_fresh_pxi(
            module_count,
            edges,
            modules,
            layout,
            workspace_package,
            dep_names,
        )
    {
        return Some(
            (0..module_count)
                .map(|i| ModuleId::from_raw(u32::try_from(i).unwrap_or(u32::MAX)))
                .collect(),
        );
    }
    let (span, module) = cycle_edge(edges, module_count).unwrap_or_else(|| {
        debug_assert!(
            false,
            "cyclic import graph must expose at least one edge span"
        );
        edges
            .first()
            .map_or((Span::new(0, 1), 0), |&(_, _, span)| (span, 0))
    });
    let cycle = format_cycle_trace(edges, modules, module);
    bag.push(module, ResolveError::CircularImport { span, cycle });
    None
}

fn cycle_modules_have_fresh_pxi(
    module_count: usize,
    edges: &[ImportEdge],
    modules: &[LoadedModule],
    layout: &BuildLayout,
    workspace_package: &str,
    dep_names: &[&str],
) -> bool {
    let cyclic: Vec<usize> = cyclic_module_indices(module_count, edges)
        .into_iter()
        .map(|idx| idx as usize)
        .collect();
    if cyclic.is_empty() {
        return false;
    }
    cyclic.iter().all(|&idx| {
        let m = &modules[idx];
        let pxi_path = layout
            .module_artifacts_resolved(&m.logical_path.display(), workspace_package, dep_names)
            .pxi;
        let Ok(pxi) = PxiFile::read_from_path(&pxi_path) else {
            return false;
        };
        pxi.source_is_fresh(&m.filesystem)
    })
}

/// Resolves import directive to target module path.
pub fn import_target_module(import: &ImportDirective, interner: &Interner) -> ModulePath {
    if import.items.is_some() {
        ModulePath::from_ast_path(&import.path, interner)
    } else {
        let (module, _) = ModulePath::split_import_target(&import.path, interner);
        module
    }
}

/// Collects import edges from parsed import lists.
pub(crate) fn collect_edges(
    importer: ModuleId,
    imports: &[phx_syntax::ast::Node<ImportDirective>],
    path_index: &HashMap<String, ModuleId>,
    interner: &Interner,
    workspace_name: &str,
    dep_names: &[&str],
    bag: &mut DiagnosticBag,
) -> Vec<ImportEdge> {
    let mut edges = Vec::new();
    let mut seen = HashSet::new();
    for imp in imports {
        let target_path = import_target_module(&imp.inner, interner);
        let canonical = ModulePath::canonicalize_import(&target_path, workspace_name, dep_names);
        let key = canonical.display();
        if key.is_empty() {
            continue;
        }
        let Some(&dep) = path_index.get(&key) else {
            bag.push(
                importer.index(),
                ResolveError::ModuleNotFound {
                    span: imp.span,
                    path: key,
                },
            );
            continue;
        };
        let edge = (importer, dep, imp.span);
        if seen.insert((importer, dep)) {
            edges.push(edge);
        }
    }
    edges
}
