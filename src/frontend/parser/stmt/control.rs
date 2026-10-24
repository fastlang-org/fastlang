use crate::frontend::lexer::token::TokenKind;
use crate::frontend::parser::ast::*;
use crate::frontend::parser::parser::Parser;

impl Parser {
    // --- set <target> -> <value>; OR set name<T>(params) -> return_type; ---
    pub(crate) fn parse_reassign_stmt(&mut self) -> Result<Stmt, String> {
        self.advance(); // 'set'

        if self.peek().kind == TokenKind::Dot {
            self.advance(); // consume '.'
            if self.peek().kind == TokenKind::DoubleColon {
                self.advance();
            }
            if self.peek().kind == TokenKind::LBrace {
                self.advance();
                self.consume(TokenKind::RBrace, "Expected '}' in set . directive")?;
            }
            if self.peek().kind == TokenKind::SemiColon {
                self.advance();
            }
            return Ok(Stmt::SetNamespace(".".to_string()));
        }

        // Check if this is an intrinsic signature declaration: `set name<T>(params) -> return_type;` or `set name() -> return_type;`
        let ident_name = match &self.peek().kind {
            TokenKind::Identifier(name) => Some(name.clone()),
            TokenKind::Undefined => Some("undefined".to_string()),
            _ => None,
        };
        if let Some(name) = ident_name {
            if let Some(next) = self.tokens.get(self.current + 1) {
                if next.kind == TokenKind::Less || next.kind == TokenKind::LParen {
                    let fn_name = name;
                    self.advance(); // consume name
                    let mut generics = Vec::new();
                    if self.peek().kind == TokenKind::Less {
                        self.parse_generics(&mut generics)?;
                        for g in &generics {
                            if let BaseType::GenericParam(gen_name) = g {
                                self.current_generics.insert(gen_name.clone());
                            }
                        }
                    }

                    self.consume(TokenKind::LParen, "Expected '(' in signature declaration")?;
                    let mut params = Vec::new();
                    while !self.is_at_end() && self.peek().kind != TokenKind::RParen {
                        let p = if self.peek().kind == TokenKind::DotDotDot
                            || self
                                .peek_ahead(1)
                                .map(|t| t.kind == TokenKind::Colon)
                                .unwrap_or(false)
                        {
                            self.parse_single_param()?
                        } else {
                            let ty = self.parse_type()?;
                            Param {
                                name: format!("arg_{}", params.len()),
                                type_node: ty,
                                default_value: None,
                                is_variadic: false,
                            }
                        };
                        params.push(p);
                        if self.peek().kind == TokenKind::Comma {
                            self.advance();
                        } else if self.peek().kind != TokenKind::RParen {
                            return Err("Expected ',' or ')' in parameter list".to_string());
                        }
                    }
                    self.consume(TokenKind::RParen, "Expected ')' after parameter list")?;

                    let return_type = if self.peek().kind == TokenKind::Arrow {
                        self.advance();
                        self.parse_return_type()?
                    } else {
                        BaseType::Void
                    };

                    self.consume(
                        TokenKind::SemiColon,
                        "Expected ';' after signature declaration",
                    )?;

                    for g in &generics {
                        if let BaseType::GenericParam(gen_name) = g {
                            self.current_generics.remove(gen_name);
                        }
                    }

                    return Ok(Stmt::Declaration(Decl::FnDecl {
                        visibility: Visibility::Public,
                        name: fn_name,
                        generics,
                        params,
                        return_type,
                        body: vec![],
                        is_virtual: false,
                        is_abstract: false,
                    }));
                }
            }
        }
        if let TokenKind::Identifier(first_name) = &self.peek().kind.clone() {
            let mut is_ns = false;
            let mut ns_path = first_name.clone();
            let mut lookahead = 1;
            while let Some(tok) = self.tokens.get(self.current + lookahead) {
                if tok.kind == TokenKind::DoubleColon {
                    if let Some(next_id) = self.tokens.get(self.current + lookahead + 1) {
                        if let TokenKind::Identifier(id_str) = &next_id.kind {
                            ns_path.push_str("::");
                            ns_path.push_str(id_str);
                            lookahead += 2;
                            continue;
                        }
                    }
                }
                break;
            }
            if let Some(tok) = self.tokens.get(self.current + lookahead) {
                if tok.kind == TokenKind::SemiColon {
                    is_ns = true;
                } else if tok.kind == TokenKind::LBrace {
                    if let Some(after) = self.tokens.get(self.current + lookahead + 1) {
                        if after.kind == TokenKind::RBrace {
                            is_ns = true;
                        }
                    }
                } else if tok.kind == TokenKind::DoubleColon {
                    if let Some(after) = self.tokens.get(self.current + lookahead + 1) {
                        if after.kind == TokenKind::LBrace {
                            if let Some(after2) = self.tokens.get(self.current + lookahead + 2) {
                                if after2.kind == TokenKind::RBrace {
                                    is_ns = true;
                                }
                            }
                        }
                    }
                }
            }
            if is_ns {
                for _ in 0..lookahead {
                    self.advance();
                }
                if self.peek().kind == TokenKind::DoubleColon {
                    self.advance();
                }
                if self.peek().kind == TokenKind::LBrace {
                    self.advance();
                    self.consume(TokenKind::RBrace, "Expected '}' in set directive")?;
                }
                if self.peek().kind == TokenKind::SemiColon {
                    self.advance();
                }
                return Ok(Stmt::SetNamespace(ns_path));
            }
        }

        let target = self.parse_expression()?;

        let op = self.peek().kind.as_str().to_string();
        if op != TokenKind::Arrow.as_str() && op != TokenKind::Assign.as_str() {
            return Err(
                format!(
                    "Syntax Error: Expected '->' or '=' after target in set statement at line {}, column {}",
                    self.peek().line,
                    self.peek().column
                )
            );
        }

        self.advance();

        let value = self.parse_expression()?;
        self.consume(TokenKind::SemiColon, "Expected ';' after set statement")?;

        Ok(Stmt::ReassignStmt { target, value, op })
    }

    // --- if (cond) { ... } else { ... } -------------------
    pub(crate) fn parse_if_stmt(&mut self) -> Result<Stmt, String> {
        self.advance(); // 'if'

        let condition = if self.peek().kind == TokenKind::LParen {
            self.advance();
            let c = self.parse_expression()?;
            self.consume(TokenKind::RParen, "Expected ')' after if condition")?;
            c
        } else {
            self.parse_expression()?
        };

        if self.peek().kind == TokenKind::Arrow {
            self.advance();
        }

        // then block
        let then_block = if self.peek().kind == TokenKind::LBrace {
            self.advance(); // consume '{'
            let blk = self.parse_block("if".to_string())?;
            self.consume(TokenKind::RBrace, "Expected '}' to close if body")?;
            blk
        } else {
            match self.parse_statement(ScopeType::Block)? {
                Some(stmt) => vec![stmt],
                None => vec![],
            }
        };

        // else block
        let else_block = if self.peek().kind == TokenKind::Else {
            self.advance(); // 'else'

            if self.peek().kind == TokenKind::If {
                let nested = self.parse_if_stmt()?;
                Some(vec![nested])
            } else {
                if self.peek().kind == TokenKind::Arrow {
                    self.advance();
                }
                if self.peek().kind == TokenKind::LBrace {
                    self.advance(); // consume '{'
                    let blk = self.parse_block("if".to_string())?;
                    self.consume(TokenKind::RBrace, "Expected '}' to close else block")?;
                    Some(blk)
                } else {
                    match self.parse_statement(ScopeType::Block)? {
                        Some(stmt) => Some(vec![stmt]),
                        None => None,
                    }
                }
            }
        } else {
            None
        };

        Ok(Stmt::IfStmt {
            condition,
            then_block,
            else_block,
        })
    }

    //todo : make parse_scope_decl deal with switch
    pub(crate) fn parse_switch_stmt(&mut self) -> Result<Stmt, String> {
        self.advance(); // consume 'switch' or 'match'
        let condition = if self.peek().kind == TokenKind::LParen {
            self.advance();
            let c = self.parse_expression()?;
            self.consume(
                TokenKind::RParen,
                "Expected ')' after switch/match condition",
            )?;
            c
        } else {
            self.parse_expression()?
        };

        if self.peek().kind == TokenKind::Arrow {
            self.advance(); // consume optional '->'
        }

        self.consume(TokenKind::LBrace, "Expected '{' to open switch/match block")?;
        let mut cases = Vec::new();
        while !self.is_at_end() && self.peek().kind != TokenKind::RBrace {
            if self.peek().kind == TokenKind::Underscore {
                self.advance(); // consume '_'
                self.consume(
                    TokenKind::FatArrow,
                    "Expected '=>' after '_' in default match branch",
                )?;

                let def_body = if self.peek().kind == TokenKind::LBrace {
                    self.advance();
                    let b = self.parse_block("switch".to_string())?;
                    self.consume(TokenKind::RBrace, "Expected '}' to close default block")?;
                    if self.peek().kind == TokenKind::Comma
                        || self.peek().kind == TokenKind::SemiColon
                    {
                        self.advance();
                    }
                    b
                } else {
                    let s = self.parse_statement(ScopeType::Block)?;
                    if self.peek().kind == TokenKind::Comma {
                        self.advance();
                    }
                    match s {
                        Some(Stmt::ExpressionStmt(expr)) | Some(Stmt::CallStmt(expr)) => {
                            vec![Stmt::ReturnStmt(Some(expr))]
                        }
                        Some(st) => vec![st],
                        None => vec![],
                    }
                };

                cases.push(Stmt::CaseStmt {
                    option: Expr::Identifier("void".to_string()),
                    set: Expr::Identifier("void".to_string()),
                    body: def_body,
                });
            } else {
                self.in_match_pattern = true;
                let val_res = self.parse_expression();
                self.in_match_pattern = false;
                let val = val_res?;
                self.consume(TokenKind::FatArrow, "Expected '=>' after match pattern")?;

                let case_body = if self.peek().kind == TokenKind::LBrace {
                    self.advance();
                    let b = self.parse_block("case".to_string())?;
                    self.consume(
                        TokenKind::RBrace,
                        "Expected '}' to close match branch block",
                    )?;
                    if self.peek().kind == TokenKind::Comma
                        || self.peek().kind == TokenKind::SemiColon
                    {
                        self.advance();
                    }
                    b
                } else {
                    let s = self.parse_statement(ScopeType::Block)?;
                    if self.peek().kind == TokenKind::Comma {
                        self.advance();
                    }
                    match s {
                        Some(Stmt::ExpressionStmt(expr)) | Some(Stmt::CallStmt(expr)) => {
                            vec![Stmt::ReturnStmt(Some(expr))]
                        }
                        Some(st) => vec![st],
                        None => vec![],
                    }
                };

                cases.push(Stmt::CaseStmt {
                    option: val,
                    set: Expr::Identifier("void".to_string()),
                    body: case_body,
                });
            }
        }
        self.consume(
            TokenKind::RBrace,
            "Expected '}' to close switch/match block",
        )?;

        Ok(Stmt::SwitchStmt {
            name: String::new(),
            condition,
            cases,
        })
    }

    pub(crate) fn parse_del_stmt(&mut self) -> Result<Stmt, String> {
        self.advance(); // consume 'del'
        let expr = self.parse_expression()?;
        let mut is_array = false;
        if let Expr::Identifier(name) = &expr {
            let var = self.var_metadata.get(name);
            is_array = var.is_some() && var.unwrap().is_array;
        }
        self.consume(TokenKind::SemiColon, "Expected ';' after del statement")?;
        Ok(Stmt::DelStmt {
            target: expr,
            is_array,
        })
    }

    pub(crate) fn parse_throw_stmt(&mut self) -> Result<Stmt, String> {
        self.advance(); // 'throw'
        let expr = self.parse_expression()?;
        self.consume(TokenKind::SemiColon, "Expected ';' after throw statement")?;
        Ok(Stmt::ThrowStmt(expr))
    }

    pub(crate) fn parse_goto_stmt(&mut self) -> Result<Stmt, String> {
        self.advance(); // consume 'goto'
        if self.peek().kind == TokenKind::Arrow {
            self.advance();
        }
        let target = if let TokenKind::LabelName(name) = self.peek().kind.clone() {
            self.advance();
            name
        } else {
            return Err("Expected label name ex @label after 'goto'".to_string());
        };

        self.consume(TokenKind::SemiColon, "Expected ';' after goto statement")?;

        Ok(Stmt::GotoStmt(Expr::Identifier(target)))
    }
    //todo : remove this

    pub(crate) fn parse_yield_stmt(&mut self) -> Result<Stmt, String> {
        self.advance(); // consume 'yield'
        let mut expr = None;
        if self.peek().kind != TokenKind::SemiColon {
            expr = Some(self.parse_expression()?);
        }
        self.consume(TokenKind::SemiColon, "Expected ';' after yield")?;
        Ok(Stmt::YieldStmt(expr))
    }

    //TODO: fix this or remove it and build a better system for it
    pub(crate) fn parse_expression_or_reassignment(&mut self) -> Result<Stmt, String> {
        if let TokenKind::Identifier(name) = self.peek().kind.clone() {
            if name == "let" {
                self.advance();
                return self.parse_var_decl(ScopeType::Block).map(Stmt::Declaration);
            }

            let next1 = self.tokens.get(self.current + 1).map(|t| &t.kind);
            let next2 = self.tokens.get(self.current + 2).map(|t| &t.kind);

            // `TypeName varName =` or `TypeName<T> varName =` or `varName :=` pattern
            let is_var_decl = if self.macro_metadata.contains(&name) {
                false
            } else if next1 == Some(&TokenKind::Walrus) {
                true
            } else {
                match (next1, next2) {
                    (
                        Some(TokenKind::Identifier(_)),
                        Some(
                            TokenKind::Assign
                            | TokenKind::Arrow
                            | TokenKind::SemiColon
                            | TokenKind::LBracket,
                        ),
                    ) => true,
                    (
                        Some(TokenKind::Multiply | TokenKind::Ampersand),
                        Some(TokenKind::Identifier(_)),
                    ) => {
                        let next3 = self.tokens.get(self.current + 3).map(|t| &t.kind);
                        matches!(
                            next3,
                            Some(
                                TokenKind::Assign
                                | TokenKind::Arrow
                                | TokenKind::SemiColon
                                | TokenKind::LBracket
                            )
                        )
                    }
                    (Some(TokenKind::LBrace), _) => true,
                    (Some(TokenKind::Less), _) => {
                        let mut i = self.current + 2;
                        let mut depth = 1;
                        while i < self.tokens.len() && depth > 0 {
                            match &self.tokens[i].kind {
                                TokenKind::Less => {
                                    depth += 1;
                                }
                                TokenKind::Greater => {
                                    depth -= 1;
                                }
                                TokenKind::ShiftRight => {
                                    if depth >= 2 {
                                        depth -= 2;
                                    } else {
                                        depth -= 1;
                                    }
                                }
                                _ => {}
                            }
                            i += 1;
                        }
                        matches!(
                            (
                                self.tokens.get(i).map(|t| &t.kind),
                                self.tokens.get(i + 1).map(|t| &t.kind),
                            ),
                            (
                                Some(TokenKind::Identifier(_)),
                                Some(
                                    TokenKind::Arrow
                                        | TokenKind::Assign
                                        | TokenKind::SemiColon
                                        | TokenKind::LBracket
                                ),
                            ) | (Some(TokenKind::LBrace), _)
                        )
                    }
                    _ => false,
                }
            };

            if is_var_decl {
                return self.parse_var_decl(ScopeType::Block).map(Stmt::Declaration);
            }
        }

        let expr = self.parse_expression()?;

        // `target -> value;` or `target = value;`
        let op = self.peek().kind.clone();
        if op == TokenKind::Arrow {
            self.advance(); // consume '->'
            let value = self.parse_expression()?;
            self.consume(TokenKind::SemiColon, "Expected ';' after arrow expression")?;

            return Ok(Stmt::ExpressionStmt(Expr::BinaryOp {
                left: Box::new(expr),
                operator: "->".to_string(),
                right: Box::new(value),
            }));
        }

        if op == TokenKind::Assign
            || op == TokenKind::PlusAssign
            || op == TokenKind::MinusAssign
            || op == TokenKind::MulAssign
            || op == TokenKind::DivAssign
            || op == TokenKind::ModAssign
            || op == TokenKind::Walrus
            || op == TokenKind::Colon
        {
            self.advance(); // consume '=' or ':' or '+=' etc.
            let value = self.parse_expression()?;
            let op_str = if op == TokenKind::Colon {
                "=".to_string()
            } else {
                op.as_str().to_string()
            };
            let reassign_stmt = Stmt::ReassignStmt {
                target: expr,
                value,
                op: op_str,
            };
            if self.peek().kind == TokenKind::If {
                self.advance();
                let cond = self.parse_postfix_condition()?;
                if self.peek().kind == TokenKind::SemiColon || self.peek().kind == TokenKind::Comma {
                    self.advance();
                }
                return Ok(Stmt::IfStmt {
                    condition: cond,
                    then_block: vec![reassign_stmt],
                    else_block: None,
                });
            }
            if self.peek().kind == TokenKind::Comma || self.peek().kind == TokenKind::SemiColon {
                self.advance();
            } else if self.peek().kind != TokenKind::RBrace {
                self.consume(
                    TokenKind::SemiColon,
                    "Expected ';' or ',' after reassignment",
                )?;
            }
            return Ok(reassign_stmt);
        }

        // Command-style macro / function invocation: `callee arg1, arg2;`
        if matches!(&expr, Expr::Identifier(_) | Expr::NamespaceAccess { .. })
            && !self.is_at_end()
                && self.peek().kind != TokenKind::SemiColon
                && self.peek().kind != TokenKind::RBrace
                && self.peek().kind != TokenKind::RParen
                && self.peek().kind != TokenKind::RBracket
                && self.peek().kind != TokenKind::Comma
            {
                let mut args = Vec::new();
                args.push(self.parse_expression()?);
                while self.peek().kind == TokenKind::Comma {
                    self.advance();
                    args.push(self.parse_expression()?);
                }
                if self.peek().kind == TokenKind::SemiColon || self.peek().kind == TokenKind::Comma {
                    self.advance();
                } else if self.peek().kind != TokenKind::RBrace {
                    return Err("Expected ';' after call statement".to_string());
                }
                return Ok(Stmt::CallStmt(Expr::Call {
                    callee: Box::new(expr),
                    generics: Vec::new(),
                    args,
                }));
            }

        let base_stmt = Stmt::ExpressionStmt(expr);
        if self.peek().kind == TokenKind::If {
            self.advance();
            let cond = self.parse_postfix_condition()?;
            if self.peek().kind == TokenKind::SemiColon || self.peek().kind == TokenKind::Comma {
                self.advance();
            } else if self.peek().kind != TokenKind::RBrace {
                return Err("Expected ';' after postfix if".to_string());
            }
            return Ok(Stmt::IfStmt {
                condition: cond,
                then_block: vec![base_stmt],
                else_block: None,
            });
        }
        if self.peek().kind == TokenKind::SemiColon || self.peek().kind == TokenKind::Comma {
            self.advance();
        } else if self.peek().kind != TokenKind::RBrace {
            return Err("Expected ';' after statement".to_string());
        }
        Ok(base_stmt)
    }

    pub(crate) fn parse_postfix_condition(&mut self) -> Result<Expr, String> {
        let cond = if self.peek().kind == TokenKind::LParen {
            self.advance();
            let c = self.parse_expression()?;
            self.consume(TokenKind::RParen, "Expected ')' after postfix if condition")?;
            c
        } else {
            self.parse_expression()?
        };
        if self.peek().kind == TokenKind::Else {
            return Err("Syntax Error: 'else' is not allowed in postfix if statements".to_string());
        }
        Ok(cond)
    }

    pub(crate) fn parse_expression_stmt(&mut self) -> Result<Stmt, String> {
        let expr = self.parse_expression()?;
        // --- Bare reassignment: x = 10; or this.x = 20; ---
        let op = self.peek().kind.clone();
        if op == TokenKind::Assign
            || op == TokenKind::Arrow
            || op == TokenKind::PlusAssign
            || op == TokenKind::MinusAssign
            || op == TokenKind::MulAssign
            || op == TokenKind::DivAssign
            || op == TokenKind::ModAssign
        {
            self.advance(); // consume '=' or '->' or '+=' etc
            let value = self.parse_expression()?;
            let op_str = op.as_str().to_string();
            let base_stmt = Stmt::ReassignStmt {
                target: expr,
                value,
                op: op_str,
            };
            if self.peek().kind == TokenKind::If {
                self.advance();
                let cond = self.parse_postfix_condition()?;
                self.consume(
                    TokenKind::SemiColon,
                    "Expected ';' after postfix if",
                )?;
                return Ok(Stmt::IfStmt {
                    condition: cond,
                    then_block: vec![base_stmt],
                    else_block: None,
                });
            }
            self.consume(
                TokenKind::SemiColon,
                "Expected ';' after assignment statement",
            )?;
            return Ok(base_stmt);
        }
        let base_stmt = Stmt::ExpressionStmt(expr);
        if self.peek().kind == TokenKind::If {
            self.advance();
            let cond = self.parse_postfix_condition()?;
            self.consume(
                TokenKind::SemiColon,
                "Expected ';' after postfix if",
            )?;
            return Ok(Stmt::IfStmt {
                condition: cond,
                then_block: vec![base_stmt],
                else_block: None,
            });
        }
        if self.peek().kind == TokenKind::SemiColon {
            self.consume(
                TokenKind::SemiColon,
                "Expected ';' after expression statement",
            )?;
        } else if self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Comma {
            return Err("Expected ';' after expression statement".to_string());
        }
        Ok(base_stmt)
    }

    pub(crate) fn parse_try_catch_stmt(&mut self) -> Result<Stmt, String> {
        self.advance(); // 'try'
        if self.peek().kind == TokenKind::Arrow {
            self.advance(); // consume '->'
        }
        self.consume(TokenKind::LBrace, "Expected '{' to open try block")?;
        let try_block = self.parse_block("try".to_string())?;
        self.consume(TokenKind::RBrace, "Expected '}' to close try block")?;

        let mut catches = Vec::new();
        while self.peek().kind == TokenKind::Catch {
            self.advance(); // consume 'catch'
            self.consume(TokenKind::LParen, "Expected '(' after 'catch'")?;

            let catch_param: String = if let TokenKind::Identifier(n) = &self.peek().kind.clone() {
                let n = n.to_string();
                self.advance();
                n
            } else {
                return Err(format!(
                    "Expected parameter name in catch block at line {}",
                    self.peek().line
                ));
            };

            let param_type = if self.peek().kind == TokenKind::Colon {
                self.advance(); // consume ':'
                Some(self.parse_type()?)
            } else {
                None
            };

            self.consume(TokenKind::RParen, "Expected ')' after catch parameter")?;
            if self.peek().kind == TokenKind::Arrow {
                self.advance(); // consume '->'
            }
            self.consume(TokenKind::LBrace, "Expected '{' to open catch block")?;
            let catch_block = self.parse_block("catch".to_string())?;
            self.consume(TokenKind::RBrace, "Expected '}' to close catch block")?;

            catches.push(CatchClause {
                param: catch_param,
                param_type,
                body: catch_block,
            });
        }

        if catches.is_empty() {
            return Err("Expected at least one 'catch' block after 'try'".to_string());
        }

        Ok(Stmt::TryCatchStmt {
            try_block,
            catches,
        })
    }
}
