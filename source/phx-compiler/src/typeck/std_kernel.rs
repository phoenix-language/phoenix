//! `?` operator lowering metadata.
//!
//! Records [`TrySiteMeta`] per postfix `?` expression so lowering can emit the correct failure
//! arm (return scrutinee or `From::from` conversion).

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
