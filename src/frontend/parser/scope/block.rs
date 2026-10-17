use crate::frontend::lexer::token::TokenKind;
use crate::frontend::parser::ast::*;
use crate::frontend::parser::parser::Parser;

impl Parser {
    pub(crate) fn parse_label_decl(&mut self, _scope: ScopeType) -> Result<Decl, String> {
        let label_name = if let TokenKind::LabelName(name) = self.peek().kind.clone() {
            self.advance();
            name
        } else {
            return Err("Expected label name".to_string());
        };

        let mut body = Vec::new();

        if self.peek().kind == TokenKind::Arrow {
            self.advance();
        }

        if self.peek().kind == TokenKind::LBrace {
            self.advance(); // consume '{'
            while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::EOF {
                if let TokenKind::LabelName(ref nested_name) = self.peek().kind {
                    return Err(format!(
                        "Syntax Error: Nested labels are strictly forbidden. Found '{}' inside label '{}' at line {}.",
                        nested_name, label_name, self.peek().line
                    ));
                }
                if let Some(stmt) = self.parse_statement(ScopeType::Label)? {
                    body.push(stmt);
                }
            }
            self.consume(TokenKind::RBrace, "Expected '}' after label block")?;
        } else if self.peek().kind == TokenKind::SemiColon {
            self.advance(); // consume ';'
        } else {
            return Err(format!("Expected '{{' or ';' after label '{}'", label_name));
        }

        // Mandatory execution bound brackets: [N]; or [];
        self.consume(
            TokenKind::LBracket,
            &format!(
                "Syntax Error: Label '{}' at line {} must be followed by mandatory execution bound brackets '[N];' or '[];'",
                label_name, self.peek().line
            ),
        )?;
        let max_runs = if self.peek().kind == TokenKind::RBracket {
            self.advance(); // consume ']' -> empty means infinite/unbounded
            None
        } else {
            let expr = self.parse_expression()?;
            self.consume(TokenKind::RBracket, "Expected ']' after label run limit expression")?;
            Some(Box::new(expr))
        };
        if self.peek().kind == TokenKind::SemiColon {
            self.advance(); // consume optional ';'
        }

        Ok(Decl::LabelDecl {
            name: label_name,
            max_runs,
            body,
        })
    }

    pub(crate) fn parse_field_block(
        &mut self,
        _metadata: &mut TypeMetadata,
        _field_type: Visibility //todo : fix it
    ) -> Result<Vec<Decl>, String> {
        self.advance(); // 'public' , 'private' or 'static'
        if self.peek().kind == TokenKind::Arrow {
            self.advance();
        }
        self.consume(TokenKind::LBrace, "Expected '{' to open block")?;
        let mut block = Vec::new();
        while !self.is_at_end() && self.peek().kind != TokenKind::RBrace {
            //we have only fn decl and var decl so we will not use the parse_statement ever here
            let stmt = self.parse_statement(ScopeType::Block)?;
            if let Some(Stmt::Declaration(s)) = stmt {
                block.push(s);
            }
        }
        self.consume(TokenKind::RBrace, "Expected '}' to close public block")?;
        if self.peek().kind == TokenKind::SemiColon {
            self.advance();
        }
        Ok(block)
    }
    // <T, U, V, W, X, Y, Z>
    pub(crate) fn parse_generics(&mut self, generics: &mut Vec<BaseType>) -> Result<(), String> {
        generics.clear();

        self.advance(); // '<'
        while !self.is_at_end() && self.peek().kind != TokenKind::Greater {
            let is_variadic = if self.peek().kind == TokenKind::DotDotDot {
                self.advance();
                true
            } else {
                false
            };
            let token = self.peek().kind.clone();
            if matches!(token, TokenKind::Identifier(_)) {
                let type_name = self.get_identifier("Unexpected error happen")?;
                let mut base_ty = if self.metadata.contains_key(&type_name) {
                    let mut meta_generics = Vec::new();
                    if self.peek().kind == TokenKind::Less {
                        self.advance();
                        self.parse_generic_list(&mut meta_generics)?;
                        self.consume(TokenKind::Greater, "Expected '>' after generic type parameter")?;
                    }
                    BaseType::Blueprint {
                        name: type_name,
                        fields: Box::new(std::collections::HashMap::new()),
                        methods: Box::new(std::collections::HashMap::new()),
                        generics: meta_generics,
                    }
                } else {
                    let name_stored = if is_variadic {
                        format!("...{}", type_name)
                    } else {
                        type_name
                    };
                    BaseType::GenericParam(name_stored)
                };

                while self.peek().kind == TokenKind::Multiply || self.peek().kind == TokenKind::Ampersand {
                    if self.peek().kind == TokenKind::Multiply {
                        self.advance();
                        base_ty = BaseType::RawPointer(Box::new(base_ty));
                    } else if self.peek().kind == TokenKind::Ampersand {
                        self.advance();
                        base_ty = BaseType::Address(Box::new(base_ty));
                    }
                }
                generics.push(base_ty);
                continue;
            } else if token == TokenKind::Comma {
                self.advance();
                continue;
            } else if let Ok(ty) = self.parse_type() {
                generics.push(ty);
                continue;
            } else {
                return Err(
                    format!(
                        "Unexpected Token {} in generic block at line {}, column {} \n\t - use Capital Type name ",
                        token.as_str(),
                        self.peek().line,
                        self.peek().column
                    )
                );
            }
        }
        self.consume(TokenKind::Greater, "Expected '>' to close generic block")?;
        Ok(())
    }
    pub(crate) fn get_allowed_handle(
        &mut self,
        base_type: &BaseType
    ) -> Result<Vec<HandleMethods>, String> {
        let mut allowed_handle: Vec<HandleMethods> = Vec::new();
        match base_type {
            BaseType::Class { .. } | BaseType::Enum { .. } | BaseType::Struct { .. } | BaseType::Blueprint { .. }
            | BaseType::Array { .. } | BaseType::Str | BaseType::UStr | BaseType::Char | BaseType::Int(_) | BaseType::UInt(_) | BaseType::Float(_) | BaseType::Bool | BaseType::USize | BaseType::ISize => {
                allowed_handle.push(HandleMethods::Add);
                allowed_handle.push(HandleMethods::Mod);
                allowed_handle.push(HandleMethods::Mul);
                allowed_handle.push(HandleMethods::Sub);
                allowed_handle.push(HandleMethods::Div);

                allowed_handle.push(HandleMethods::AddReassign);
                allowed_handle.push(HandleMethods::SubReassign);
                allowed_handle.push(HandleMethods::MulReassign);
                allowed_handle.push(HandleMethods::DivReassign);
                allowed_handle.push(HandleMethods::ModReassign);

                allowed_handle.push(HandleMethods::PreDecrement);
                allowed_handle.push(HandleMethods::PreIncrement);
                allowed_handle.push(HandleMethods::Decrement);
                allowed_handle.push(HandleMethods::Increment);

                allowed_handle.push(HandleMethods::Not);
                allowed_handle.push(HandleMethods::Negate);
                allowed_handle.push(HandleMethods::And);
                allowed_handle.push(HandleMethods::Or);

                allowed_handle.push(HandleMethods::PartialEqual);
                allowed_handle.push(HandleMethods::NotEqual);
                allowed_handle.push(HandleMethods::GreaterThan);
                allowed_handle.push(HandleMethods::GreaterThanEqual);
                allowed_handle.push(HandleMethods::LessThan);
                allowed_handle.push(HandleMethods::LessThanEqual);

                allowed_handle.push(HandleMethods::Equal);
                allowed_handle.push(HandleMethods::EqualAssign);
                allowed_handle.push(HandleMethods::Arrow);
                allowed_handle.push(HandleMethods::ArrowAssign);
                allowed_handle.push(HandleMethods::FatArrow);

                allowed_handle.push(HandleMethods::Call);

                allowed_handle.push(HandleMethods::Default);

                allowed_handle.push(HandleMethods::IndexAccess);
                allowed_handle.push(HandleMethods::PropertyAccess);
                allowed_handle.push(HandleMethods::NamespaceAccess);
                allowed_handle.push(HandleMethods::HandleAccess);

                allowed_handle.push(HandleMethods::Iter);
                allowed_handle.push(HandleMethods::Next);
                allowed_handle.push(HandleMethods::IterDone);

                allowed_handle.push(HandleMethods::Display);
                allowed_handle.push(HandleMethods::Throw);

                allowed_handle.push(HandleMethods::Drop);
                allowed_handle.push(HandleMethods::Share);
                allowed_handle.push(HandleMethods::Deref);
                allowed_handle.push(HandleMethods::Copy);
                allowed_handle.push(HandleMethods::Cast);
                allowed_handle.push(HandleMethods::Extension);
            }
            BaseType::Block { .. } => {
                allowed_handle.push(HandleMethods::Display);
            }
            BaseType::Machine { .. } => {
                allowed_handle.push(HandleMethods::Call);

                allowed_handle.push(HandleMethods::Break);
                allowed_handle.push(HandleMethods::Continue);

                allowed_handle.push(HandleMethods::Display);

                allowed_handle.push(HandleMethods::Throw);

                allowed_handle.push(HandleMethods::Error);

                allowed_handle.push(HandleMethods::Leave);
                allowed_handle.push(HandleMethods::Yield);

                allowed_handle.push(HandleMethods::Drop);
                allowed_handle.push(HandleMethods::Return);
                allowed_handle.push(HandleMethods::IsDone);
            }
            BaseType::Fn { .. } | BaseType::Method { .. } => {
                allowed_handle.push(HandleMethods::Break);
                allowed_handle.push(HandleMethods::Continue);

                allowed_handle.push(HandleMethods::Display);

                allowed_handle.push(HandleMethods::Throw);

                allowed_handle.push(HandleMethods::Error);

                allowed_handle.push(HandleMethods::Leave);
                allowed_handle.push(HandleMethods::Yield);

                allowed_handle.push(HandleMethods::Return);
                allowed_handle.push(HandleMethods::IsDone);
            }
            _ => {
                return Err(format!("No handles allowed for type {:?}", base_type));
            }
        }
        Ok(allowed_handle)
    }
    pub(crate) fn parse_handle_body(
        &mut self,
        used_methods: &mut Vec<HandleMethods>,
        allowed_methods: Vec<HandleMethods>
    ) -> Result<Vec<Decl>, String> {
        let mut handle_fn: Vec<Decl> = vec![];
        if self.peek().kind == TokenKind::Arrow {
            self.advance();
        }
        self.consume(TokenKind::LBrace, "Expected '{' to open handle block")?;

        while !self.is_at_end() && self.peek().kind != TokenKind::RBrace {
            if self.peek().kind == TokenKind::Fn {
                self.advance();
                let method_name = self.get_handle_identifier("Expected handle method name")?;
                let handle_kind = HandleMethods::from_str(&method_name);
                if handle_kind == HandleMethods::NotFound {
                    return Err(
                        format!(
                            "Syntax Error: '{}' is not a valid allowed handle method at line {}, column {}",
                            method_name,
                            self.peek().line,
                            self.peek().column
                        )
                    );
                }
                if !allowed_methods.contains(&handle_kind) {
                    return Err(
                        format!(
                            "Syntax Error: '{}' is not a allowed handle method for this scope type at line {}, column {}",
                            method_name,
                            self.peek().line,
                            self.peek().column
                        )
                    );
                }
                used_methods.push(handle_kind);

                let mut generics: Vec<BaseType> = Vec::new();
                if self.peek().kind == TokenKind::Less {
                    if matches!(
                        handle_kind,
                        HandleMethods::Display
                            | HandleMethods::Iter
                            | HandleMethods::Next
                            | HandleMethods::IterDone
                            | HandleMethods::Break
                            | HandleMethods::Continue
                            | HandleMethods::Copy
                            | HandleMethods::Drop
                            | HandleMethods::IsDone
                            | HandleMethods::Increment
                            | HandleMethods::Decrement
                            | HandleMethods::PreIncrement
                            | HandleMethods::PreDecrement
                    ) {
                        return Err(format!(
                            "Syntax Error: Handle method '{}' is not allowed to have generics at line {}, column {}",
                            method_name,
                            self.peek().line,
                            self.peek().column
                        ));
                    }
                    self.parse_generics(&mut generics)?;
                    for g in &generics {
                        if let BaseType::GenericParam(gen_name) = g {
                            self.current_generics.insert(gen_name.clone());
                        }
                    }
                }

                let mut method_params: Vec<Param> = Vec::new();
                if self.peek().kind == TokenKind::LParen {
                    self.advance();
                    if self.peek().kind != TokenKind::RParen {
                        loop {
                            let p = self.parse_single_param()?;
                            method_params.push(p);
                            if self.peek().kind == TokenKind::Comma {
                                self.advance();
                            } else {
                                break;
                            }
                        }
                    }
                    self.consume(TokenKind::RParen, "Expected ')' after handle method parameters")?;
                }

                if
                    matches!(
                        handle_kind,
                        HandleMethods::Display |
                            HandleMethods::Iter |
                            HandleMethods::Next |
                            HandleMethods::IterDone |
                            HandleMethods::Break |
                            HandleMethods::Continue |
                            HandleMethods::Copy
                    )
                    && !method_params.is_empty() {
                        return Err(
                            format!(
                                "Syntax Error: Handle method '{}' cannot take any parameters at line {}, column {}",
                                method_name,
                                self.peek().line,
                                self.peek().column
                            )
                        );
                    }

                let default_ret: BaseType = if handle_kind == HandleMethods::Display {
                    BaseType::Str
                } else {
                    BaseType::Void
                };

                let return_type = if self.peek().kind == TokenKind::Arrow {
                    self.advance();
                    if self.peek().kind == TokenKind::LBrace {
                        default_ret
                    } else {
                        self.parse_type()?
                    }
                } else {
                    default_ret
                };

                self.consume(TokenKind::LBrace, "Expected '{' to open handle method body")?;
                let body = self.parse_block(method_name.clone())?;
                self.consume(TokenKind::RBrace, "Expected '}' to close handle method body")?;

                for g in &generics {
                    if let BaseType::GenericParam(gen_name) = g {
                        self.current_generics.remove(gen_name);
                    }
                }

                handle_fn.push(Decl::FnDecl {
                    visibility: Visibility::Private,
                    is_virtual: false,
                    is_abstract: false,
                    name: method_name,
                    generics,
                    params: method_params,
                    return_type,
                    body,
                });
            } else {
                return Err(
                    format!(
                        "Syntax Error: Only 'fn' declarations are permitted inside a handle block. Modifiers like 'public', 'private', or 'method' are forbidden. Found '{}' at line {}, column {}",
                        self.peek().kind.as_str(),
                        self.peek().line,
                        self.peek().column
                    )
                );
            }
        }
        self.consume(TokenKind::RBrace, "Expected '}' to close handle block")?;
        Ok(handle_fn)
    }

    pub(crate) fn parse_constructor_decl(
        &mut self,
        meta: &mut TypeMetadata
    ) -> Result<Option<Vec<ConstructorDecl>>, String> {
        self.advance(); // 'constructor'
        if self.peek().kind == TokenKind::Arrow {
            self.advance();
        }
        self.consume(TokenKind::LBrace, "Expected '{' to open constructor block")?;
        let mut constructor_list = Vec::new();

        let mut con_meta: Vec<ConstructorType> = Vec::new();
        let mut init_num = 0;
        while !self.is_at_end() && self.peek().kind != TokenKind::RBrace {
            if self.peek().kind == TokenKind::Init {
                self.advance(); // consume 'init'
                let mut param = Vec::new();
                self.consume(TokenKind::LParen, "Expected '(' after 'init'")?;
                if self.peek().kind != TokenKind::RParen {
                    loop {
                        let p = self.parse_single_param()?;
                        param.push(p);
                        if self.peek().kind == TokenKind::Comma {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                }
                self.consume(TokenKind::RParen, "Expected ')' after constructor params")?;
                if self.peek().kind == TokenKind::Arrow {
                    self.advance();
                } else {
                    //?advice: better to add -> after init
                    println!(
                        "WARNING:constructors should have a return type after init, adding ->{{ at line {}, column {} ",
                        self.peek().line,
                        self.peek().column
                    );
                }
                self.consume(TokenKind::LBrace, "Expected '{' for constructor body")?;
                let mut body = Vec::new();
                while !self.is_at_end() && self.peek().kind != TokenKind::RBrace {
                    if let Some(stmt) = self.parse_statement(ScopeType::Fn)? {
                        body.push(stmt);
                    }
                }
                self.consume(TokenKind::RBrace, "Expected '}' for constructor body")?;
                constructor_list.push(ConstructorDecl {
                    expected_types: Vec::new(), // will be populated in analyzer or later
                    params: param.clone(),
                    body,
                });
                init_num += 1;
                let unique_name = format!("__init__{}__{}", init_num, param.len());
                con_meta.push(ConstructorType {
                    name: unique_name,
                    params: param,
                });
            } else {
                return Err(
                    format!(
                        "Syntax Error: this {} is not allowed in constructor block at line {}, column {} \n\t - use 'init(params) -> {{ ... }}' inside constructor block to declare constructors",
                        self.peek().kind.as_str(),
                        self.peek().line,
                        self.peek().column
                    )
                );
            }
        }
        self.consume(TokenKind::RBrace, "Expected '}' to close constructor block")?;
        meta.constructor = Some(con_meta);
        Ok(Some(constructor_list))
    }
    pub(crate) fn parse_block(&mut self, _scope: String) -> Result<Vec<Stmt>, String> {
        let mut stmts: Vec<Stmt> = Vec::new();
        while !self.is_at_end() && self.peek().kind != TokenKind::RBrace {
            match self.parse_statement(ScopeType::Block) {
                Ok(Some(stmt)) => {
                    if self.peek().kind == TokenKind::RBrace
                        && !matches!(_scope.as_str(), "loop" | "while" | "do" | "for" | "for-in" | "object")
                    {
                        match &stmt {
                            Stmt::ExpressionStmt(expr) => {
                                if self.previous(None).kind != TokenKind::SemiColon {
                                    stmts.push(Stmt::ReturnStmt(Some(expr.clone())));
                                    continue;
                                }
                            }
                            Stmt::CallStmt(expr) => {
                                if self.previous(None).kind != TokenKind::SemiColon {
                                    stmts.push(Stmt::ReturnStmt(Some(expr.clone())));
                                    continue;
                                }
                            }
                            _ => {}
                        }
                    }
                    stmts.push(stmt);
                }
                Ok(None) => {
                    if !self.is_at_end() && self.peek().kind != TokenKind::RBrace {
                        self.advance();
                    }
                }
                Err(err) => {
                    return Err(err);
                }
            }
        }
        Ok(stmts)
    }
}
