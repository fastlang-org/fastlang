use crate::frontend::lexer::token::TokenKind;
use crate::frontend::parser::ast::*;
use crate::frontend::parser::parser::Parser;

impl Parser {
    pub(crate) fn parse_blueprint_decl(&mut self) -> Result<Decl, String> {
        if self.peek().kind == TokenKind::TypeBluePrint {
            self.advance();
        }
        let name = self.get_identifier("Expected blueprint name")?;

        let mut generics = Vec::new();
        if self.peek().kind == TokenKind::Less {
            self.parse_generics(&mut generics)?;
        }
        for g in &generics {
            let gen_name = g.as_str();
            self.current_generics.insert(gen_name.clone());
            if let Some(stripped) = gen_name.strip_prefix("...") {
                self.current_generics.insert(stripped.to_string());
            }
        }

        if self.peek().kind == TokenKind::Arrow || self.peek().kind == TokenKind::Assign {
            self.advance();
        }

        let mut share_directive: Option<BlueprintShareDirective> = None;
        let definition = if self.peek().kind == TokenKind::LBrace {
            self.advance();
            // Parse Explicit definition: { int(32) x; int(32) y; } or { x := default; str y; }
            let mut fields = Vec::new();
            while self.peek().kind != TokenKind::RBrace && !self.is_at_end() {
                if self.peek().kind == TokenKind::Share {
                    self.advance(); // consume 'share'
                    let target_name =
                        self.get_identifier("Expected target field name after 'share'")?;
                    let condition = if self.peek().kind == TokenKind::If {
                        self.advance(); // consume 'if'
                        self.consume(TokenKind::LParen, "Expected '(' after 'if'")?;
                        let expr = self.parse_expression()?;
                        self.consume(TokenKind::RParen, "Expected ')' after share condition")?;
                        Some(expr)
                    } else {
                        None
                    };
                    let then_block = if self.peek().kind == TokenKind::LBrace {
                        self.advance(); // consume '{'
                        let mut stmts = Vec::new();
                        while !self.is_at_end() && self.peek().kind != TokenKind::RBrace {
                            if let Some(stmt) = self.parse_statement(ScopeType::Block)? {
                                stmts.push(stmt);
                            }
                        }
                        self.consume(TokenKind::RBrace, "Expected '}' at end of share then-block")?;
                        Some(stmts)
                    } else {
                        None
                    };
                    if self.peek().kind == TokenKind::SemiColon {
                        self.advance();
                    }
                    share_directive = Some(BlueprintShareDirective {
                        target_field: target_name,
                        condition,
                        then_block,
                    });
                    continue;
                }

                let mut is_static = false;
                if self.peek().kind == TokenKind::Static {
                    self.advance();
                    is_static = true;
                }

                let is_colon_or_walrus = if let Some(next_tok) = self.tokens.get(self.current + 1) {
                    next_tok.kind == TokenKind::Colon || next_tok.kind == TokenKind::Walrus
                } else {
                    false
                };

                let (field_name, type_node, default_value) = if is_colon_or_walrus {
                    let p = self.parse_single_param()?;
                    (p.name, p.type_node, p.default_value)
                } else {
                    let mut type_node = self.parse_type()?;
                    let field_name = self.get_identifier("Expected field name")?;
                    while self.peek().kind == TokenKind::LBracket {
                        self.advance();
                        self.consume(TokenKind::RBracket, "Expected ']' after '['")?;
                        type_node = BaseType::Array {
                            base_type: Box::new(type_node),
                            size: Box::new(None),
                        };
                    }
                    let default_value = if self.peek().kind == TokenKind::Assign
                        || self.peek().kind == TokenKind::Walrus
                    {
                        self.advance();
                        Some(self.parse_expression()?)
                    } else {
                        None
                    };
                    (field_name, type_node, default_value)
                };

                if self.peek().kind == TokenKind::SemiColon || self.peek().kind == TokenKind::Comma
                {
                    self.advance();
                }

                fields.push(BlueprintField {
                    is_static,
                    name: field_name,
                    type_node,
                    default_value,
                });
            }
            self.consume(TokenKind::RBrace, "Expected '}'")?;
            BlueprintDef::Explicit(fields)
        } else {
            return Err("Syntax Error: Expected '{' after blueprint declaration.".to_string());
        };

        // Match optional semicolon after blueprint block (like tests/20_blueprint.fs: blueprint Point -> {int(32) x; int(32) y;};)
        if self.peek().kind == TokenKind::SemiColon {
            self.consume(TokenKind::SemiColon, "Expected ';'")?;
        }

        let mut meta = TypeMetadata {
            name: name.clone(),
            ty: BaseType::Blueprint {
                name: name.clone(),
                fields: Box::new(std::collections::HashMap::new()),
                methods: Box::new(std::collections::HashMap::new()),
                generics: vec![],
            },
            fields: std::collections::HashMap::new(),
            constructor: None,
            methods: std::collections::HashMap::new(),
            handles: Vec::new(),
            handle_signatures: std::collections::HashMap::new(),
            vars: std::collections::HashMap::new(),
            variants: None,
        };

        if let BlueprintDef::Explicit(ref fields) = definition {
            for field in fields {
                meta.fields
                    .insert(field.name.clone(), field.type_node.clone());
            }
        }
        self.metadata.insert(name.clone(), meta);

        for g in &generics {
            let gen_name = g.as_str();
            self.current_generics.remove(&gen_name);
            if let Some(stripped) = gen_name.strip_prefix("...") {
                self.current_generics.remove(stripped);
            }
        }

        Ok(Decl::BlueprintDecl {
            visibility: Visibility::Private,
            name,
            generics,
            definition,
            share_directive,
        })
    }

    fn parse_impl_target_name(&mut self) -> Result<String, String> {
        let mut target = match &self.peek().kind {
            TokenKind::Identifier(name) => {
                let s = name.clone();
                self.advance();
                s
            }
            TokenKind::TypeChar => {
                self.advance();
                "char".to_string()
            }
            TokenKind::TypeUChar => {
                self.advance();
                "uchar".to_string()
            }
            TokenKind::TypeStr => {
                self.advance();
                "str".to_string()
            }
            TokenKind::TypeUStr => {
                self.advance();
                "ustr".to_string()
            }
            TokenKind::TypeBool | TokenKind::Flag => {
                self.advance();
                "bool".to_string()
            }
            TokenKind::TypeUSize => {
                self.advance();
                "usize".to_string()
            }
            TokenKind::TypeISize => {
                self.advance();
                "isize".to_string()
            }
            TokenKind::TypeInt(sz) => {
                let s = format!("int{}", sz);
                self.advance();
                s
            }
            TokenKind::TypeUInt(sz) => {
                let s = format!("uint{}", sz);
                self.advance();
                s
            }
            TokenKind::TypeFloat(sz) => {
                let s = format!("float{}", sz);
                self.advance();
                s
            }
            TokenKind::TypeType => {
                self.advance();
                "type".to_string()
            }
            _ => self.get_identifier("Expected target name for impl block")?,
        };

        while self.peek().kind == TokenKind::DoubleColon {
            self.advance(); // consume '::'
            let sub = self.get_identifier("Expected identifier after '::' in impl target")?;
            target.push_str("::");
            target.push_str(&sub);
        }

        while self.peek().kind == TokenKind::Multiply || self.peek().kind == TokenKind::Ampersand {
            if self.peek().kind == TokenKind::Multiply {
                self.advance();
                target.push('*');
            } else if self.peek().kind == TokenKind::Ampersand {
                self.advance();
                target.push('&');
            }
        }

        Ok(target)
    }

    pub(crate) fn parse_impl_decl(&mut self) -> Result<Decl, String> {
        self.consume(TokenKind::Impl, "Expected 'impl'")?;

        let is_handle_impl = if self.peek().kind == TokenKind::Handle {
            self.advance(); // consume 'handle'
            self.consume(
                TokenKind::For,
                "Expected 'for' after 'handle' in 'impl handle for'",
            )?;
            true
        } else {
            false
        };

        let is_static_impl = if !is_handle_impl && self.peek().kind == TokenKind::Static {
            self.advance(); // consume 'static'
            self.consume(
                TokenKind::For,
                "Expected 'for' after 'static' in 'impl static for'",
            )?;
            true
        } else {
            false
        };

        let target = self.parse_impl_target_name()?;
        let mut target_generics = Vec::new();
        if self.peek().kind == TokenKind::Less {
            self.parse_generics(&mut target_generics)?;
            for g in &target_generics {
                let gen_name = match g {
                    BaseType::GenericParam(n) => n.clone(),
                    BaseType::Blueprint { name, .. } => name.clone(),
                    _ => g.as_str(),
                };
                self.current_generics.insert(gen_name);
            }
        }

        let target_type = if target.ends_with('*') {
            let base_name = &target[..target.len() - 1];
            BaseType::RawPointer(Box::new(BaseType::from_str(base_name)))
        } else if target.ends_with('&') {
            let base_name = &target[..target.len() - 1];
            BaseType::Address(Box::new(BaseType::from_str(base_name)))
        } else if target == "array" {
            BaseType::Array {
                base_type: Box::new(if target_generics.is_empty() {
                    BaseType::Unknown
                } else {
                    target_generics[0].clone()
                }),
                size: Box::new(None),
            }
        } else if target == "char" {
            BaseType::Char
        } else if target == "bool" || target == "flag" {
            BaseType::Bool
        } else if target == "type" {
            BaseType::Type(Box::new(if target_generics.is_empty() {
                BaseType::Unknown
            } else {
                target_generics[0].clone()
            }))
        } else if target.starts_with("int") {
            BaseType::Int(Size::S32)
        } else if target.starts_with("uint") {
            BaseType::UInt(Size::S32)
        } else if target.starts_with("float") {
            BaseType::Float(Size::S64)
        } else if let Some(meta) = self.metadata.get(&target) {
            meta.ty.clone()
        } else if self.fn_metadata.contains_key(&target) {
            BaseType::Fn {
                name: Some(target.clone()),
                params: vec![],
                return_type: Box::new(BaseType::Unknown),
                mode: ExecutionMode::Runtime,
            }
        } else {
            BaseType::Blueprint {
                name: target.clone(),
                fields: Box::new(std::collections::HashMap::new()),
                methods: Box::new(std::collections::HashMap::new()),
                generics: target_generics.clone(),
            }
        };

        if is_handle_impl {
            let mut used_methods = Vec::new();
            let allowed = self.get_allowed_handle(&target_type)?;
            let handle_block = self.parse_handle_body(&mut used_methods, allowed)?;
            if self.peek().kind == TokenKind::SemiColon {
                self.consume(TokenKind::SemiColon, "Expected ';'")?;
            }
            for g in &target_generics {
                let gen_name = match g {
                    BaseType::GenericParam(n) => n.clone(),
                    BaseType::Blueprint { name, .. } => name.clone(),
                    _ => g.as_str(),
                };
                self.current_generics.remove(&gen_name);
            }
            return Ok(Decl::ImplDecl {
                target,
                target_generics,
                is_handle_impl: true,
                methods: Vec::new(),
                handle_block,
                static_block: Vec::new(),
            });
        }

        if is_static_impl {
            if self.peek().kind == TokenKind::Arrow {
                self.advance();
            }
            self.consume(TokenKind::LBrace, "Expected '{'")?;
            let mut static_block: Vec<Decl> = Vec::new();
            while self.peek().kind != TokenKind::RBrace && !self.is_at_end() {
                let stmt = self.parse_statement(ScopeType::Impl)?;
                if let Some(Stmt::Declaration(s)) = stmt {
                    static_block.push(s);
                } else if stmt.is_some() {
                    return Err("Expected declaration inside impl static block".to_string());
                }
            }
            self.consume(TokenKind::RBrace, "Expected '}'")?;
            if self.peek().kind == TokenKind::SemiColon {
                self.consume(TokenKind::SemiColon, "Expected ';'")?;
            }
            for g in &target_generics {
                let gen_name = match g {
                    BaseType::GenericParam(n) => n.clone(),
                    BaseType::Blueprint { name, .. } => name.clone(),
                    _ => g.as_str(),
                };
                self.current_generics.remove(&gen_name);
            }
            return Ok(Decl::ImplDecl {
                target,
                target_generics,
                is_handle_impl: false,
                methods: Vec::new(),
                handle_block: Vec::new(),
                static_block,
            });
        }

        if self.peek().kind == TokenKind::Arrow {
            self.advance();
        }
        self.consume(TokenKind::LBrace, "Expected '{'")?;

        let mut methods: Vec<Decl> = Vec::new();
        let mut handle_block: Vec<Decl> = Vec::new();
        let mut static_block: Vec<Decl> = Vec::new();

        while self.peek().kind != TokenKind::RBrace && !self.is_at_end() {
            if self.peek().kind == TokenKind::Handle {
                self.advance(); // consume 'handle'
                let mut used_handles = Vec::new();
                let allowed = self.get_allowed_handle(&target_type)?;
                let hdls = self.parse_handle_body(&mut used_handles, allowed)?;
                handle_block.extend(hdls);
            } else if self.peek().kind == TokenKind::Static {
                self.advance(); // consume 'static'
                if self.peek().kind == TokenKind::Arrow {
                    self.advance();
                }
                if self.peek().kind == TokenKind::LBrace {
                    self.advance(); // consume '{'
                    while self.peek().kind != TokenKind::RBrace && !self.is_at_end() {
                        let stmt = self.parse_statement(ScopeType::Impl)?;
                        if let Some(Stmt::Declaration(s)) = stmt {
                            static_block.push(s);
                        } else if stmt.is_some() {
                            return Err("Expected declaration inside static block".to_string());
                        }
                    }
                    self.consume(TokenKind::RBrace, "Expected '}' to close static block")?;
                    if self.peek().kind == TokenKind::SemiColon {
                        self.advance();
                    }
                } else if self.peek().kind == TokenKind::Fn {
                    let fn_decl = self.parse_fn_decl()?;
                    static_block.push(fn_decl);
                } else {
                    return Err("Expected '{' or 'fn' after 'static' in impl block".to_string());
                }
            } else {
                let stmt = self.parse_statement(ScopeType::Impl)?;
                if let Some(Stmt::Declaration(s)) = stmt {
                    methods.push(s);
                } else if stmt.is_some() {
                    return Err("Expected declaration inside impl block".to_string());
                }
            }
        }
        self.consume(TokenKind::RBrace, "Expected '}'")?;
        if self.peek().kind == TokenKind::SemiColon {
            self.consume(TokenKind::SemiColon, "Expected ';'")?;
        }

        let spec_key = if target_generics.is_empty()
            || target_generics
                .iter()
                .all(|g| matches!(g, BaseType::GenericParam(_)))
        {
            target.clone()
        } else {
            let g_str: Vec<String> = target_generics.iter().map(|g| g.as_str()).collect();
            format!("{}<{}>", target, g_str.join(", "))
        };

        let entry = self
            .metadata
            .entry(spec_key.clone())
            .or_insert_with(|| TypeMetadata {
                name: spec_key.clone(),
                ty: target_type.clone(),
                methods: std::collections::HashMap::new(),
                fields: std::collections::HashMap::new(),
                constructor: None,
                handles: vec![],
                handle_signatures: std::collections::HashMap::new(),
                vars: std::collections::HashMap::new(),
                variants: None,
            });
        for m in methods.iter().chain(static_block.iter()) {
            if let Decl::FnDecl {
                name,
                generics,
                params,
                return_type,
                ..
            } = m
            {
                entry.methods.insert(
                    name.clone(),
                    FnType {
                        name: name.clone(),
                        generics: generics.clone(),
                        params: params.clone(),
                        return_type: return_type.clone(),
                        mode: ExecutionMode::Runtime,
                    },
                );
            }
        }
        for h in &handle_block {
            if let Decl::FnDecl {
                name,
                generics,
                params,
                return_type,
                ..
            } = h
            {
                let hk = HandleMethods::from_str(name.as_str());
                if hk != HandleMethods::NotFound && !entry.handles.contains(&hk) {
                    entry.handles.push(hk);
                }
                entry.handle_signatures.insert(
                    name.clone(),
                    FnType {
                        name: name.clone(),
                        generics: generics.clone(),
                        params: params.clone(),
                        return_type: return_type.clone(),
                        mode: ExecutionMode::Runtime,
                    },
                );
            }
        }

        for g in &target_generics {
            let gen_name = match g {
                BaseType::GenericParam(n) => n.clone(),
                BaseType::Blueprint { name, .. } => name.clone(),
                _ => g.as_str(),
            };
            self.current_generics.remove(&gen_name);
        }

        Ok(Decl::ImplDecl {
            target,
            target_generics,
            is_handle_impl: false,
            methods,
            handle_block,
            static_block,
        })
    }
}
