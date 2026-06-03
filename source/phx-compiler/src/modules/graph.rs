//! Import dependency graph and topological ordering.

use std::collections::{HashMap, HashSet, VecDeque};

use phx_diagnostics::{DiagnosticBag, ResolveError, Span};
use phx_syntax::Interner;
use phx_syntax::ast::decl::{ImportDirective, ImportItem};
use phx_syntax::ast::ident::PathSegment;

use super::loader::{LoadedModule, ModuleId};
use super::path::ModulePath;
use crate::project::BuildLayout;
use crate::pxi::PxiFile;

/// Directed edges: importer → imported module path string.
fn topo_sort_inner(
    module_count: usize,
    edges: &[(ModuleId, ModuleId)],
) -> Option<Vec<ModuleId>> {
    let mut indegree = vec![0usize; module_count];
    let mut adj: Vec<Vec<ModuleId>> = vec![Vec::new(); module_count];
    for &(from, to) in edges {
        if from.index() as usize >= module_count || to.index() as usize >= module_count {
            continue;
        }
        adj[from.index() as usize].push(to);
        indegree[to.index() as usize] += 1;
    }

    let mut queue: VecDeque<ModuleId> = VecDeque::new();
    for i in 0..module_count {
        if indegree[i] == 0 {
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

/// Topological order; reports [`ResolveError::CircularImport`] on failure.
pub(crate) fn topo_sort(
    module_count: usize,
    edges: &[(ModuleId, ModuleId)],
    roots: ModuleId,
    bag: &mut DiagnosticBag,
) -> Option<Vec<ModuleId>> {
    let _ = roots;
    topo_sort_inner(module_count, edges).or_else(|| {
        bag.push(ResolveError::CircularImport {
            span: Span::new(0, 0),
            cycle: "module graph cycle".to_owned(),
        });
        None
    })
}

/// Topological order, or cycle escape when every cyclic module has a fresh `.pxi`.
pub(crate) fn topo_sort_with_pxi_escape(
    module_count: usize,
    edges: &[(ModuleId, ModuleId)],
    roots: ModuleId,
    modules: &[LoadedModule],
    layout: Option<&BuildLayout>,
    bag: &mut DiagnosticBag,
) -> Option<Vec<ModuleId>> {
    let _ = roots;
    if let Some(order) = topo_sort_inner(module_count, edges) {
        return Some(order);
    }
    if let Some(layout) = layout {
        if cycle_modules_have_fresh_pxi(module_count, edges, modules, layout) {
            return Some(
                (0..module_count)
                    .map(|i| ModuleId::from_raw(u32::try_from(i).unwrap_or(u32::MAX)))
                    .collect(),
            );
        }
    }
    bag.push(ResolveError::CircularImport {
        span: Span::new(0, 0),
        cycle: format!("module graph cycle (entry module id {})", roots.index()),
    });
    None
}

fn cycle_modules_have_fresh_pxi(
    module_count: usize,
    edges: &[(ModuleId, ModuleId)],
    modules: &[LoadedModule],
    layout: &BuildLayout,
) -> bool {
    let mut indegree = vec![0usize; module_count];
    for &(_, to) in edges {
        if (to.index() as usize) < module_count {
            indegree[to.index() as usize] += 1;
        }
    }
    let cyclic: Vec<usize> = indegree
        .iter()
        .enumerate()
        .filter(|(_, d)| **d > 0)
        .map(|(i, _)| i)
        .collect();
    if cyclic.is_empty() {
        return false;
    }
    cyclic.iter().all(|&idx| {
        let m = &modules[idx];
        let pxi_path = layout.module_artifacts(&m.logical_path.display()).pxi;
        let Ok(pxi) = PxiFile::read_from_path(&pxi_path) else {
            return false;
        };
        pxi.source_is_fresh(&m.filesystem)
    })
}

/// Resolves import directive to target module path.
/// Resolves import directive to target module path.
pub fn import_target_module(
    import: &ImportDirective,
    interner: &Interner,
) -> ModulePath {
    if import.items.is_some() {
        ModulePath::from_ast_path(&import.path, interner)
    } else {
        let (module, _) = ModulePath::split_import_target(&import.path, interner);
        module
    }
}

/// Maps logical module path → `ModuleId`.
pub(crate) fn build_path_index(paths: &[ModulePath]) -> HashMap<String, ModuleId> {
    let mut map = HashMap::new();
    for (i, p) in paths.iter().enumerate() {
        map.insert(p.display(), ModuleId::from_raw(u32::try_from(i).unwrap_or(u32::MAX)));
    }
    map
}

/// Collects import edges from parsed import lists.
pub(crate) fn collect_edges(
    importer: ModuleId,
    imports: &[phx_syntax::ast::Node<ImportDirective>],
    path_index: &HashMap<String, ModuleId>,
    interner: &Interner,
    bag: &mut DiagnosticBag,
) -> Vec<(ModuleId, ModuleId)> {
    let mut edges = Vec::new();
    let mut seen = HashSet::new();
    for imp in imports {
        let target_path = import_target_module(&imp.inner, interner);
        let key = target_path.display();
        if key.is_empty() {
            continue;
        }
        let Some(&dep) = path_index.get(&key) else {
            bag.push(ResolveError::ModuleNotFound {
                span: imp.span,
                path: key,
            });
            continue;
        };
        let edge = (importer, dep);
        if seen.insert(edge) {
            edges.push(edge);
        }
    }
    edges
}
