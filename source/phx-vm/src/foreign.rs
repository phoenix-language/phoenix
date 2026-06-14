//! VM-hosted foreign stub table for `extern "C"` symbols (Phase A).
//!
//! Stubs are registered at test/load time and invoked via `CallIndirect` with
//! `target_kind = 1`.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use phx_bytecode::BytecodeModule;

use crate::VmErrorKind;
use crate::frame::Machine;

/// Foreign stub signature: receives the live machine and module; pops args and pushes return.
pub type ForeignStubFn = fn(&mut Machine, &BytecodeModule) -> Result<(), VmErrorKind>;

static FOREIGN_STUBS: OnceLock<Mutex<ForeignStubRegistry>> = OnceLock::new();

#[derive(Default)]
struct ForeignStubRegistry {
    by_id: HashMap<u32, ForeignStubFn>,
    by_name: HashMap<String, u32>,
    next_id: u32,
}

fn registry() -> &'static Mutex<ForeignStubRegistry> {
    FOREIGN_STUBS.get_or_init(|| Mutex::new(ForeignStubRegistry::default()))
}

fn lock_registry() -> MutexGuard<'static, ForeignStubRegistry> {
    registry().lock().unwrap_or_else(PoisonError::into_inner)
}

/// Registers a foreign stub under `name` and returns its stable stub id.
///
/// Re-registering the same `name` returns the existing id.
#[must_use]
pub fn register_foreign_stub(name: &str, stub: ForeignStubFn) -> u32 {
    let mut reg = lock_registry();
    if let Some(&id) = reg.by_name.get(name) {
        reg.by_id.insert(id, stub);
        return id;
    }
    let id = reg.next_id;
    reg.next_id = reg.next_id.saturating_add(1);
    reg.by_name.insert(name.to_owned(), id);
    reg.by_id.insert(id, stub);
    id
}

/// Dispatches a foreign stub by id.
///
/// # Errors
///
/// Returns [`VmError::InvalidForeignStub`] when `id` was never registered.
pub fn dispatch_foreign(
    id: u32,
    machine: &mut Machine,
    module: &BytecodeModule,
) -> Result<(), VmErrorKind> {
    let reg = lock_registry();
    let stub = reg
        .by_id
        .get(&id)
        .copied()
        .ok_or(VmErrorKind::InvalidForeignStub(id))?;
    drop(reg);
    stub(machine, module)
}

/// Clears all registered stubs (test harness only).
pub fn clear_foreign_stubs() {
    *lock_registry() = ForeignStubRegistry::default();
}
