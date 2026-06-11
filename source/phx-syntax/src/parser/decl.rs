//! Top-level and type declaration parsing.
//!
//! Handles `#import`, `pub`, `Name :: struct/enum/trait`, `type` aliases, `impl` blocks, and
//! function signatures. Module paths reuse lexer slices (`&str`) and intern them without copying.

use phx_diagnostics::ExpectedToken;

use crate::ast::Node;
use crate::ast::decl::{
    DeriveDirective, EnumVariant, FnDirective, Function, FunctionSig, ImplMember, ImportDirective,
    ImportItem, ImportItems, Param, StructBody, StructField, TopLevelDecl, TopLevelItem, TraitItem,
    Variant,
};
use crate::parser::Parser;
use crate::token::{Keyword, TokenKind};

impl Parser<'_> {
    /// Parses `#import path [:: { items }];`.
    pub(crate) fn parse_import(&mut self) -> Result<Node<ImportDirective>, ParseError> {
        let start = self.pos;
        self.expect_kind(ExpectedToken::Punct("#import"), &TokenKind::HashImport)?;
        let path = self.parse_module_path()?;
        let items = if self.eat_kind(&TokenKind::ColonColon) {
            self.expect_kind(ExpectedToken::Punct("{"), &TokenKind::LBrace)?;
            let list = self.parse_import_items()?;
            Some(list)
        } else {
            None
        };
        self.expect_semi()?;
        Ok(self.node(ImportDirective { path, items }, self.span_from(start)))
    }

    /// Parses `{ item, … }` or `{ * }` after `#import path ::`.
    fn parse_import_items(&mut self) -> Result<ImportItems, ParseError> {
        let mut items = Vec::new();
        if self.eat_kind(&TokenKind::RBrace) {
            return Ok(ImportItems { items });
        }
        loop {
            if self.eat_kind(&TokenKind::Star) {
                items.push(ImportItem::Glob);
            } else {
                items.push(ImportItem::Ident(self.parse_import_symbol()?));
            }
            if self.eat_kind(&TokenKind::RBrace) {
                break;
            }
            self.expect_kind(ExpectedToken::Punct(","), &TokenKind::Comma)?;
        }
        Ok(ImportItems { items })
    }

    /// Module path for `#import` (does not consume `::` before `{` import lists).
    fn parse_module_path(&mut self) -> Result<crate::ast::Path, ParseError> {
        let mut segments = Vec::new();
        loop {
            match self.peek_kind() {
                TokenKind::TypeIdent(seg) => {
                    let span = self.current_span();
                    self.bump();
                    segments.push(crate::ast::PathSegment::Type(
                        crate::ast::TypePathSegment::new(self.intern_type_name(seg, span)?),
                    ));
                }
                TokenKind::Ident(seg) => {
                    segments.push(crate::ast::PathSegment::Ident(self.bump_ident(seg)?));
                }
                _ => break,
            }
            if self.peek_kind() != TokenKind::ColonColon {
                break;
            }
            if matches!(self.peek_at(1), TokenKind::LBrace) {
                break;
            }
            self.bump();
        }
        if segments.is_empty() {
            return Err(self.error_unexpected(ExpectedToken::Ident));
        }
        Ok(crate::ast::Path { segments })
    }

    /// Parses a `::`-separated path (value and type segments).
    #[allow(dead_code)] // Used when qualified paths are unified with expression path parsing.
    pub(crate) fn parse_path(&mut self) -> Result<crate::ast::Path, ParseError> {
        let mut segments = Vec::new();
        loop {
            match self.peek_kind() {
                TokenKind::TypeIdent(seg) => {
                    let span = self.current_span();
                    self.bump();
                    segments.push(crate::ast::PathSegment::Type(
                        crate::ast::TypePathSegment::new(self.intern_type_name(seg, span)?),
                    ));
                }
                TokenKind::Ident(seg) => {
                    segments.push(crate::ast::PathSegment::Ident(self.bump_ident(seg)?));
                }
                _ => break,
            }
            if !self.eat_kind(&TokenKind::ColonColon) {
                break;
            }
        }
        if segments.is_empty() {
            return Err(self.error_unexpected(ExpectedToken::Ident));
        }
        Ok(crate::ast::Path { segments })
    }

    /// Parses one `pub`? top-level declaration followed by `;`.
    pub(crate) fn parse_top_level_item(&mut self) -> Result<Node<TopLevelItem>, ParseError> {
        let start = self.pos;
        let attrs = self.parse_attribute_list()?;
        let pub_ = self.eat_keyword(Keyword::Pub);
        let mut decl =
            if pub_ && (self.peek_keyword(Keyword::Reexport) || self.peek_keyword(Keyword::Mod)) {
                if self.peek_keyword(Keyword::Reexport) {
                    self.parse_reexport_decl()?
                } else {
                    self.parse_mod_decl()?
                }
            } else {
                self.parse_top_level_decl()?
            };
        self.merge_bracket_derives(&mut decl, &attrs);
        self.expect_semi()?;
        Ok(self.node(TopLevelItem { attrs, pub_, decl }, self.span_from(start)))
    }

    /// Parses `type`, `const`, `var`, `fn`, or `Name :: struct/enum/trait/impl`.
    fn parse_top_level_decl(&mut self) -> Result<TopLevelDecl, ParseError> {
        if self.peek_kind() == TokenKind::HashDerive {
            let derives = self.parse_derive_directives()?;
            if matches!(self.peek_kind(), TokenKind::TypeIdent(_)) {
                return self.parse_named_decl_with_derives(derives);
            }
            let func = self.parse_function_decl_with_derives(derives, false)?;
            return Ok(TopLevelDecl::Function(func));
        }
        if matches!(
            self.peek_kind(),
            TokenKind::HashInline | TokenKind::HashCold | TokenKind::HashHot
        ) || self.peek_keyword(Keyword::Unsafe)
        {
            let func = self.parse_function_decl_body(false)?;
            return Ok(TopLevelDecl::Function(func));
        }
        if self.peek_keyword(Keyword::Extern) {
            return self.parse_extern_decl();
        }
        match self.peek_kind() {
            TokenKind::Keyword(Keyword::Type) => self.parse_type_alias(),
            TokenKind::Keyword(Keyword::Mod) => self.parse_mod_decl(),
            TokenKind::Ident(_) => {
                let func = self.parse_function_decl_body(false)?;
                Ok(TopLevelDecl::Function(func))
            }
            TokenKind::TypeIdent(_) => self.parse_named_decl(),
            TokenKind::Keyword(Keyword::Const) => {
                self.bump();
                let name = self.parse_ident()?;
                let ty = if self.eat_kind(&TokenKind::Colon) {
                    Some(self.parse_type()?)
                } else {
                    None
                };
                self.expect_kind(ExpectedToken::Punct("="), &TokenKind::Eq)?;
                let init = self.parse_expr()?;
                Ok(TopLevelDecl::Const { name, ty, init })
            }
            TokenKind::Keyword(Keyword::Var) => {
                self.bump();
                let name = self.parse_ident()?;
                self.expect_kind(ExpectedToken::Punct(":"), &TokenKind::Colon)?;
                let ty = self.parse_type()?;
                self.expect_kind(ExpectedToken::Punct("="), &TokenKind::Eq)?;
                let init = self.parse_expr()?;
                Ok(TopLevelDecl::Var { name, ty, init })
            }
            _ => Err(self.error_unexpected(ExpectedToken::Token)),
        }
    }

    /// Parses `mod name` (visibility from enclosing `pub` on the item).
    fn parse_mod_decl(&mut self) -> Result<TopLevelDecl, ParseError> {
        self.expect_keyword(Keyword::Mod)?;
        let name = self.parse_ident()?;
        Ok(TopLevelDecl::Mod { name })
    }

    /// Parses `pub reexport :: path`.
    fn parse_reexport_decl(&mut self) -> Result<TopLevelDecl, ParseError> {
        self.expect_keyword(Keyword::Reexport)?;
        self.expect_kind(ExpectedToken::Punct("::"), &TokenKind::ColonColon)?;
        let path = self.parse_reexport_path()?;
        Ok(TopLevelDecl::Reexport { path })
    }

    /// Parses `item` or `child::item` after `reexport ::`.
    fn parse_reexport_path(&mut self) -> Result<crate::ast::Path, ParseError> {
        let mut segments = Vec::new();
        while let TokenKind::Ident(seg) = self.peek_kind() {
            segments.push(crate::ast::PathSegment::Ident(self.bump_ident(seg)?));
            if !self.eat_kind(&TokenKind::ColonColon) {
                break;
            }
        }
        if segments.is_empty() {
            return Err(self.error_unexpected(ExpectedToken::Ident));
        }
        Ok(crate::ast::Path { segments })
    }

    /// Parses `type Name [= generics] = Ty`.
    fn parse_type_alias(&mut self) -> Result<TopLevelDecl, ParseError> {
        self.bump();
        let name = self.parse_type_alias_name()?;
        let generics = if self.peek_kind() == TokenKind::Lt {
            Some(self.parse_generic_params()?)
        } else {
            None
        };
        self.expect_kind(ExpectedToken::Punct("="), &TokenKind::Eq)?;
        let ty = self.parse_type()?;
        Ok(TopLevelDecl::TypeAlias { name, generics, ty })
    }

    /// Parses `Name :: struct | enum | trait | impl`.
    fn parse_named_decl(&mut self) -> Result<TopLevelDecl, ParseError> {
        let derives = self.parse_derive_directives()?;
        self.parse_named_decl_with_derives(derives)
    }

    /// Parses `Name :: struct | enum | trait | impl` with leading `#derive` already consumed.
    fn parse_named_decl_with_derives(
        &mut self,
        derives: Vec<DeriveDirective>,
    ) -> Result<TopLevelDecl, ParseError> {
        let name = self.parse_type_name()?;
        self.expect_kind(ExpectedToken::Punct("::"), &TokenKind::ColonColon)?;
        let generics = if self.peek_kind() == TokenKind::Lt {
            Some(self.parse_generic_params()?)
        } else {
            None
        };
        match self.peek_kind() {
            TokenKind::Keyword(Keyword::Struct) => {
                self.bump();
                let body = self.parse_struct_body()?;
                Ok(TopLevelDecl::Struct {
                    name,
                    derives,
                    generics,
                    body,
                })
            }
            TokenKind::Keyword(Keyword::Enum) => {
                self.bump();
                let variants = self.parse_enum_variants()?;
                Ok(TopLevelDecl::Enum {
                    name,
                    derives,
                    generics,
                    variants,
                })
            }
            TokenKind::Keyword(Keyword::Trait) => {
                self.bump();
                let items = self.parse_trait_items(false)?;
                Ok(TopLevelDecl::Trait {
                    name,
                    derives,
                    generics,
                    unsafe_: false,
                    items,
                })
            }
            TokenKind::Keyword(Keyword::Unsafe)
                if matches!(
                    self.peek_at(1),
                    TokenKind::Keyword(Keyword::Trait | Keyword::Impl)
                ) =>
            {
                self.bump();
                match self.peek_kind() {
                    TokenKind::Keyword(Keyword::Trait) => {
                        self.bump();
                        let items = self.parse_trait_items(true)?;
                        Ok(TopLevelDecl::Trait {
                            name,
                            derives,
                            generics,
                            unsafe_: true,
                            items,
                        })
                    }
                    TokenKind::Keyword(Keyword::Impl) => {
                        self.bump();
                        let (trait_, members) = self.parse_impl_tail(generics.clone())?;
                        Ok(TopLevelDecl::Impl {
                            type_name: name,
                            generics,
                            unsafe_: true,
                            trait_,
                            members,
                        })
                    }
                    _ => Err(self.error_unexpected(ExpectedToken::Token)),
                }
            }
            TokenKind::Keyword(Keyword::Impl) => {
                self.bump();
                let (trait_, members) = self.parse_impl_tail(generics.clone())?;
                Ok(TopLevelDecl::Impl {
                    type_name: name,
                    generics,
                    unsafe_: false,
                    trait_,
                    members,
                })
            }
            TokenKind::Ident(_) => {
                let func = self.parse_function_decl_body(true)?;
                Ok(TopLevelDecl::Function(func))
            }
            _ => Err(self.error_unexpected(ExpectedToken::Token)),
        }
    }

    /// Parses struct body: `{ fields }`, `(t, …)`, or unit.
    fn parse_struct_body(&mut self) -> Result<StructBody, ParseError> {
        if self.eat_kind(&TokenKind::LBrace) {
            let fields = self.parse_struct_fields()?;
            return Ok(StructBody::Fields(fields));
        }
        if self.eat_kind(&TokenKind::LParen) {
            let mut types = Vec::new();
            if !self.eat_kind(&TokenKind::RParen) {
                types.push(self.parse_type()?);
                while self.eat_kind(&TokenKind::Comma) {
                    types.push(self.parse_type()?);
                }
                self.expect_kind(ExpectedToken::Punct(")"), &TokenKind::RParen)?;
            }
            for i in 0..types.len() {
                let _ = self.interner.intern(&i.to_string());
            }
            return Ok(StructBody::Tuple(types));
        }
        Ok(StructBody::Unit)
    }

    /// Parses `field: Ty, …` inside a struct definition.
    fn parse_struct_fields(&mut self) -> Result<Vec<StructField>, ParseError> {
        let mut fields = Vec::new();
        if self.eat_kind(&TokenKind::RBrace) {
            return Ok(fields);
        }
        loop {
            if self.eat_kind(&TokenKind::RBrace) {
                break;
            }
            let name = self.parse_ident()?;
            self.expect_kind(ExpectedToken::Punct(":"), &TokenKind::Colon)?;
            let ty = self.parse_type()?;
            fields.push(StructField { name, ty });
            if self.eat_kind(&TokenKind::RBrace) {
                break;
            }
            self.expect_kind(ExpectedToken::Punct(","), &TokenKind::Comma)?;
        }
        Ok(fields)
    }

    /// Parses `{ Variant, … }` for an enum.
    fn parse_enum_variants(&mut self) -> Result<Vec<EnumVariant>, ParseError> {
        self.expect_kind(ExpectedToken::Punct("{"), &TokenKind::LBrace)?;
        let mut variants = Vec::new();
        if self.eat_kind(&TokenKind::RBrace) {
            return Ok(variants);
        }
        loop {
            if self.eat_kind(&TokenKind::RBrace) {
                break;
            }
            variants.push(self.parse_enum_variant()?);
            if self.eat_kind(&TokenKind::RBrace) {
                break;
            }
            self.expect_kind(ExpectedToken::Punct(","), &TokenKind::Comma)?;
        }
        Ok(variants)
    }

    /// Parses one enum variant (unit, struct, or tuple form).
    fn parse_enum_variant(&mut self) -> Result<EnumVariant, ParseError> {
        let name = self.parse_type_name()?;
        let kind = if self.eat_kind(&TokenKind::LBrace) {
            let fields = self.parse_struct_fields()?;
            Variant::Struct(fields)
        } else if self.eat_kind(&TokenKind::LParen) {
            let mut types = Vec::new();
            if !self.eat_kind(&TokenKind::RParen) {
                types.push(self.parse_type()?);
                while self.eat_kind(&TokenKind::Comma) {
                    types.push(self.parse_type()?);
                }
                self.expect_kind(ExpectedToken::Punct(")"), &TokenKind::RParen)?;
            }
            Variant::Tuple(types)
        } else {
            Variant::Unit
        };
        Ok(EnumVariant { name, kind })
    }

    /// Parses trait members inside `{ … }`.
    fn parse_trait_items(
        &mut self,
        parent_trait_unsafe: bool,
    ) -> Result<Vec<TraitItem>, ParseError> {
        self.expect_kind(ExpectedToken::Punct("{"), &TokenKind::LBrace)?;
        let mut items = Vec::new();
        while !self.eat_kind(&TokenKind::RBrace) {
            if self.eat_keyword(Keyword::Type) {
                let span = self.current_span();
                let name = self.parse_type_name()?;
                self.expect_semi()?;
                items.push(TraitItem::AssociatedType(crate::ast::Ident {
                    symbol: name.symbol,
                    span,
                    id: name.id,
                }));
            } else {
                let sig = self.parse_function_sig(parent_trait_unsafe)?;
                self.expect_semi()?;
                items.push(TraitItem::Method(sig));
            }
        }
        Ok(items)
    }

    fn parse_impl_tail(
        &mut self,
        _generics: Option<Vec<crate::ast::GenericParam>>,
    ) -> Result<(Option<crate::ast::Node<crate::ast::Type>>, Vec<ImplMember>), ParseError> {
        let trait_ = if self.peek_kind() == TokenKind::ColonColon {
            self.bump();
            Some(self.parse_trait_bound()?)
        } else if self.peek_kind() == TokenKind::Keyword(Keyword::For) {
            self.bump();
            return Err(self.reject_unsupported(
                "trait impl uses 'Type :: impl :: Trait', not 'impl for Trait'",
            ));
        } else {
            None
        };
        self.expect_kind(ExpectedToken::Punct("{"), &TokenKind::LBrace)?;
        let mut members = Vec::new();
        while !self.eat_kind(&TokenKind::RBrace) {
            if self.eat_keyword(Keyword::Type) {
                let name = self.parse_type_name()?;
                self.expect_kind(ExpectedToken::Punct("="), &TokenKind::Eq)?;
                let ty = self.parse_type()?;
                self.expect_semi()?;
                members.push(ImplMember::AssociatedType { name, ty });
            } else {
                members.push(ImplMember::Method(self.parse_function_decl_body(false)?));
                self.expect_semi()?;
            }
        }
        Ok((trait_, members))
    }

    /// Parses a function signature (optional body for default impls).
    fn parse_function_sig(&mut self, parent_trait_unsafe: bool) -> Result<FunctionSig, ParseError> {
        let mut unsafe_ = false;
        if self.eat_keyword(Keyword::Unsafe) {
            if parent_trait_unsafe {
                return Err(self.reject_unsupported(
                    "redundant `unsafe` on method in `unsafe trait` (methods inherit unsafety)",
                ));
            }
            unsafe_ = true;
        }
        let name = self.parse_ident()?;
        self.expect_kind(ExpectedToken::Punct("::"), &TokenKind::ColonColon)?;
        let generics = if self.peek_kind() == TokenKind::Lt {
            Some(self.parse_generic_params()?)
        } else {
            None
        };
        let params = self.parse_params()?;
        let ret = self.parse_optional_return_type()?;
        let body = if self.peek_kind() == TokenKind::LBrace {
            Some(self.parse_block()?)
        } else {
            None
        };
        Ok(FunctionSig {
            name,
            unsafe_,
            generics,
            params,
            ret,
            body,
        })
    }

    /// Parses a full function (directives, name, sig, body).
    fn parse_function_decl_body(&mut self, name_only: bool) -> Result<Function, ParseError> {
        let attrs = self.parse_attribute_list()?;
        let mut derives = self.parse_derive_directives()?;
        derives.extend(self.derive_attrs_from_bracket(&attrs));
        self.parse_function_decl_with_derives(derives, name_only)
    }

    /// Parses a function after `#derive` directives are already consumed.
    fn parse_function_decl_with_derives(
        &mut self,
        mut derives: Vec<DeriveDirective>,
        name_only: bool,
    ) -> Result<Function, ParseError> {
        let attrs = if name_only {
            Vec::new()
        } else {
            self.parse_attribute_list()?
        };
        derives.extend(self.derive_attrs_from_bracket(&attrs));
        let directives = self.parse_fn_directives();
        let unsafe_ = self.eat_keyword(Keyword::Unsafe);
        let name = if name_only {
            self.parse_ident()?
        } else {
            let id = self.parse_ident()?;
            self.expect_kind(ExpectedToken::Punct("::"), &TokenKind::ColonColon)?;
            id
        };
        let generics = if self.peek_kind() == TokenKind::Lt {
            Some(self.parse_generic_params()?)
        } else {
            None
        };
        let params = self.parse_params()?;
        let ret = self.parse_optional_return_type()?;
        let body = self.parse_block()?;
        Ok(Function {
            attrs,
            derives,
            directives,
            unsafe_,
            name,
            generics,
            params,
            ret,
            body,
        })
    }

    /// Merges `#[derive(...)]` bracket attributes into `#derive` directive list.
    fn derive_attrs_from_bracket(
        &self,
        attrs: &[crate::ast::Node<crate::ast::Attribute>],
    ) -> Vec<DeriveDirective> {
        crate::attr_collect::derive_from_bracket_attrs(&self.interner, attrs)
    }

    fn merge_bracket_derives(
        &self,
        decl: &mut TopLevelDecl,
        attrs: &[crate::ast::Node<crate::ast::Attribute>],
    ) {
        let extra = self.derive_attrs_from_bracket(attrs);
        if extra.is_empty() {
            return;
        }
        match decl {
            TopLevelDecl::Struct { derives, .. }
            | TopLevelDecl::Enum { derives, .. }
            | TopLevelDecl::Trait { derives, .. } => derives.extend(extra),
            TopLevelDecl::Function(f) => f.derives.extend(extra),
            TopLevelDecl::TypeAlias { .. }
            | TopLevelDecl::Const { .. }
            | TopLevelDecl::Var { .. }
            | TopLevelDecl::Impl { .. }
            | TopLevelDecl::Mod { .. }
            | TopLevelDecl::Reexport { .. }
            | TopLevelDecl::ExternBlock { .. }
            | TopLevelDecl::ExternItem { .. } => {}
        }
    }

    fn parse_optional_return_type(
        &mut self,
    ) -> Result<Option<crate::ast::Node<crate::ast::Type>>, ParseError> {
        if !self.eat_kind(&TokenKind::FatArrow) {
            return Ok(None);
        }
        if self.peek_kind() == TokenKind::LBrace {
            return Ok(None);
        }
        Ok(Some(self.parse_type()?))
    }

    /// Parses leading `#inline` / `#cold` / `#hot` on a function.
    fn parse_fn_directives(&mut self) -> Vec<FnDirective> {
        let mut dirs = Vec::new();
        loop {
            match self.peek_kind() {
                TokenKind::HashInline => {
                    self.bump();
                    dirs.push(FnDirective::Inline);
                }
                TokenKind::HashCold => {
                    self.bump();
                    dirs.push(FnDirective::Cold);
                }
                TokenKind::HashHot => {
                    self.bump();
                    dirs.push(FnDirective::Hot);
                }
                _ => break,
            }
        }
        dirs
    }

    /// Parses leading `#derive(Trait, …)` attributes.
    pub(crate) fn parse_derive_directives(&mut self) -> Result<Vec<DeriveDirective>, ParseError> {
        let mut derives = Vec::new();
        while self.peek_kind() == TokenKind::HashDerive {
            self.bump();
            self.expect_kind(ExpectedToken::Punct("("), &TokenKind::LParen)?;
            let mut traits = vec![self.parse_type_name()?];
            while self.eat_kind(&TokenKind::Comma) {
                traits.push(self.parse_type_name()?);
            }
            self.expect_kind(ExpectedToken::Punct(")"), &TokenKind::RParen)?;
            derives.push(DeriveDirective { traits });
        }
        Ok(derives)
    }

    /// Parses `(param, …)`.
    fn parse_params(&mut self) -> Result<Vec<Param>, ParseError> {
        self.expect_kind(ExpectedToken::Punct("("), &TokenKind::LParen)?;
        let mut params = Vec::new();
        if self.eat_kind(&TokenKind::RParen) {
            return Ok(params);
        }
        loop {
            params.push(self.parse_param()?);
            if self.eat_kind(&TokenKind::RParen) {
                break;
            }
            self.expect_kind(ExpectedToken::Punct(","), &TokenKind::Comma)?;
        }
        Ok(params)
    }

    /// Parses one parameter (`self` or `name: Ty`).
    pub(crate) fn parse_param(&mut self) -> Result<Param, ParseError> {
        if matches!(
            self.peek_kind(),
            TokenKind::Keyword(Keyword::Mut | Keyword::SelfLower)
        ) {
            let mut_ = self.eat_keyword(Keyword::Mut);
            self.eat_keyword(Keyword::SelfLower);
            let ty = if self.eat_kind(&TokenKind::Colon) {
                Some(self.parse_type()?)
            } else {
                None
            };
            return Ok(Param::Receiver { mut_, ty });
        }
        let name = self.parse_ident()?;
        self.expect_kind(ExpectedToken::Punct(":"), &TokenKind::Colon)?;
        let ty = self.parse_type()?;
        Ok(Param::Named { name, ty })
    }

    /// Parses `extern "ABI" { sig; … }` or `extern "ABI" sig`.
    fn parse_extern_decl(&mut self) -> Result<TopLevelDecl, ParseError> {
        self.eat_keyword(Keyword::Extern);
        let abi = self.parse_abi_string()?;
        if self.peek_kind() == TokenKind::LBrace {
            self.bump();
            let mut items = Vec::new();
            while !self.eat_kind(&TokenKind::RBrace) {
                items.push(self.parse_extern_function_sig()?);
                self.expect_semi()?;
            }
            Ok(TopLevelDecl::ExternBlock { abi, items })
        } else {
            let sig = self.parse_extern_function_sig()?;
            Ok(TopLevelDecl::ExternItem { abi, sig })
        }
    }

    fn parse_abi_string(&mut self) -> Result<String, ParseError> {
        match self.peek_kind() {
            TokenKind::String(s) => {
                self.bump();
                Ok(s)
            }
            _ => Err(self.error_unexpected(ExpectedToken::Literal)),
        }
    }

    fn parse_extern_function_sig(&mut self) -> Result<FunctionSig, ParseError> {
        let name = self.parse_ident()?;
        self.expect_kind(ExpectedToken::Punct("::"), &TokenKind::ColonColon)?;
        if self.peek_kind() == TokenKind::Lt {
            return Err(self.error_unexpected(ExpectedToken::Punct("(")));
        }
        let params = self.parse_params()?;
        let ret = self.parse_optional_return_type()?;
        Ok(FunctionSig {
            name,
            unsafe_: false,
            generics: None,
            params,
            ret,
            body: None,
        })
    }
}

use phx_diagnostics::ParseError;
