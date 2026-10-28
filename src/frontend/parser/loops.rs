use crate::frontend::lexer::token::TokenKind;
use crate::frontend::parser::ast::*;
use crate::frontend::parser::parser::Parser;

impl Parser {
    // --- loop N -> { ... }  or  loop -> { ... } (infinite) --
    // or loop N -> scope_name() / loop -> scope_name()
    pub(crate) fn parse_loop_stmt(&mut self) -> Result<Stmt, String> {
        self.advance(); // 'loop'
        let count = if
            self.peek().kind == TokenKind::LBrace ||
            self.peek().kind == TokenKind::Arrow
        {
            None
        } else {
            Some(self.parse_expression()?)
        };
        if self.peek().kind == TokenKind::Arrow {
            self.advance();
        }

        let body = if self.peek().kind == TokenKind::LBrace {
            self.advance(); // '{'
            let stmts = self.parse_block("loop".to_string())?;
            self.consume(TokenKind::RBrace, "Expected '}' to close loop body")?;
            EitherBlock::Inline(stmts)
        } else {
            let expr = self.parse_expression()?;
            if self.peek().kind == TokenKind::SemiColon {
                self.advance();
            }
            EitherBlock::External(expr)
        };

        Ok(Stmt::LoopStmt { count, body })
    }

    // --- while (cond) -> { ... }  or  while (cond) -> scope_name() ---
    pub(crate) fn parse_while_stmt(&mut self) -> Result<Stmt, String> {
        self.advance(); // 'while'
        let condition = if self.peek().kind == TokenKind::LParen {
            self.advance();
            let c = self.parse_expression()?;
            self.consume(TokenKind::RParen, "Expected ')' after while condition")?;
            c
        } else {
            self.parse_expression()?
        };

        if self.peek().kind == TokenKind::Arrow {
            self.advance(); // consume '->'
        }

        let body = if self.peek().kind == TokenKind::LBrace {
            self.advance(); // '{'
            let stmts = self.parse_block("while".to_string())?;
            self.consume(TokenKind::RBrace, "Expected '}' to close while body")?;
            EitherBlock::Inline(stmts)
        } else {
            let expr = self.parse_expression()?;
            if self.peek().kind == TokenKind::SemiColon {
                self.advance();
            }
            EitherBlock::External(expr)
        };

        Ok(Stmt::WhileStmt { condition, body })
    }

    // --- do -> { ... } while(cond);  or  do stmt; while(cond); ---
    pub(crate) fn parse_do_while_stmt(&mut self) -> Result<Stmt, String> {
        self.advance(); // 'do'
        if self.peek().kind == TokenKind::Arrow {
            self.advance();
        }

        let body = if self.peek().kind == TokenKind::LBrace {
            self.advance(); // '{'
            let stmts = self.parse_block("do".to_string())?;
            self.consume(TokenKind::RBrace, "Expected '}' to close do body")?;
            EitherBlock::Inline(stmts)
        } else if self.peek().kind == TokenKind::While {
            return Err("Syntax Error: 'do' body cannot be empty before 'while'".to_string());
        } else {
            match self.parse_statement(ScopeType::Block)? {
                Some(stmt) => EitherBlock::Inline(vec![stmt]),
                None => EitherBlock::Inline(vec![]),
            }
        };

        self.consume(TokenKind::While, "Expected 'while' after do body")?;
        let condition = if self.peek().kind == TokenKind::LParen {
            self.advance();
            let c = self.parse_expression()?;
            self.consume(TokenKind::RParen, "Expected ')' after while condition")?;
            c
        } else {
            self.parse_expression()?
        };

        if self.peek().kind == TokenKind::SemiColon {
            self.advance();
        }

        Ok(Stmt::DoWhileStmt { body, condition })
    }

    // --- for (init; cond; inc) -> { ... } or for (x in y) -> { ... } or for x in y -> { ... } ---
    pub(crate) fn parse_for_stmt(&mut self) -> Result<Stmt, String> {
        self.advance(); // 'for'

        let has_paren = if self.peek().kind == TokenKind::LParen {
            self.advance();
            true
        } else {
            false
        };

        // Lookahead to see if it's a for-in loop
        let mut is_for_in = !has_paren;
        if has_paren {
            let mut lookahead = self.current;
            let mut paren_depth = 1;
            while lookahead < self.tokens.len() {
                match &self.tokens[lookahead].kind {
                    TokenKind::In => {
                        if paren_depth == 1 {
                            is_for_in = true;
                            break;
                        }
                    }
                    TokenKind::LParen => {
                        paren_depth += 1;
                    }
                    TokenKind::RParen => {
                        paren_depth -= 1;
                        if paren_depth == 0 {
                            break;
                        }
                    }
                    TokenKind::SemiColon => {
                        if paren_depth == 1 {
                            break;
                        }
                    }
                    _ => {}
                }
                lookahead += 1;
            }
        }

        if is_for_in {
            return self.parse_for_in_stmt_body(has_paren);
        }

        let init = if self.peek().kind == TokenKind::SemiColon {
            self.advance(); // skip ';'
            None
        } else {
            Some(Box::new(self.parse_for_init_stmt()?))
        };

        let condition = if self.peek().kind == TokenKind::SemiColon {
            None
        } else {
            Some(self.parse_expression()?)
        };
        self.consume(TokenKind::SemiColon, "Expected ';' after for condition")?;

        let increment = if self.peek().kind == TokenKind::RParen {
            None
        } else {
            let expr = self.parse_expression()?;
            let op = self.peek().kind.clone();
            // Only consume the op token if it's an assignment-like operator
            if
                op == TokenKind::Arrow ||
                op == TokenKind::Assign ||
                op == TokenKind::PlusAssign ||
                op == TokenKind::MinusAssign ||
                op == TokenKind::MulAssign ||
                op == TokenKind::DivAssign ||
                op == TokenKind::ModAssign
            {
                self.advance(); // consume the assignment operator
                let value = self.parse_expression()?;

                Some(
                    Box::new(Stmt::ReassignStmt {
                        target: expr,
                        value,
                        op: op.as_str().to_string(),
                    })
                )
            } else {
                Some(Box::new(Stmt::ExpressionStmt(expr)))
            }
        };
        self.consume(TokenKind::RParen, "Expected ')' after for clauses")?;

        if self.peek().kind == TokenKind::Arrow {
            self.advance(); // consume '->'
        }

        let body = if self.peek().kind == TokenKind::LBrace {
            self.advance();
            let stmts = self.parse_block("for".to_string())?;
            self.consume(TokenKind::RBrace, "Expected '}' to close for body")?;
            EitherBlock::Inline(stmts)
        } else {
            let expr = self.parse_expression()?;
            if self.peek().kind == TokenKind::SemiColon {
                self.advance();
            }
            EitherBlock::External(expr)
        };

        Ok(Stmt::ForStmt {
            init,
            condition,
            increment,
            body,
        })
    }

    pub(crate) fn parse_for_in_stmt_body(&mut self, has_paren: bool) -> Result<Stmt, String> {
        let item = if self.is_var_decl_start() {
            self.parse_var_decl(ScopeType::Block).map(Stmt::Declaration)?
        } else {
            let expr = self.parse_expression()?;
            Stmt::ExpressionStmt(expr)
        };

        self.consume(TokenKind::In, "Expected 'in' in for-in loop")?;
        let iterable = self.parse_expression()?;
        if has_paren {
            self.consume(TokenKind::RParen, "Expected ')' after for-in clauses")?;
        }
        if self.peek().kind == TokenKind::Arrow {
            self.advance(); // consume '->'
        }
        let body = if self.peek().kind == TokenKind::LBrace {
            self.advance();
            let stmts = self.parse_block("for-in".to_string())?;
            self.consume(TokenKind::RBrace, "Expected '}' to close for-in body")?;
            EitherBlock::Inline(stmts)
        } else {
            let expr = self.parse_expression()?;
            if self.peek().kind == TokenKind::SemiColon {
                self.advance();
            }
            EitherBlock::External(expr)
        };

        Ok(Stmt::ForInStmt {
            item: Box::new(item),
            iterable,
            body,
        })
    }
}
