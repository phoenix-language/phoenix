//! VM intrinsics, extern calls, and unsafe fn call sites.
//!
//! Handles call forms that bypass ordinary static fn resolution: compiler intrinsics
//! ([`IntrinsicSite`]), `extern "C"` symbols, and indirect fn-pointer calls. Invoked from
//! [`super::expr`] when checking postfix call sites. Records lowering metadata in
//! [`IndirectCallMeta`] and intrinsic site maps, and enforces Phoenix unsafe-block rules at
//! each call site.
//!
//! # Responsibilities
//!
//! - **Unsafe call sites** — [`TypeChecker::check_unsafe_fn_call`] and
//!   [`TypeChecker::check_extern_call`] require an enclosing `unsafe` block when calling
//!   effective-unsafe fns or extern symbols.
//! - **Intrinsics** — [`TypeChecker::check_intrinsic_call`] validates arity and argument types for
//!   `alloc_bytes`, `dealloc_bytes`, `slice_from_raw_parts`, `slice_len`, and `size_of`, and
//!   records [`IntrinsicSite`] plus compile-time `size_of` literals where applicable.
//! - **Indirect calls** — [`TypeChecker::record_indirect_call`] tags fn-pointer and foreign calls
//!   with expected arity and signature ids for bytecode verification.
//!
//! # Key entry points
//!
//! | Function | Role |
//! |----------|------|
//! | [`TypeChecker::check_intrinsic_call`] | Type-check one intrinsic; populate site metadata. |
//! | [`TypeChecker::check_unsafe_fn_call`] | Emit error when an unsafe fn is called outside `unsafe`. |
//! | [`TypeChecker::check_extern_call`] | Emit error when an extern fn is called outside `unsafe`. |
//! | [`TypeChecker::record_indirect_call`] | Record fn-pointer call metadata for lowering/verify. |
//! | [`TypeChecker::is_static_fn_callee`] | True when callee is a resolved static fn body (not a pointer). |

use phx_diagnostics::{MismatchKind, Span, TypeCheckError};
use phx_syntax::ast::types::Type;
use phx_syntax::ast::{ExprNode, Node};

use super::TypeChecker;
use crate::resolver::{DefId, DefKind};
use crate::typeck::IndirectCallMeta;
use crate::typeck::builtins::{int_literal_type, u8_type};
use crate::typeck::intrinsic_kernel::IntrinsicSite;
use crate::typeck::primitive::primitive_kind_for_type;
use crate::typeck::subst::Substitution;
use crate::typeck::types::{ExprId, Ty, TypeId};

impl TypeChecker<'_> {
    /// Emits an error when an effective-unsafe fn is called outside an `unsafe` block.
    pub(in crate::typeck::check) fn check_unsafe_fn_call(&mut self, def: DefId, span: Span) {
        if !self.is_effective_unsafe(def) || self.unsafe_depth > 0 {
            return;
        }
        let Some(record) = self.resolved.defs.get(def.index() as usize) else {
            return;
        };
        self.bag.push(
            self.current_module,
            TypeCheckError::UnsafeFnCallRequiresUnsafe {
                name: self.symbol_name(record.name),
                span,
            },
        );
    }
    /// Returns whether `def` resolves to a static function body (not a fn pointer or extern stub).
    #[must_use]
    pub(in crate::typeck::check) fn is_static_fn_callee(&self, def: DefId) -> bool {
        self.resolved
            .defs
            .get(def.index() as usize)
            .is_some_and(|d| d.kind.is_function_body())
    }

    /// Emits an error when an `extern "C"` fn is called outside an `unsafe` block.
    pub(in crate::typeck::check) fn check_extern_call(&mut self, def: DefId, span: Span) {
        let Some(record) = self.resolved.defs.get(def.index() as usize) else {
            return;
        };
        if record.kind != DefKind::ExternFn || self.unsafe_depth > 0 {
            return;
        }
        self.bag.push(
            self.current_module,
            TypeCheckError::ExternCallRequiresUnsafe {
                name: self.symbol_name(record.name),
                span,
            },
        );
    }

    pub(in crate::typeck::check) fn check_alloc_bytes_args(
        &mut self,
        args: &[ExprNode],
        span: Span,
    ) {
        let u32_ty = int_literal_type(&mut self.types, true);
        if args.len() == 1 {
            let got = self.check_expr_node(&args[0]);
            if !self.types_equal(got, u32_ty) {
                self.error_mismatch(
                    u32_ty,
                    got,
                    args[0].span,
                    MismatchKind::Argument { index: 0 },
                );
            }
        } else {
            self.bag.push(
                self.current_module,
                TypeCheckError::ArityMismatch {
                    expected: 1,
                    found: args.len(),
                    span,
                },
            );
        }
    }

    pub(in crate::typeck::check) fn check_dealloc_bytes_args(
        &mut self,
        args: &[ExprNode],
        span: Span,
    ) {
        let u32_ty = int_literal_type(&mut self.types, true);
        let u8_ty = u8_type(&mut self.types);
        let ptr_ty = self.types.intern(&Ty::Ptr {
            mut_: true,
            inner: u8_ty,
        });
        if args.len() == 2 {
            let got_ptr = self.check_expr_node(&args[0]);
            if !self.types_equal(got_ptr, ptr_ty) {
                self.error_mismatch(
                    ptr_ty,
                    got_ptr,
                    args[0].span,
                    MismatchKind::Argument { index: 0 },
                );
            }
            let got_size = self.check_expr_node(&args[1]);
            if !self.types_equal(got_size, u32_ty) {
                self.error_mismatch(
                    u32_ty,
                    got_size,
                    args[1].span,
                    MismatchKind::Argument { index: 1 },
                );
            }
        } else {
            self.bag.push(
                self.current_module,
                TypeCheckError::ArityMismatch {
                    expected: 2,
                    found: args.len(),
                    span,
                },
            );
        }
    }

    /// Type-checks one compiler intrinsic call site.
    ///
    /// Dispatches on [`IntrinsicSite`], validates arguments, records the site in
    /// `intrinsic_call_sites`, and stores compile-time `size_of` byte counts when applicable.
    /// Most intrinsics require an enclosing `unsafe` block; `size_of` and `slice_len` do not.
    #[allow(clippy::too_many_lines)]
    pub(in crate::typeck::check) fn check_intrinsic_call(
        &mut self,
        site: IntrinsicSite,
        def: DefId,
        type_args: Option<&[Node<Type>]>,
        args: &[ExprNode],
        span: Span,
        expr_id: ExprId,
    ) -> TypeId {
        if site != IntrinsicSite::SizeOf
            && site != IntrinsicSite::SliceLen
            && self.unsafe_depth == 0
        {
            let name = self
                .resolved
                .defs
                .get(def.index() as usize)
                .map(|d| self.symbol_name(d.name))
                .unwrap_or_else(|| "intrinsic".to_owned());
            self.bag.push(
                self.current_module,
                TypeCheckError::IntrinsicRequiresUnsafe { name, span },
            );
        }
        match site {
            IntrinsicSite::AllocBytes => {
                let u8_ty = u8_type(&mut self.types);
                let ret = self.types.intern(&Ty::Ptr {
                    mut_: true,
                    inner: u8_ty,
                });
                self.check_alloc_bytes_args(args, span);
                self.intrinsic_call_sites.insert(expr_id, site);
                ret
            }
            IntrinsicSite::DeallocBytes => {
                self.check_dealloc_bytes_args(args, span);
                self.intrinsic_call_sites.insert(expr_id, site);
                self.unit
            }
            IntrinsicSite::SliceFromRawParts => {
                let u32_ty = int_literal_type(&mut self.types, true);
                let unit_ret = self.unit;
                if args.len() != 2 {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::ArityMismatch {
                            expected: 2,
                            found: args.len(),
                            span,
                        },
                    );
                    self.intrinsic_call_sites.insert(expr_id, site);
                    return unit_ret;
                }
                let ptr_ty = self.check_expr_node(&args[0]);
                let len_ty = self.check_expr_node(&args[1]);
                if !self.types_equal(len_ty, u32_ty) {
                    self.error_mismatch(
                        u32_ty,
                        len_ty,
                        args[1].span,
                        MismatchKind::Argument { index: 1 },
                    );
                }
                let Ty::Ptr { inner, .. } = self.types.get(ptr_ty).clone() else {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::Mismatch {
                            expected: "*mut T".to_owned(),
                            found: self.format_ty_diagnostic(ptr_ty),
                            span: args[0].span,
                            kind: MismatchKind::Argument { index: 0 },
                        },
                    );
                    self.intrinsic_call_sites.insert(expr_id, site);
                    return unit_ret;
                };
                if primitive_kind_for_type(&self.types, inner).is_none() {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::UnsupportedFeature {
                            feature: "heap slice over non-primitive element type",
                            span,
                        },
                    );
                    self.intrinsic_call_sites.insert(expr_id, site);
                    return unit_ret;
                }
                let ret = self.types.intern(&Ty::Slice(inner));
                self.intrinsic_call_sites.insert(expr_id, site);
                ret
            }
            IntrinsicSite::SliceLen => {
                let u32_ty = int_literal_type(&mut self.types, true);
                let unit_ret = self.unit;
                if args.len() != 1 {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::ArityMismatch {
                            expected: 1,
                            found: args.len(),
                            span,
                        },
                    );
                    self.intrinsic_call_sites.insert(expr_id, site);
                    return unit_ret;
                }
                let slice_ty = self.check_expr_node(&args[0]);
                let Ty::Slice(_) = self.types.get(slice_ty).clone() else {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::Mismatch {
                            expected: "[T]".to_owned(),
                            found: self.format_ty_diagnostic(slice_ty),
                            span: args[0].span,
                            kind: MismatchKind::Argument { index: 0 },
                        },
                    );
                    self.intrinsic_call_sites.insert(expr_id, site);
                    return unit_ret;
                };
                self.intrinsic_call_sites.insert(expr_id, site);
                u32_ty
            }
            IntrinsicSite::SizeOf => {
                let u32_ty = int_literal_type(&mut self.types, true);
                if !args.is_empty() {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::ArityMismatch {
                            expected: 0,
                            found: args.len(),
                            span,
                        },
                    );
                    self.intrinsic_call_sites.insert(expr_id, site);
                    return u32_ty;
                }
                let Some(type_args) = type_args else {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::UnsupportedFeature {
                            feature: "size_of requires an explicit type argument",
                            span,
                        },
                    );
                    self.intrinsic_call_sites.insert(expr_id, site);
                    return u32_ty;
                };
                if type_args.len() != 1 {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::UnsupportedFeature {
                            feature: "size_of expects exactly one type argument",
                            span,
                        },
                    );
                    self.intrinsic_call_sites.insert(expr_id, site);
                    return u32_ty;
                }
                let type_defs = self.type_defs.clone();
                let mut queried = self.lower_ast_type_with_defs(&type_args[0], &type_defs);
                if let Some(subst) = &self.subst {
                    queried = Substitution::apply(&mut self.types, queried, subst, self.resolved);
                    queried = self.mono_substitute_generic_param(queried, &[], None);
                }
                let Some(bytes) = crate::typeck::type_size::type_byte_size(
                    &self.types,
                    &self.program_layout,
                    self.resolved,
                    queried,
                ) else {
                    self.bag.push(
                        self.current_module,
                        TypeCheckError::UnsupportedFeature {
                            feature: "size_of for this type",
                            span: type_args[0].span,
                        },
                    );
                    self.intrinsic_call_sites.insert(expr_id, site);
                    return u32_ty;
                };
                self.size_of_literals.insert(expr_id, bytes);
                self.intrinsic_call_sites.insert(expr_id, site);
                u32_ty
            }
        }
    }

    /// Records metadata for an indirect (fn-pointer or foreign) call at `expr_id`.
    ///
    /// Skips static fn callees. Populates `indirect_call_sites` with expected arity and a stable
    /// signature id for bytecode verification.
    pub(in crate::typeck::check) fn record_indirect_call(
        &mut self,
        callee_ty: TypeId,
        callee_def: Option<DefId>,
        expr_id: ExprId,
        foreign: bool,
    ) {
        if callee_def.is_some_and(|d| self.is_static_fn_callee(d)) {
            return;
        }
        let Ty::Fn { params, .. } = self.types.get(callee_ty).clone() else {
            return;
        };
        let sig_type_id = self.alloc_fn_sig_type_id(callee_ty);
        let expected_arity = u32::try_from(params.len()).unwrap_or(u32::MAX);
        self.indirect_call_sites.insert(
            expr_id,
            IndirectCallMeta {
                sig_type_id,
                expected_arity,
                foreign,
            },
        );
    }
}
