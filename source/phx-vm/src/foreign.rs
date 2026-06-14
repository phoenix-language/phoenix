//! VM-hosted foreign stub table for `extern "C"` symbols (Phase A).
//!
//! ## Phase A contract
//!
//! Phase A is **VM-hosted** foreign calls: the compiler records `extern "C"` signatures
//! and the VM resolves symbols at load time via test stubs. See `docs/design/features/ffi.md`.
//!
//! Stub ids are assigned in **registration order** (`next_id` starting at 0). Re-registering
//! the same name returns the existing id. This is intentional for Phase A test harnesses but
//! **not stable for real linking** — linker-assigned symbol ids are deferred to post-beta
//! link hardening (ROADMAP "Stable FFI symbol identity").
//!
//! Stubs are invoked via `CallIndirect` with `target_kind = 1`.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use phx_bytecode::BytecodeModule;

use crate::VmErrorKind;
use crate::context::Machine;

/// Foreign stub signature: receives the live machine and module; pops args and pushes return.
pub type ForeignStubFn = fn(&mut Machine, &BytecodeModule) -> Result<(), VmErrorKind>;

/// Registry of foreign stubs keyed by id and name.
#[derive(Default)]
pub struct ForeignRegistry {
    by_id: HashMap<u32, ForeignStubFn>,
    by_name: HashMap<String, u32>,
    next_id: u32,
}

impl std::fmt::Debug for ForeignRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ForeignRegistry")
            .field("stub_count", &self.by_id.len())
            .field("next_id", &self.next_id)
            .finish_non_exhaustive()
    }
}

impl ForeignRegistry {
    /// Registers a foreign stub under `name` and returns its stub id.
    ///
    /// Re-registering the same `name` returns the existing id and updates the stub fn.
    #[must_use]
    pub fn register(&mut self, name: &str, stub: ForeignStubFn) -> u32 {
        if let Some(&id) = self.by_name.get(name) {
            self.by_id.insert(id, stub);
            return id;
        }
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.by_name.insert(name.to_owned(), id);
        self.by_id.insert(id, stub);
        id
    }

    /// Dispatches a foreign stub by id.
    ///
    /// # Errors
    ///
    /// Returns [`VmErrorKind::InvalidForeignStub`] when `id` was never registered.
    pub fn dispatch(
        &self,
        id: u32,
        machine: &mut Machine,
        module: &BytecodeModule,
    ) -> Result<(), VmErrorKind> {
        let stub = self
            .by_id
            .get(&id)
            .copied()
            .ok_or(VmErrorKind::InvalidForeignStub(id))?;
        stub(machine, module)
    }

    /// Clears all registered stubs.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

static FOREIGN_STUBS: OnceLock<Mutex<ForeignRegistry>> = OnceLock::new();

fn registry() -> &'static Mutex<ForeignRegistry> {
    FOREIGN_STUBS.get_or_init(|| Mutex::new(ForeignRegistry::default()))
}

fn lock_registry() -> MutexGuard<'static, ForeignRegistry> {
    registry().lock().unwrap_or_else(PoisonError::into_inner)
}

/// Registers a foreign stub under `name` in the process-global Phase A registry.
///
/// Re-registering the same `name` returns the existing id.
#[must_use]
pub fn register_foreign_stub(name: &str, stub: ForeignStubFn) -> u32 {
    lock_registry().register(name, stub)
}

/// Dispatches a foreign stub by id from the process-global Phase A registry.
///
/// # Errors
///
/// Returns [`VmErrorKind::InvalidForeignStub`] when `id` was never registered.
pub fn dispatch_foreign(
    id: u32,
    machine: &mut Machine,
    module: &BytecodeModule,
) -> Result<(), VmErrorKind> {
    let reg = lock_registry();
    reg.dispatch(id, machine, module)
}

/// Dispatches a foreign stub using an explicit registry (future per-runtime linking).
///
/// # Errors
///
/// Returns [`VmErrorKind::InvalidForeignStub`] when `id` was never registered.
#[doc(hidden)]
pub fn dispatch_foreign_in(
    registry: &ForeignRegistry,
    id: u32,
    machine: &mut Machine,
    module: &BytecodeModule,
) -> Result<(), VmErrorKind> {
    registry.dispatch(id, machine, module)
}

/// Clears all registered stubs in the process-global registry (test harness only).
pub fn clear_foreign_stubs() {
    lock_registry().clear();
}
