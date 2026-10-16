pub mod control;
pub mod module_decl;
pub mod var_decl;

pub use var_decl::RhsValue;

use crate::frontend::lexer::token::TokenKind;
use crate::frontend::parser::ast::*;
use crate::frontend::parser::parser::Parser;

impl Parser {
    pub(crate) fn parse_statement(&mut self, scope: ScopeType) -> Result<Option<Stmt>, String> {
        let result: Result<crate::frontend::parser::ast::Stmt, String> = match &self.peek().kind {
            TokenKind::SemiColon => {
                self.advance();
                return Ok(None);
            }
            TokenKind::Import => {
                self.parse_import_stmt().map(Stmt::Declaration)
            }
            TokenKind::Public => {
                self.parse_exported_stmt().map(Stmt::Declaration)
            }
            TokenKind::Extern => self.parse_extern_decl().map(Stmt::Declaration),
            TokenKind::Define => self.parse_define_stmt(false).map(Stmt::Declaration),
            TokenKind::Const => {
                if self.tokens.get(self.current + 1).map(|t| &t.kind) == Some(&TokenKind::LBrace) {
                    self.advance(); // consume 'const'
                    self.parse_object_destructure_decl(Editability::NotEditable, scope)
                } else if self.is_named_destructure_start_at(self.current + 1) {
                    self.advance(); // consume 'const'
                    self.parse_named_destructure_decl(Editability::NotEditable, scope)
                } else {
                    self.parse_const(scope).map(Stmt::Declaration)
                }
            }
            | TokenKind::TypeInt(_)
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
            | TokenKind::Flag
            | TokenKind::TypeMethod => {
                if let Some(next) = self.tokens.get(self.current + 1) {
                    if let TokenKind::Identifier(_) = next.kind {
                        if let Some(after) = self.tokens.get(self.current + 2) {
                            if after.kind == TokenKind::LParen || after.kind == TokenKind::Less {
                                return self.parse_fn_decl().map(Stmt::Declaration).map(Some);
                            }
                        }
                    }
                }
                self.parse_var_decl(scope).map(Stmt::Declaration)
            }
            | TokenKind::TypeFn
            | TokenKind::TypeLambda
            | TokenKind::TypeNumber
            | TokenKind::TypeType
            | TokenKind::TypeAuto => self.parse_var_decl(scope).map(Stmt::Declaration),

            TokenKind::TypeFunction => {
                if let Some(next) = self.tokens.get(self.current + 1) {
                    if next.kind == TokenKind::DoubleColon {
                        if let Some(sub) = self.tokens.get(self.current + 2) {
                            if matches!(sub.kind, TokenKind::Fn | TokenKind::TypeFn | TokenKind::TypeMethod) {
                                self.advance(); // consume 'function'
                                self.advance(); // consume '::'
                                return self.parse_fn_decl().map(Stmt::Declaration).map(Some);
                            }
                        }
                    } else if let TokenKind::Identifier(_) = next.kind {
                        if let Some(after) = self.tokens.get(self.current + 2) {
                            if after.kind == TokenKind::LParen || after.kind == TokenKind::Less {
                                self.advance(); // consume 'function', identifier is function name
                                return self.parse_fn_decl().map(Stmt::Declaration).map(Some);
                            }
                        }
                    }
                }
                self.parse_var_decl(scope).map(Stmt::Declaration)
            }

            TokenKind::TypeObject => {
                if let Some(next) = self.tokens.get(self.current + 1) {
                    if next.kind == TokenKind::DoubleColon {
                        self.advance(); // consume 'object'
                        self.advance(); // consume '::'
                        match self.peek().kind {
                            TokenKind::TypeStruct => return self.parse_struct_decl().map(Stmt::Declaration).map(Some),
                            TokenKind::TypeClass => return self.parse_class_decl().map(Stmt::Declaration).map(Some),
                            TokenKind::TypeBluePrint => return self.parse_blueprint_decl().map(Stmt::Declaration).map(Some),
                            TokenKind::TypeEnum => return self.parse_enum_decl().map(Stmt::Declaration).map(Some),
                            TokenKind::Identifier(ref id) if id == "namespace" => {
                                self.advance(); // consume 'namespace'
                                return self.parse_namespace_decl().map(Stmt::Declaration).map(Some);
                            }
                            _ => {}
                        }
                    } else if let TokenKind::Identifier(_) = next.kind {
                        if let Some(after) = self.tokens.get(self.current + 2) {
                            if after.kind == TokenKind::DoubleColon {
                                self.advance(); // consume 'object'
                                return self.parse_namespace_decl().map(Stmt::Declaration).map(Some);
                            } else if after.kind == TokenKind::Assign {
                                if let Some(after_assign) = self.tokens.get(self.current + 3) {
                                    if after_assign.kind == TokenKind::LBrace {
                                        self.advance(); // consume 'object'
                                        return self.parse_blueprint_decl().map(Stmt::Declaration).map(Some);
                                    }
                                }
                            } else if after.kind == TokenKind::LBrace {
                                self.advance(); // consume 'object'
                                return self.parse_enum_decl().map(Stmt::Declaration).map(Some);
                            }
                        }
                    }
                }
                self.parse_var_decl(scope).map(Stmt::Declaration)
            }

            TokenKind::Using => self.parse_using_stmt(),
            TokenKind::Set => self.parse_reassign_stmt(),

            | TokenKind::TypeBluePrint
            | TokenKind::Impl
            | TokenKind::Fn
            | TokenKind::Virtual
            | TokenKind::Abstract
            | TokenKind::TypeClass
            | TokenKind::TypeStruct
            | TokenKind::TypeEnum
            | TokenKind::TypeBlock
            | TokenKind::TypeMicro
            | TokenKind::TypeMacro
            | TokenKind::TypeMachine
            | TokenKind::Del => {
                if
                    !matches!(
                        scope,
                        ScopeType::Global |
                            ScopeType::Class |
                            ScopeType::Struct |
                            ScopeType::Blueprint |
                            ScopeType::Handle |
                            ScopeType::Label |
                            ScopeType::Block |
                            ScopeType::Fn |
                            ScopeType::Impl
                    )
                {
                    return Err(
                        format!(
                            "Syntax Error: {:?} declarations are not allowed in this scope",
                            self.peek().kind
                        )
                    );
                }
                match self.peek().kind {
                    TokenKind::TypeBluePrint => self.parse_blueprint_decl().map(Stmt::Declaration),
                    TokenKind::Impl => self.parse_impl_decl().map(Stmt::Declaration),
                    TokenKind::Fn | TokenKind::Virtual | TokenKind::Abstract => {
                        if self.peek().kind == TokenKind::Fn && self.peek_ahead(1).map(|t| t.kind == TokenKind::LParen).unwrap_or(false) {
                            self.parse_expression_stmt()
                        } else {
                            self.parse_fn_decl().map(Stmt::Declaration)
                        }
                    }
                    TokenKind::TypeClass => self.parse_class_decl().map(Stmt::Declaration),
                    TokenKind::TypeStruct => self.parse_struct_decl().map(Stmt::Declaration),
                    TokenKind::TypeEnum => self.parse_enum_decl().map(Stmt::Declaration),
                    TokenKind::TypeBlock => self.parse_block_scope_decl().map(Stmt::Declaration),
                    TokenKind::TypeMicro => self.parse_micro_decl().map(Stmt::Declaration),
                    TokenKind::TypeMacro => self.parse_macro_decl().map(Stmt::Declaration),
                    TokenKind::TypeMachine => self.parse_machine_decl().map(Stmt::Declaration),
                    TokenKind::Del => self.parse_del_stmt(),
                    _ => unreachable!(),
                }
            }

            | TokenKind::If
            | TokenKind::For
            | TokenKind::Loop
            | TokenKind::While
            | TokenKind::Do
            | TokenKind::Match => {
                if
                    !matches!(
                        scope,
                        ScopeType::Global |
                            ScopeType::Fn |
                            ScopeType::Class |
                            ScopeType::Blueprint |
                            ScopeType::Block |
                            ScopeType::Label |
                            ScopeType::Handle
                    )
                {
                    return Err(
                        format!("Syntax Error: {:?} is not allowed in this scope", self.peek().kind)
                    );
                }
                match self.peek().kind {
                    TokenKind::If => self.parse_if_stmt(),
                    TokenKind::For => self.parse_for_stmt(),
                    TokenKind::Loop => self.parse_loop_stmt(),
                    TokenKind::While => self.parse_while_stmt(),
                    TokenKind::Do => self.parse_do_while_stmt(),
                    TokenKind::Match => self.parse_switch_stmt(),
                    _ => unreachable!(),
                }
            }

            TokenKind::Leave => {
                self.advance();
                if self.peek().kind == TokenKind::If {
                    self.advance();
                    let cond = self.parse_postfix_condition()?;
                    self.consume(TokenKind::SemiColon, "Expected ';' after postfix if")?;
                    return Ok(Some(Stmt::IfStmt {
                        condition: cond,
                        then_block: vec![Stmt::LeaveStmt],
                        else_block: None,
                    }));
                }
                self.consume(TokenKind::SemiColon, "Expected ';' after leave")?;
                Ok(Stmt::LeaveStmt)
            }

            TokenKind::Yield => { self.parse_yield_stmt() }
            TokenKind::Break | TokenKind::Continue => {
                let kind = self.peek().kind.clone();
                self.advance();
                let base_stmt = if kind == TokenKind::Break {
                    Stmt::BreakStmt
                } else {
                    Stmt::ContinueStmt
                };
                if self.peek().kind == TokenKind::If {
                    self.advance();
                    let cond = self.parse_postfix_condition()?;
                    self.consume(TokenKind::SemiColon, "Expected ';' after postfix if")?;
                    return Ok(Some(Stmt::IfStmt {
                        condition: cond,
                        then_block: vec![base_stmt],
                        else_block: None,
                    }));
                }
                self.consume(
                    TokenKind::SemiColon,
                    format!("Expected ';' after {:?}", kind).as_str()
                )?;
                Ok(base_stmt)
            }

            TokenKind::Return => {
                self.advance();
                let mut ret_val = None;
                if self.peek().kind != TokenKind::SemiColon && self.peek().kind != TokenKind::If {
                    ret_val = Some(self.parse_expression()?);
                }
                if self.peek().kind == TokenKind::If {
                    self.advance();
                    let cond = self.parse_postfix_condition()?;
                    self.consume(TokenKind::SemiColon, "Expected ';' after postfix if")?;
                    return Ok(Some(Stmt::IfStmt {
                        condition: cond,
                        then_block: vec![Stmt::ReturnStmt(ret_val)],
                        else_block: None,
                    }));
                }
                if self.peek().kind == TokenKind::SemiColon {
                    self.advance();
                }
                Ok(Stmt::ReturnStmt(ret_val))
            }

            TokenKind::Throw => self.parse_throw_stmt(),
            TokenKind::Try => self.parse_try_catch_stmt(),

            TokenKind::LabelName(lbl)
                if lbl.eq_ignore_ascii_case("@dependency") =>
            {
                self.advance(); // consume directive
                if self.peek().kind == TokenKind::LBrace {
                    self.advance(); // consume '{'
                    let mut depth = 1;
                    while !self.is_at_end() && depth > 0 {
                        if self.peek().kind == TokenKind::LBrace {
                            depth += 1;
                        } else if self.peek().kind == TokenKind::RBrace {
                            depth -= 1;
                            if depth == 0 {
                                self.advance(); // consume '}'
                                break;
                            }
                        }
                        self.advance();
                    }
                }
                if self.peek().kind == TokenKind::SemiColon {
                    self.advance();
                }
                return Ok(None);
            }

            TokenKind::LabelName(lbl)
                if lbl.eq_ignore_ascii_case("@compile")
                    || lbl.eq_ignore_ascii_case("@sys") =>
            {
                if lbl.eq_ignore_ascii_case("@compile") && self.is_compile_validation_ahead() {
                    self.advance(); // consume '@compile'
                    if self.peek().kind == TokenKind::Arrow {
                        self.advance();
                    }
                    self.consume(
                        TokenKind::LBrace,
                        "Expected '{' to open @compile validation body",
                    )?;
                    let mut body = Vec::new();
                    while !self.is_at_end() && self.peek().kind != TokenKind::RBrace {
                        if let Some(s) = self.parse_statement(ScopeType::Block)? {
                            body.push(s);
                        }
                    }
                    self.consume(
                        TokenKind::RBrace,
                        "Expected '}' after @compile validation body",
                    )?;
                    let mut args = Vec::new();
                    if self.peek().kind == TokenKind::LParen {
                        self.advance(); // consume '('
                        if self.peek().kind != TokenKind::RParen {
                            args.push(self.parse_expression()?);
                            while self.peek().kind == TokenKind::Comma {
                                self.advance();
                                args.push(self.parse_expression()?);
                            }
                        }
                        self.consume(
                            TokenKind::RParen,
                            "Expected ')' after @compile validation arguments",
                        )?;
                    }
                    if self.peek().kind == TokenKind::SemiColon {
                        self.advance();
                    }
                    Ok(Stmt::CompileValidation { body, args })
                } else if (self.peek_ahead(1).map(|t| &t.kind) == Some(&TokenKind::DoubleColon)
                    && self.peek_ahead(2).map(|t| &t.kind) == Some(&TokenKind::LBrace))
                    || (matches!(
                        self.peek_ahead(1).map(|t| &t.kind),
                        Some(TokenKind::Identifier(_))
                    ) && self.peek_ahead(2).map(|t| &t.kind) == Some(&TokenKind::DoubleColon)
                        && self.peek_ahead(3).map(|t| &t.kind) == Some(&TokenKind::LBrace))
                {
                    self.parse_directive_decl().map(Stmt::Declaration)
                } else if self.peek_ahead(1).map(|t| &t.kind) == Some(&TokenKind::Arrow)
                    || self.peek_ahead(1).map(|t| &t.kind) == Some(&TokenKind::LBrace)
                {
                    let lbl_name = lbl.clone();
                    Err(format!(
                        "Syntax Error: Expected '::' after '{}' (e.g. '{} :: {{ ... }}')",
                        lbl_name, lbl_name
                    ))
                } else {
                    self.parse_expression_or_reassignment()
                }
            }

            TokenKind::Identifier(_) if self.is_namespace_decl_start() => {
                self.parse_namespace_decl().map(Stmt::Declaration)
            }

            TokenKind::Identifier(_) if self.is_named_destructure_start() => {
                self.parse_named_destructure_decl(Editability::Editable, scope)
            }
            TokenKind::Identifier(_) if self.is_var_decl_start() => {
                self.parse_var_decl(scope).map(Stmt::Declaration)
            }
            TokenKind::Identifier(_) if self.is_block_decl_start() => {
                self.parse_bare_block_decl().map(Stmt::Declaration)
            }
            TokenKind::Identifier(_) => self.parse_expression_or_reassignment(),

            TokenKind::This if self.peek_ahead(1).map(|t| &t.kind) == Some(&TokenKind::LBrace) => {
                self.advance(); // consume 'this'
                self.advance(); // consume '{'
                let mut stmts = Vec::new();
                while !self.is_at_end() && self.peek().kind != TokenKind::RBrace {
                    if let Some(s) = self.parse_statement(scope.clone())? {
                        stmts.push(s);
                    }
                }
                self.consume(TokenKind::RBrace, "Expected '}' after this scope block")?;
                if self.peek().kind == TokenKind::SemiColon {
                    self.advance();
                }
                Ok(Stmt::ThisBlock(stmts))
            }
            TokenKind::This => { self.parse_expression_or_reassignment() }

            TokenKind::Super => {
                if
                    !matches!(
                        scope,
                        ScopeType::Class |
                            ScopeType::Struct |
                            ScopeType::Blueprint |
                            ScopeType::Handle |
                            ScopeType::Label
                    )
                {
                    return Err("Syntax Error: 'super' is not allowed in this scope".to_string());
                }
                self.parse_expression_or_reassignment()
            }

            TokenKind::Goto => {
                if
                    !matches!(
                        scope,
                        ScopeType::Handle |
                            ScopeType::Label |
                            ScopeType::Fn |
                            ScopeType::Blueprint |
                            ScopeType::Block
                    )
                {
                    return Err(
                        "Syntax Error: Goto statements are not allowed in this scope".to_string()
                    );
                }
                self.parse_goto_stmt()
            }
            TokenKind::LBrace if self.is_object_destructure_start() => {
                self.parse_object_destructure_decl(Editability::Editable, scope)
            }
            TokenKind::LBrace => {
                self.advance(); // consume '{'
                let blk = self.parse_block("block".to_string())?;
                self.consume(TokenKind::RBrace, "Expected '}' to close block")?;
                if self.peek().kind == TokenKind::If {
                    self.advance(); // consume 'if'
                    let cond = self.parse_postfix_condition()?;
                    if self.peek().kind == TokenKind::SemiColon {
                        self.advance();
                    }
                    Ok(Stmt::IfStmt {
                        condition: cond,
                        then_block: blk,
                        else_block: None,
                    })
                } else {
                    if self.peek().kind == TokenKind::SemiColon {
                        self.advance();
                    }
                    Ok(Stmt::Block(blk))
                }
            }
            _ => self.parse_expression_stmt(),
        };

        match result {
            Ok(stmt) => Ok(Some(stmt)),
            Err(err) => {
                let err_str = format!("{}", err);
                let err_msg: String = if err_str.starts_with("Syntax Error:") {
                    err_str.clone()
                } else {
                    format!("Syntax Error: {}", err_str)
                };
                eprintln!("{}", err_msg);
                self.synchronize();
                Err(err_msg)
            }
        }
    }

    pub(crate) fn get_identifier(&mut self, err_msg: &str) -> Result<String, String> {
        match &self.peek().kind {
            TokenKind::Identifier(n) => {
                let s = n.clone();
                self.advance();
                Ok(s)
            }
            _ => Err(err_msg.to_string()),
        }
    }

    pub(crate) fn get_handle_identifier(&mut self, err_msg: &str) -> Result<String, String> {
        match &self.peek().kind {
            TokenKind::Identifier(n) => {
                let s = n.clone();
                self.advance();
                Ok(s)
            }
            TokenKind::Call => {
                self.advance();
                Ok("call".to_string())
            }
            TokenKind::Return => {
                self.advance();
                Ok("return".to_string())
            }
            TokenKind::Break => {
                self.advance();
                Ok("break".to_string())
            }
            TokenKind::Continue => {
                self.advance();
                Ok("continue".to_string())
            }
            TokenKind::Leave => {
                self.advance();
                Ok("leave".to_string())
            }
            TokenKind::Yield => {
                self.advance();
                Ok("yield".to_string())
            }
            TokenKind::Throw => {
                self.advance();
                Ok("throw".to_string())
            }
            TokenKind::Share => {
                self.advance();
                Ok("share".to_string())
            }
            _ => Err(err_msg.to_string()),
        }
    }
}
