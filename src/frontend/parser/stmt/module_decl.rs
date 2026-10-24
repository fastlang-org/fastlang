use crate::frontend::lexer::token::TokenKind;
use crate::frontend::parser::ast::*;
use crate::frontend::parser::ast::type_from_expr;
use crate::frontend::parser::parser::Parser;

impl Parser {
    pub(crate) fn parse_import_stmt(&mut self) -> Result<Decl, String> {
        self.advance(); // consume 'import'

        // Check for ABI prefix e.g. import @c "stdio.h"; or import @cpp "vector";
        let mut abi: Option<String> = None;
        if let TokenKind::LabelName(lbl) = &self.peek().kind {
            if lbl.eq_ignore_ascii_case("@c") {
                self.advance();
                abi = Some("C".to_string());
            } else if lbl.eq_ignore_ascii_case("@cpp") {
                self.advance();
                abi = Some("Cpp".to_string());
            }
        }

        // Case 1: String import (e.g. import "stdio.h" or import "std/string")
        if let TokenKind::String(path) = &self.peek().kind {
            let mod_path = path.clone();
            self.advance();

            // Check if ABI is specified at the end e.g. import "stdio.h" @c;
            if abi.is_none() {
                if let TokenKind::LabelName(lbl) = &self.peek().kind {
                    if lbl.eq_ignore_ascii_case("@c") {
                        self.advance();
                        abi = Some("C".to_string());
                    } else if lbl.eq_ignore_ascii_case("@cpp") {
                        self.advance();
                        abi = Some("Cpp".to_string());
                    }
                }
            }

            let alias = if self.peek().kind == TokenKind::As {
                self.advance();
                Some(self.get_identifier("Expected alias identifier after 'as'")?)
            } else {
                None
            };

            self.consume(TokenKind::SemiColon, "Expected ';' after import statement")?;
            return Ok(Decl::Import {
                module_path: vec![mod_path],
                imports: None,
                abi,
                alias,
            });
        }

        let mut module_path: Vec<String> = Vec::new();
        let mut imports: Option<Vec<String>> = None;

        module_path.push(self.get_identifier("Expected module name after 'import'")?);

        while self.peek().kind == TokenKind::DoubleColon {
            self.advance();

            if self.peek().kind == TokenKind::LBrace {
                self.advance();
                let mut selected: Vec<String> = Vec::new();

                if !self.is_at_end() && self.peek().kind != TokenKind::RBrace {
                    loop {
                        selected.push(self.get_identifier("Expected import name in import list")?);

                        if self.peek().kind == TokenKind::Comma {
                            self.advance();
                            continue;
                        }
                        break;
                    }
                }

                self.consume(TokenKind::RBrace, "Expected '}' after import list")?;

                imports = Some(selected);
                break;
            } else {
                module_path.push(self.get_identifier("Expected module name after '::'")?);
            }
        }

        let alias = if self.peek().kind == TokenKind::As {
            self.advance();
            Some(self.get_identifier("Expected alias identifier after 'as'")?)
        } else {
            None
        };

        self.consume(TokenKind::SemiColon, "Expected ';' after import statement")?;

        Ok(Decl::Import {
            module_path,
            imports,
            abi,
            alias,
        })
    }

    pub(crate) fn parse_extern_decl(&mut self) -> Result<Decl, String> {
        self.consume(TokenKind::Extern, "Expected 'extern'")?;
        let abi = match &self.peek().kind {
            TokenKind::LabelName(lbl) if lbl.eq_ignore_ascii_case("@c") => {
                self.advance();
                "C".to_string()
            }
            TokenKind::LabelName(lbl) if lbl.eq_ignore_ascii_case("@cpp") => {
                self.advance();
                "Cpp".to_string()
            }
            _ => "C".to_string(),
        };

        let mut header: Option<String> = None;
        if let TokenKind::String(hdr) = &self.peek().kind {
            header = Some(hdr.clone());
            self.advance();
        }

        if self.peek().kind == TokenKind::DoubleColon {
            self.advance(); // consume '::'
        } else if self.peek().kind == TokenKind::LBrace {
            return Err("Syntax Error: Expected '::' after extern ABI (e.g. 'extern @c \"header.h\" :: { ... }' or 'extern @c :: { ... }')".to_string());
        }

        if header.is_none() && self.peek().kind == TokenKind::LBrace {
            eprintln!("Warning: 'extern @{} :: {{ ... }}' does not specify a target header file. It is recommended to specify the header file (e.g. 'extern @{} \"header.h\" :: {{ ... }}') to prevent header ambiguity.", abi.to_lowercase(), abi.to_lowercase());
        }

        if self.peek().kind == TokenKind::LBrace {
            self.advance(); // consume '{'
            let mut decls = Vec::new();
            while !self.is_at_end() && self.peek().kind != TokenKind::RBrace {
                if self.peek().kind == TokenKind::Set {
                    return Err("Syntax Error: 'set' keyword is not allowed in 'extern' blocks; only 'fn', 'struct', 'enum', or 'type' are allowed for foreign declarations.".to_string());
                } else if self.peek().kind == TokenKind::Fn {
                    let fn_decl = self.parse_extern_fn_signature(&abi)?;
                    decls.push(fn_decl);
                } else if self.peek().kind == TokenKind::TypeStruct {
                    let struct_decl = self.parse_struct_decl()?;
                    if self.peek().kind == TokenKind::SemiColon {
                        self.advance();
                    }
                    decls.push(struct_decl);
                } else if self.peek().kind == TokenKind::TypeEnum {
                    let enum_decl = self.parse_enum_decl()?;
                    if self.peek().kind == TokenKind::SemiColon {
                        self.advance();
                    }
                    decls.push(enum_decl);
                } else if self.peek().kind == TokenKind::TypeType {
                    self.advance(); // consume 'type'
                    let t_name = self.get_identifier("Expected type name after 'type' in extern block")?;
                    let type_alias = if self.peek().kind == TokenKind::Assign {
                        self.advance();
                        Some(self.parse_type()?)
                    } else {
                        None
                    };
                    self.consume(TokenKind::SemiColon, "Expected ';' after type declaration in extern block")?;
                    let decl = Decl::DefineDecl {
                        visibility: Visibility::Public,
                        name: t_name,
                        type_alias,
                        value: None,
                    };
                    decls.push(decl);
                } else {
                    return Err(format!(
                        "Syntax Error: Unexpected token '{:?}' in extern block",
                        self.peek().kind
                    ));
                }
            }
            self.consume(TokenKind::RBrace, "Expected '}' at end of extern block")?;
            Ok(Decl::ExternBlockDecl { abi, header, decls })
        } else if self.peek().kind == TokenKind::Fn {
            self.parse_extern_fn_signature(&abi)
        } else {
            Err(format!(
                "Syntax Error: Expected ':: {{' or 'fn' after extern, found '{:?}'",
                self.peek().kind
            ))
        }
    }

    pub(crate) fn parse_single_param(&mut self) -> Result<Param, String> {
        let mut is_variadic = if self.peek().kind == TokenKind::DotDotDot {
            self.advance(); // consume '...'
            true
        } else {
            false
        };

        let is_anonymous = if is_variadic {
            let next_kind = self.peek_ahead(1).map(|t| &t.kind);
            next_kind != Some(&TokenKind::Colon) && next_kind != Some(&TokenKind::Walrus)
        } else {
            match &self.peek().kind {
                TokenKind::TypeInt(_)
                | TokenKind::TypeUInt(_)
                | TokenKind::TypeFloat(_)
                | TokenKind::TypeUSize
                | TokenKind::TypeISize
                | TokenKind::TypeChar
                | TokenKind::TypeUChar
                | TokenKind::TypeStr
                | TokenKind::TypeUStr
                | TokenKind::TypeBool
                | TokenKind::TypeVoid
                | TokenKind::TypeType
                | TokenKind::TypeAuto => {
                    let next_kind = self.peek_ahead(1).map(|t| &t.kind);
                    next_kind != Some(&TokenKind::Colon) && next_kind != Some(&TokenKind::Walrus)
                }
                TokenKind::Identifier(_) => {
                    let next_kind = self.peek_ahead(1).map(|t| &t.kind);
                    next_kind != Some(&TokenKind::Colon) && next_kind != Some(&TokenKind::Walrus)
                }
                _ => false,
            }
        };

        if is_anonymous {
            let anon_idx = self.current;
            let type_node = self.parse_type()?;
            return Ok(Param {
                name: format!("__anon_param_{}", anon_idx),
                type_node,
                default_value: None,
                is_variadic,
            });
        }

        let err_msg = format!(
            "Expected parameter name at line {}, column {}, found '{:?}'",
            self.peek().line,
            self.peek().column,
            self.peek().kind
        );
        let param_name = if self.peek().kind == TokenKind::Underscore {
            self.advance();
            format!("__unused_{}", self.current)
        } else {
            self.get_identifier(&err_msg)?
        };
        let (type_node, default_value) = if self.peek().kind == TokenKind::Comma || self.peek().kind == TokenKind::RParen {
            (BaseType::Auto, None)
        } else if self.peek().kind == TokenKind::Walrus {
            self.advance(); // consume ':='
            let val_expr = self.parse_expression()?;
            let t = type_from_expr(&val_expr);
            (t, Some(val_expr))
        } else if self.peek().kind == TokenKind::Colon {
            self.advance(); // consume ':'
            if self.peek().kind == TokenKind::DotDotDot {
                self.advance(); // consume '...' if written like args: ...T
                is_variadic = true;
            }
            let t = self.parse_type()?;
            let default_val =
                if self.peek().kind == TokenKind::Assign || self.peek().kind == TokenKind::Walrus {
                    self.advance();
                    Some(self.parse_expression()?)
                } else {
                    None
                };
            (t, default_val)
        } else {
            return Err("Expected ':' or ':=' after parameter name".to_string());
        };
        Ok(Param {
            name: param_name,
            type_node,
            default_value,
            is_variadic,
        })
    }

    pub(crate) fn parse_params_entry(&mut self) -> Result<Vec<Param>, String> {
        if self.peek().kind == TokenKind::LParen {
            self.advance(); // consume '('
            let mut names = Vec::new();
            while self.peek().kind != TokenKind::RParen {
                let name = if self.peek().kind == TokenKind::Underscore {
                    self.advance();
                    format!("__unused_{}", self.current)
                } else {
                    self.get_identifier("Expected parameter name in grouped parameters")?
                };
                names.push(name);
                if self.peek().kind == TokenKind::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
            self.consume(TokenKind::RParen, "Expected ')' after grouped parameter names")?;

            let (type_node, default_value) = if self.peek().kind == TokenKind::Walrus {
                self.advance();
                let expr = self.parse_expression()?;
                (type_from_expr(&expr), Some(expr))
            } else if self.peek().kind == TokenKind::Colon {
                self.advance();
                let t = self.parse_type()?;
                let def = if self.peek().kind == TokenKind::Assign || self.peek().kind == TokenKind::Walrus {
                    self.advance();
                    Some(self.parse_expression()?)
                } else {
                    None
                };
                (t, def)
            } else {
                return Err("Expected ':' or ':=' after grouped parameters '(a, b)'".to_string());
            };

            let mut out = Vec::new();
            for n in names {
                out.push(Param {
                    name: n,
                    type_node: type_node.clone(),
                    default_value: default_value.clone(),
                    is_variadic: false,
                });
            }
            return Ok(out);
        }
        let p = self.parse_single_param()?;
        Ok(vec![p])
    }

    pub(crate) fn parse_extern_fn_signature(&mut self, abi: &str) -> Result<Decl, String> {
        self.consume(TokenKind::Fn, "Expected 'fn'")?;
        let name = self.get_identifier("Expected function name in extern declaration")?;
        let mut generics: Vec<BaseType> = Vec::new();
        if self.peek().kind == TokenKind::Less {
            self.parse_generics(&mut generics)?;
        }
        self.consume(TokenKind::LParen, "Expected '(' after function name")?;
        let mut params = Vec::new();
        if self.peek().kind != TokenKind::RParen {
            loop {
                let p = self.parse_single_param()?;
                params.push(p);
                if self.peek().kind == TokenKind::Comma {
                    self.advance();
                    continue;
                }
                break;
            }
        }
        self.consume(TokenKind::RParen, "Expected ')' after parameters")?;
        let return_type = if self.peek().kind == TokenKind::Arrow {
            self.advance();
            self.parse_return_type()?
        } else {
            BaseType::Void
        };
        let alias = if self.peek().kind == TokenKind::As {
            self.advance();
            Some(self.get_identifier("Expected alias identifier after 'as'")?)
        } else {
            None
        };
        self.consume(
            TokenKind::SemiColon,
            "Expected ';' after extern function declaration",
        )?;
        Ok(Decl::ExternFnDecl {
            abi: abi.to_string(),
            name,
            generics,
            params,
            return_type,
            alias,
        })
    }

    pub(crate) fn parse_using_stmt(&mut self) -> Result<Stmt, String> {
        self.consume(TokenKind::Using, "Expected 'using'")?;
        let name = self.get_identifier("Expected identifier after 'using'")?;
        self.consume(TokenKind::SemiColon, "Expected ';' after using statement")?;
        Ok(Stmt::UsingStmt(name))
    }

    pub(crate) fn parse_define_stmt(&mut self, is_public: bool) -> Result<Decl, String> {
        self.consume(TokenKind::Define, "Expected 'define'")?;
        let name = match &self.peek().kind {
            TokenKind::Identifier(id) => {
                let s = id.clone();
                self.advance();
                s
            }
            TokenKind::TypeStr => {
                self.advance();
                "str".to_string()
            }
            TokenKind::TypeUStr => {
                self.advance();
                "ustr".to_string()
            }
            _ => return Err("Expected identifier after 'define'".to_string()),
        };
        if self.peek().kind == TokenKind::Assign || self.peek().kind == TokenKind::Arrow {
            self.advance();
        } else {
            return Err("Expected '=' or '->' after define identifier".to_string());
        }

        let is_type = match &self.peek().kind {
            TokenKind::TypeInt(_)
            | TokenKind::TypeUInt(_)
            | TokenKind::TypeUSize
            | TokenKind::TypeISize
            | TokenKind::TypeFloat(_)
            | TokenKind::TypeChar
            | TokenKind::TypeUChar
            | TokenKind::TypeStr
            | TokenKind::TypeUStr
            | TokenKind::TypeBool
            | TokenKind::TypeVoid
            | TokenKind::TypeStruct
            | TokenKind::TypeClass
            | TokenKind::TypeBluePrint
            | TokenKind::TypeEnum
            | TokenKind::TypeMethod
            | TokenKind::TypeFn
            | TokenKind::TypeLambda
            | TokenKind::TypeAuto
            | TokenKind::TypeBlock => true,
            TokenKind::Identifier(id) => {
                self.peek_ahead(1)
                    .map(|t| t.kind == TokenKind::Less || t.kind == TokenKind::LBracket)
                    .unwrap_or(false)
                    || self.metadata.contains_key(id)
            }
            _ => false,
        };

        let visibility = if is_public {
            Visibility::Public
        } else {
            Visibility::Private
        };

        if is_type {
            let t = self.parse_type()?;
            self.type_aliases.insert(name.clone(), t.clone());
            self.consume(TokenKind::SemiColon, "Expected ';' after define type alias")?;
            Ok(Decl::DefineDecl {
                visibility,
                name,
                type_alias: Some(t),
                value: None,
            })
        } else {
            let val = self.parse_expression()?;
            self.consume(TokenKind::SemiColon, "Expected ';' after define value")?;
            Ok(Decl::DefineDecl {
                visibility,
                name,
                type_alias: None,
                value: Some(val),
            })
        }
    }

    pub(crate) fn parse_exported_stmt(&mut self) -> Result<Decl, String> {
        self.advance(); // consume 'export' or 'public'

        // After export/public, we expect a valid exportable statement (fn, block, class, struct, enum, micro, blueprint, machine, define, or var)
        let mut stmt = match &self.peek().kind {
            TokenKind::Fn | TokenKind::TypeMethod => self.parse_fn_decl()?,
            TokenKind::TypeClass => self.parse_class_decl()?,
            TokenKind::TypeStruct => self.parse_struct_decl()?,
            TokenKind::TypeEnum => self.parse_enum_decl()?,
            TokenKind::TypeMacro => self.parse_macro_decl()?,
            TokenKind::TypeBluePrint => self.parse_blueprint_decl()?,
            TokenKind::TypeMachine => self.parse_machine_decl()?,
            TokenKind::TypeBlock => self.parse_block_scope_decl()?,
            TokenKind::Define => self.parse_define_stmt(true)?,
            TokenKind::Impl => self.parse_impl_decl()?,
            TokenKind::LabelName(lbl)
                if lbl.eq_ignore_ascii_case("@compile")
                    || lbl.eq_ignore_ascii_case("@sys") =>
            {
                self.parse_directive_decl()?
            }
            TokenKind::Identifier(_) if self.is_namespace_decl_start() => {
                self.parse_namespace_decl()?
            }
            TokenKind::Identifier(_) if self.is_block_decl_start() => {
                self.parse_bare_block_decl()?
            }
            _ if self.is_var_decl_start() => self.parse_var_decl(ScopeType::Global)?,
            kind => {
                return Err(
                    format!(
                        "Syntax Error: Cannot make '{:?}' public, only fn, block, class, struct, enum, blueprint, machine, define, micro, impl, and variables can be public",
                        kind
                    )
                );
            }
        };

        // Set visibility to Public
        match &mut stmt {
            Decl::FnDecl {
                ref mut visibility, ..
            } => {
                *visibility = Visibility::Public;
            }
            Decl::BlockDecl {
                ref mut visibility, ..
            } => {
                *visibility = Visibility::Public;
            }
            Decl::ClassDecl {
                ref mut visibility, ..
            } => {
                *visibility = Visibility::Public;
            }
            Decl::StructDecl {
                ref mut visibility, ..
            } => {
                *visibility = Visibility::Public;
            }
            Decl::BlueprintDecl {
                ref mut visibility, ..
            } => {
                *visibility = Visibility::Public;
            }
            Decl::EnumDecl {
                ref mut visibility, ..
            } => {
                *visibility = Visibility::Public;
            }
            Decl::MacroDecl {
                ref mut visibility, ..
            } => {
                *visibility = Visibility::Public;
            }
            Decl::NamespaceDecl {
                ref mut visibility, ..
            } => {
                *visibility = Visibility::Public;
            }
            Decl::MachineDecl {
                ref mut visibility, ..
            } => {
                *visibility = Visibility::Public;
            }
            Decl::DefineDecl {
                ref mut visibility, ..
            } => {
                *visibility = Visibility::Public;
            }
            Decl::CompileDecl { ref mut decls, .. } => {
                for d in decls {
                    match d {
                        Decl::FnDecl {
                            ref mut visibility, ..
                        }
                        | Decl::BlockDecl {
                            ref mut visibility, ..
                        }
                        | Decl::ClassDecl {
                            ref mut visibility, ..
                        }
                        | Decl::StructDecl {
                            ref mut visibility, ..
                        }
                        | Decl::BlueprintDecl {
                            ref mut visibility, ..
                        }
                        | Decl::EnumDecl {
                            ref mut visibility, ..
                        }
                        | Decl::MacroDecl {
                            ref mut visibility, ..
                        }
                        | Decl::MachineDecl {
                            ref mut visibility, ..
                        }
                        | Decl::DefineDecl {
                            ref mut visibility, ..
                        }
                        | Decl::VarDecl {
                            ref mut visibility, ..
                        }
                        | Decl::DestructureDecl {
                            ref mut visibility, ..
                        } => {
                            *visibility = Visibility::Public;
                        }
                        _ => {}
                    }
                }
            }
            Decl::VarDecl {
                ref mut visibility, ..
            } => {
                *visibility = Visibility::Public;
            }
            Decl::ImplDecl { .. } => {}
            Decl::DestructureDecl {
                ref mut visibility, ..
            } => {
                *visibility = Visibility::Public;
            }
            Decl::ArrayDecl {
                ref mut visibility, ..
            } => {
                *visibility = Visibility::Public;
            }
            Decl::ObjectDecl {
                ref mut visibility, ..
            } => {
                *visibility = Visibility::Public;
            }
            _ => {}
        }

        Ok(stmt)
    }

    pub(crate) fn is_namespace_decl_start(&self) -> bool {
        if let TokenKind::Identifier(_) = &self.peek().kind {
            if let Some(next) = self.peek_ahead(1) {
                if next.kind == TokenKind::DoubleColon {
                    if let Some(after) = self.peek_ahead(2) {
                        return after.kind == TokenKind::LBrace;
                    }
                }
            }
        }
        false
    }

    pub(crate) fn parse_namespace_decl(&mut self) -> Result<Decl, String> {
        let name = self.get_identifier("Expected namespace name")?;
        self.consume(TokenKind::DoubleColon, "Expected '::' after namespace name")?;
        self.consume(TokenKind::LBrace, "Expected '{' to open namespace body")?;
        let mut decls = Vec::new();
        while !self.is_at_end() && self.peek().kind != TokenKind::RBrace {
            let start_pos = self.current;
            match self.parse_statement(ScopeType::Global)? {
                Some(Stmt::Declaration(d)) => decls.push(d),
                Some(_) => {
                    return Err(
                        "Syntax Error: Only declarations (functions, types, namespaces, micros, variables) are allowed directly inside a namespace".to_string(),
                    );
                }
                None => {
                    if self.current == start_pos && !self.is_at_end() && self.peek().kind != TokenKind::RBrace {
                        self.advance();
                    }
                }
            }
        }
        self.consume(TokenKind::RBrace, "Expected '}' to close namespace body")?;
        if self.peek().kind == TokenKind::SemiColon {
            self.advance();
        }
        Ok(Decl::NamespaceDecl {
            visibility: Visibility::Private,
            name,
            decls,
        })
    }

    pub(crate) fn is_block_decl_start(&self) -> bool {
        if let TokenKind::Identifier(_) = &self.peek().kind {
            if let Some(next) = self.peek_ahead(1) {
                if next.kind == TokenKind::LBrace {
                    return true;
                }
                if next.kind == TokenKind::Arrow {
                    let mut i = 2;
                    while let Some(tok) = self.peek_ahead(i) {
                        if tok.kind == TokenKind::LBrace {
                            return true;
                        }
                        if tok.kind == TokenKind::SemiColon
                            || tok.kind == TokenKind::Assign
                            || tok.kind == TokenKind::EOF
                        {
                            return false;
                        }
                        i += 1;
                    }
                }
            }
        }
        false
    }

    pub(crate) fn parse_bare_block_decl(&mut self) -> Result<Decl, String> {
        let name = self.get_identifier("Expected block name")?;
        let mut return_type = None;
        if self.peek().kind == TokenKind::Arrow {
            self.advance(); // consume '->'
            if self.peek().kind != TokenKind::LBrace {
                return_type = Some(self.parse_type()?);
            }
        }
        self.consume(TokenKind::LBrace, "Expected '{' to open block body")?;
        let mut statements = Vec::new();
        while !self.is_at_end() && self.peek().kind != TokenKind::RBrace {
            match self.parse_statement(ScopeType::Block) {
                Ok(Some(stmt)) => statements.push(stmt),
                Ok(None) => {
                    if !self.is_at_end() && self.peek().kind != TokenKind::RBrace {
                        self.advance();
                    }
                }
                Err(err) => return Err(err),
            }
        }
        self.consume(TokenKind::RBrace, "Expected '}' to close block body")?;
        if self.peek().kind == TokenKind::SemiColon {
            self.advance();
        }
        Ok(Decl::BlockDecl {
            visibility: Visibility::Private,
            name,
            return_type,
            statements,
        })
    }
}
