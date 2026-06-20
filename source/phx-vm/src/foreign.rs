//! VM-hosted foreign stub table for `extern "C"` symbols (Phase A).
//!
//! Phase A resolves `extern "C"` imports at VM load time via Rust-hosted stubs instead of
//! `dlopen`/`dlsym`. The compiler records foreign signatures; codegen lowers calls to
//! [`Opcode::CallIndirect`](phx_bytecode::Opcode::CallIndirect) with `target_kind = 1`; the interpreter
//! routes those calls through [`dispatch_foreign`]. See `docs/design/features/ffi.md`.
//!
//! ## Phase A contract
//!
//! | Rule | Detail |
//! | --- | --- |
//! | Registration | Stubs are keyed by symbol name and assigned ids in **registration order** (`next_id` starting at 0) |
//! | Re-registration | Same `name` returns the existing id and replaces the stub fn |
//! | Id stability | Intentional for test harnesses; **not** linker-stable — see ROADMAP "Stable FFI symbol identity" |
//! | Stack convention | Stubs pop arguments from [`Machine::stack`] (first param at lower index) and push the return value |
//!
//! ## Global vs explicit registry
//!
//! [`register_foreign_stub`] and [`dispatch_foreign`] use a process-global [`ForeignRegistry`]
//! behind a mutex — sufficient for `phx run` and integration tests. [`dispatch_foreign_in`] accepts
//! an explicit registry for future per-runtime linking (Phase B).
//!
//! [`clear_foreign_stubs`] resets the global registry; test harness only.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use phx_bytecode::BytecodeModule;

use crate::VmErrorKind;
use crate::context::Machine;

/// Foreign stub signature: receives the live machine and module; pops args and pushes return.
///
/// Stubs run synchronously on the interpreter thread. They must follow the VM stack convention:
/// pop arguments in codegen order (first parameter at the lower stack index), then push one
/// [`crate::frame::Value`] return cell (or none for `()` returns — push is omitted by convention in
/// Phase A tests).
pub type ForeignStubFn = fn(&mut Machine, &BytecodeModule) -> Result<(), VmErrorKind>;

/// Registry of foreign stubs keyed by id and name.
///
/// Owns stub function pointers and the name→id map used when codegen assigns foreign ids by
/// declaration order. Prefer the process-global helpers ([`register_foreign_stub`],
/// [`dispatch_foreign`]) unless embedding the VM with a dedicated link table.
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
    /// Ids are assigned sequentially from zero on first registration. Re-registering the same
    /// `name` returns the existing id and replaces the stub fn.
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
    /// Returns [`VmErrorKind::InvalidForeignStub`] when `id` was never registered. Propagates
    /// any error returned by the stub itself.
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

    /// Clears all registered stubs and resets id allocation.
    ///
    /// Test harness only — production callers should not rely on clearing mid-process.
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
/// Re-registering the same `name` returns the existing id and replaces the stub fn. Registration
/// order must match codegen foreign id assignment for the linked program.
#[must_use]
pub fn register_foreign_stub(name: &str, stub: ForeignStubFn) -> u32 {
    lock_registry().register(name, stub)
}

/// Dispatches a foreign stub by id from the process-global Phase A registry.
///
/// Called by the interpreter's `CallIndirect` handler when `target_kind = 1`.
///
/// # Errors
///
/// Returns [`VmErrorKind::InvalidForeignStub`] when `id` was never registered. Propagates any
/// error returned by the stub itself.
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
/// Same contract as [`dispatch_foreign`], but reads from `registry` instead of the global table.
///
/// # Errors
///
/// Returns [`VmErrorKind::InvalidForeignStub`] when `id` was never registered. Propagates any
/// error returned by the stub itself.
#[doc(hidden)]
pub fn dispatch_foreign_in(
    registry: &ForeignRegistry,
    id: u32,
    machine: &mut Machine,
    module: &BytecodeModule,
) -> Result<(), VmErrorKind> {
    registry.dispatch(id, machine, module)
}

/// Clears all registered stubs in the process-global registry.
///
/// Test harness only — resets id allocation to zero.
pub fn clear_foreign_stubs() {
    lock_registry().clear();
}
