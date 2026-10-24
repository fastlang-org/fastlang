
use crate::frontend::lexer::token::TokenKind;
use crate::frontend::parser::ast::*;
use crate::frontend::parser::parser::Parser;

impl Parser {
    pub(crate) fn parse_fn_decl(&mut self) -> Result<Decl, String> {
        let mut statement_block: Vec<Stmt> = Vec::new();
        let mut params: Vec<Param> = Vec::new();
        let mut return_type: BaseType = BaseType::Unknown;

        let mut is_virtual = false;
        let mut is_abstract = false;
        if self.peek().kind == TokenKind::Virtual {
            self.advance();
            is_virtual = true;
        } else if self.peek().kind == TokenKind::Abstract {
            self.advance();
            is_abstract = true;
        }

        if self.peek().kind == TokenKind::Fn || self.peek().kind == TokenKind::TypeMethod {
            self.advance();
        } else if !matches!(self.peek().kind, TokenKind::Identifier(_)) {
            return Err("Expected 'fn' or 'method'".to_string());
        }
        let name = self.get_identifier("Expected function name")?;
        if self.peek().kind == TokenKind::Using {
            self.advance();
            let target = self.get_identifier("Expected target function name after 'using'")?;
            self.consume(TokenKind::SemiColon, "Expected ';' after function alias declaration")?;
            return Ok(Decl::FnAliasDecl {
                visibility: Visibility::Public,
                name,
                target,
            });
        }
        let mut generics: Vec<BaseType> = Vec::new();
        if self.peek().kind == TokenKind::Less {
            self.parse_generics(&mut generics)?;
            for g in &generics {
                if let BaseType::GenericParam(gen_name) = g {
                    self.current_generics.insert(gen_name.clone());
                }
            }
        }
        let mut fn_meta: FnType = FnType {
            name: name.clone(),
            generics: generics.clone(),
            params: Vec::new(),
            return_type: return_type.clone(),
            mode: ExecutionMode::Runtime,
        };

        if self.peek().kind == TokenKind::Assign {
            self.advance();
        }
        self.consume(TokenKind::LParen, "Expected '(' after function name")?;
        if self.peek().kind != TokenKind::RParen {
            loop {
                let p_list = self.parse_params_entry()?;
                params.extend(p_list);
                if self.peek().kind == TokenKind::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        self.consume(TokenKind::RParen, "Expected ')' after function parameters")?;
        if self.peek().kind == TokenKind::Arrow {
            self.advance();
            if !(self.peek().kind == TokenKind::LBrace || self.peek().kind == TokenKind::SemiColon) {
                return_type = self.parse_return_type()?;
                if self.peek().kind == TokenKind::Pipe || self.peek().kind == TokenKind::Or {
                    return Err("Syntax Error: Union types are not supported in return type".to_string());
                }
            }
        } else if name == "main" {
            return_type = BaseType::Int(Size::S32);
        } else {
            return_type = BaseType::Void;
        }

        if return_type == BaseType::Unknown && name == "main" {
            return_type = BaseType::Int(Size::S32);
        }

        if is_abstract {
            if self.peek().kind == TokenKind::SemiColon {
                self.advance();
            } else if self.peek().kind == TokenKind::LBrace {
                return Err("Syntax Error: Abstract function cannot have a body".to_string());
            }
        } else {
            self.consume(TokenKind::LBrace, "Expected '{' to open function body")?;
            while !self.is_at_end() && self.peek().kind != TokenKind::RBrace {
                match self.parse_statement(ScopeType::Fn) {
                    Ok(Some(stmt)) => {
                        if self.peek().kind == TokenKind::RBrace {
                            match &stmt {
                                Stmt::ExpressionStmt(expr) => {
                                    if self.previous(None).kind != TokenKind::SemiColon {
                                        statement_block.push(Stmt::ReturnStmt(Some(expr.clone())));
                                        continue;
                                    }
                                }
                                Stmt::CallStmt(expr) => {
                                    if self.previous(None).kind != TokenKind::SemiColon {
                                        statement_block.push(Stmt::ReturnStmt(Some(expr.clone())));
                                        continue;
                                    }
                                }
                                _ => {}
                            }
                        }
                        statement_block.push(stmt);
                    }
                    Ok(None) => {
                        if !self.is_at_end() && self.peek().kind != TokenKind::RBrace {
                            self.advance();
                        }
                    }
                    Err(err) => return Err(err),
                }
            }
            self.consume(TokenKind::RBrace, "Expected '}' to close statement block")?;
            if self.peek().kind == TokenKind::SemiColon {
                self.advance(); // consume ';'
            }
        }

        fn_meta.name = name.clone();
        fn_meta.generics = generics.clone();
        fn_meta.params = params.clone();
        fn_meta.return_type = return_type.clone();
        self.fn_metadata.insert(name.clone(), fn_meta);

        for g in &generics {
            if let BaseType::GenericParam(gen_name) = g {
                self.current_generics.remove(gen_name);
            }
        }

        Ok(Decl::FnDecl {
            visibility: Visibility::Private,
            is_virtual,
            is_abstract,
            name,
            generics,
            params,
            return_type,
            body: statement_block,
        })
    }

    pub(crate) fn parse_block_scope_decl(&mut self) -> Result<Decl, String> {
        self.advance(); // consume 'block'
        let mut return_type = None;
        if self.peek().kind == TokenKind::Less {
            self.advance(); // consume '<'
            return_type = Some(self.parse_type()?);
            self.consume(TokenKind::Greater, "Expected '>' to close block return type")?;
        }

        let name = self.get_identifier("Expected block name")?;

        if self.peek().kind == TokenKind::Assign {
            self.advance(); // consume '='
        }

        if self.peek().kind == TokenKind::LParen {
            self.advance();
            self.consume(TokenKind::RParen, "Expected ')' after block parameters")?;
        }

        if self.peek().kind == TokenKind::Arrow {
            self.advance();
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
