//! `?` operator lowering metadata.
//!
//! Records [`TrySiteMeta`] per postfix `?` expression during type checking so lowering can emit the
//! correct failure arm without re-deriving enum tags or local slots from the AST.
//!
//! ## Pipeline placement
//!
//! 1. **Type check** — [`super::check::expr`] recognizes postfix `?`, validates scrutinee type
//!    (`Option` or `Result`), and pushes [`TrySiteMeta`] keyed by expression [`super::types::ExprId`].
//! 2. **Lower** — [`crate::lower::expr`] reads the map and emits branch/return glue for
//!    [`TryFailureMode::ReturnScrutinee`] or monomorphized `From::from` for
//!    [`TryFailureMode::ConvertErr`].
//!
//! ## Failure modes
//!
//! | Mode | When | Lowering behavior |
//! |------|------|-------------------|
//! | [`TryFailureMode::ReturnScrutinee`] | Scrutinee and enclosing return use the same enum shape | Return scrutinee unchanged (V0-042) |
//! | [`TryFailureMode::ConvertErr`] | `Result` err types differ but `From` applies | Convert Err payload via monomorphized `From::from` (V0-059) |

use super::bindings::LocalSlot;
use super::types::TypeId;
use crate::resolver::DefId;

/// How the failure arm of `expr?` is lowered.
#[derive(Debug, Clone)]
pub enum TryFailureMode {
    /// V0-042: return scrutinee enum unchanged (Option or identical Result).
    ReturnScrutinee,
    /// V0-059: convert Err payload via monomorphized `From::from`.
    ConvertErr {
        /// Err payload type from the scrutinee `Result`.
        err_in_ty: TypeId,
        /// Err type of the enclosing function return `Result`.
        err_out_ty: TypeId,
        /// Enclosing function return type (`Result<T, E_out>`).
        return_result_ty: TypeId,
        /// `From::from` on `E_out: From<E_in>`.
        from_fn: DefId,
    },
}

/// Metadata for lowering `expr?` recorded during type-check.
#[derive(Debug, Clone)]
pub struct TrySiteMeta {
    /// Enum template definition (`Option` or `Result`).
    pub enum_def: DefId,
    /// Monomorphization type arguments for the scrutinee enum.
    pub enum_args: Vec<TypeId>,
    /// Scrutinee enum type (`Option<…>` or `Result<…, …>`).
    pub scrutinee_ty: TypeId,
    /// Unwrapped success payload type.
    pub success_ty: TypeId,
    /// Tag for `Some` / `Ok`.
    pub success_tag: u32,
    /// Tag for `None` / `Err`.
    pub failure_tag: u32,
    /// Local slot holding the scrutinee enum during `?` lowering.
    pub temp_slot: LocalSlot,
    /// Failure-arm lowering strategy.
    pub failure_mode: TryFailureMode,
}
