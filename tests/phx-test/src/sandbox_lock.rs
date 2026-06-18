//! Per-project filesystem locks for shared sandbox fixtures and the repo stdlib.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};

static PROJECT_LOCKS: OnceLock<Mutex<HashMap<String, &'static Mutex<()>>>> = OnceLock::new();
static STD_LOCK: Mutex<()> = Mutex::new(());

fn project_mutex(name: &str) -> &'static Mutex<()> {
    let map = PROJECT_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = map
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(mutex) = map.get(name) {
        return mutex;
    }
    let leaked: &'static Mutex<()> = Box::leak(Box::new(Mutex::new(())));
    map.insert(name.to_string(), leaked);
    leaked
}

/// Serializes mutations and builds for a named sandbox project (`tests/cli/fixtures/<name>/`).
pub fn project_fs_lock(name: &str) -> MutexGuard<'static, ()> {
    project_mutex(name)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Run `f` while holding the per-project lock (use when subprocess I/O must not race builds).
pub fn with_project_fs_lock<R>(name: &str, f: impl FnOnce() -> R) -> R {
    let _lock = project_fs_lock(name);
    f()
}

/// Serializes mutations and builds for the repository `std/` project.
pub fn std_fs_lock() -> MutexGuard<'static, ()> {
    STD_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Returns the sandbox project name when `root` is `…/tests/cli/fixtures/<name>`.
pub fn sandbox_project_name_from_path(root: &Path) -> Option<String> {
    let path = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut components = path.components().rev();
    let name = components.next()?.as_os_str().to_str()?;
    let fixtures = components.next()?;
    if fixtures.as_os_str() != "fixtures" {
        return None;
    }
    let cli = components.next()?;
    if cli.as_os_str() != "cli" {
        return None;
    }
    let tests = components.next()?;
    if tests.as_os_str() != "tests" {
        return None;
    }
    Some(name.to_string())
}

/// Returns the sandbox project name when `path` lies under `…/tests/cli/fixtures/<name>/`.
pub fn sandbox_project_name_from_fixture_path(path: &Path) -> Option<String> {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    for ancestor in path.ancestors() {
        if let Some(name) = sandbox_project_name_from_path(ancestor) {
            return Some(name);
        }
    }
    None
}
