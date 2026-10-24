use crate::frontend::lexer::scanner::Scanner;
use crate::frontend::lexer::token::TokenKind;
use crate::frontend::parser::ast::*;
use crate::frontend::parser::parser::Parser;

impl Parser {
    pub(crate) fn parse_macro_decl(&mut self) -> Result<Decl, String> {
        self.advance(); // consume 'macro'

        let name = if self.peek().kind == TokenKind::DollarSign {
            self.advance();
            let id = self.get_handle_identifier("Expected identifier after '$'")?;
            format!("${}", id)
        } else if let TokenKind::LabelName(lbl) = &self.peek().kind {
            let label = lbl.clone();
            self.advance();
            label
        } else {
            let n = self.get_handle_identifier("Expected macro name after 'macro'")?;
            if let TokenKind::LabelName(lbl) = &self.peek().kind {
                let label = lbl.clone();
                self.advance();
                label
            } else {
                n
            }
        };
        self.macro_metadata.insert(name.clone());

        let mut generics: Vec<BaseType> = Vec::new();
        if self.peek().kind == TokenKind::Less {
            self.parse_generics(&mut generics)?;
            for g in &generics {
                if let BaseType::GenericParam(gen_name) = g {
                    self.current_generics.insert(gen_name.clone());
                }
            }
        }

        let mut params = Vec::new();
        let mut pattern = Vec::new();

        if self.peek().kind == TokenKind::Assign {
            self.advance();
        }

        // Loop parsing pattern elements until `->` or `{`
        while !self.is_at_end()
            && self.peek().kind != TokenKind::Arrow
            && self.peek().kind != TokenKind::LBrace
        {
            if self.peek().kind == TokenKind::At
                && self.peek_ahead(1).map(|t| &t.kind) == Some(&TokenKind::LBracket)
            {
                self.advance(); // consume '@'
                self.advance(); // consume '['
                let p = self.parse_single_param()?;
                self.consume(TokenKind::RBracket, "Expected ']' after @[param]")?;
                pattern.push(MacroPatternItem::LabelParam(p.clone()));
                params.push(p);
            } else if self.peek().kind == TokenKind::Hash
                && self.peek_ahead(1).map(|t| &t.kind) == Some(&TokenKind::LBracket)
            {
                self.advance(); // consume '#'
                self.advance(); // consume '['
                let p = self.parse_single_param()?;
                self.consume(TokenKind::RBracket, "Expected ']' after #[param]")?;
                pattern.push(MacroPatternItem::HashParam(p.clone()));
                params.push(p);
            } else if self.peek().kind == TokenKind::Multiply
                && self.peek_ahead(1).map(|t| &t.kind) == Some(&TokenKind::LBracket)
            {
                self.advance(); // consume '*'
                self.advance(); // consume '['
                let mut group_params = Vec::new();
                while !self.is_at_end() && self.peek().kind != TokenKind::RBracket {
                    let p = self.parse_single_param()?;
                    if matches!(&p.type_node, BaseType::Block { .. } | BaseType::Object) {
                        return Err(format!(
                            "Syntax Error: block or object not allowed in non-delimiter parameter '{}'. You must use a delimiter instead.",
                            p.name
                        ));
                    }
                    group_params.push(p.clone());
                    params.push(p);
                    if self.peek().kind == TokenKind::Comma {
                        self.advance();
                    }
                }
                self.consume(TokenKind::RBracket, "Expected ']' after *[...]")?;
                pattern.push(MacroPatternItem::ParamGroup {
                    delimiter: MacroDelimiter::UniversalAny,
                    params: group_params,
                });
            } else if self.peek().kind == TokenKind::Caret
                && self.peek_ahead(1).map(|t| &t.kind) == Some(&TokenKind::LBracket)
            {
                self.advance(); // consume '^'
                self.advance(); // consume '['
                let mut group_params = Vec::new();
                while !self.is_at_end() && self.peek().kind != TokenKind::RBracket {
                    let p = self.parse_single_param()?;
                    group_params.push(p.clone());
                    params.push(p);
                    if self.peek().kind == TokenKind::Comma {
                        self.advance();
                    }
                }
                self.consume(TokenKind::RBracket, "Expected ']' after ^[...]")?;
                pattern.push(MacroPatternItem::ParamGroup {
                    delimiter: MacroDelimiter::BracketedAny,
                    params: group_params,
                });
            } else if self.peek().kind == TokenKind::Ampersand {
                self.advance(); // consume '&'
                if self.peek().kind == TokenKind::LBracket {
                    self.advance(); // consume '['
                    while !self.is_at_end() && self.peek().kind != TokenKind::RBracket {
                        let tok = self.advance();
                        if let Some(op) = MacroOperator::from_token_kind(&tok.kind) {
                            pattern.push(MacroPatternItem::Operator(op));
                        } else {
                            return Err(format!(
                                "Syntax Error: Unsupported operator in macro pattern &[...]: {:?}",
                                tok.kind
                            ));
                        }
                    }
                    self.consume(TokenKind::RBracket, "Expected ']' after operator in macro pattern")?;
                } else if let Some(kw) = MacroKeyword::from_token_kind(&self.peek().kind) {
                    self.advance();
                    pattern.push(MacroPatternItem::Keyword(kw));
                } else {
                    return Err(format!(
                        "Syntax Error: Unexpected token after '&' in macro pattern: {:?}",
                        self.peek().kind
                    ));
                }
            } else if let Some(kw) = MacroKeyword::from_token_kind(&self.peek().kind) {
                self.advance();
                pattern.push(MacroPatternItem::Keyword(kw));
            } else if let TokenKind::TemplateText(text) = self.peek().kind.clone() {
                self.advance(); // consume template text representing #{...} or `[...]`
                let sub_tokens = Scanner::scan_all(&text);
                let mut sub_parser = Parser::new_with_aliases(sub_tokens, self.type_aliases.clone());
                let p = sub_parser.parse_single_param()?;
                let kind = MacroBlockKind::HashBrace;
                pattern.push(MacroPatternItem::BlockParam { kind, param: p.clone() });
                params.push(p);
            } else if self.peek().kind == TokenKind::LBracket {
                self.advance(); // consume outer '['
                // Check if this is an exclusion list: [[{}, ...] ^ [params]]
                let is_exclusion = if self.peek().kind == TokenKind::LBracket {
                    let mut is_excl = false;
                    let mut look = 1;
                    let mut depth = 1;
                    while let Some(tok) = self.peek_ahead(look) {
                        if tok.kind == TokenKind::LBracket {
                            depth += 1;
                        } else if tok.kind == TokenKind::RBracket {
                            depth -= 1;
                            if depth == 0 {
                                if self.peek_ahead(look + 1).map(|t| &t.kind) == Some(&TokenKind::Caret) {
                                    is_excl = true;
                                }
                                break;
                            }
                        }
                        look += 1;
                    }
                    is_excl
                } else {
                    false
                };

                if is_exclusion {
                    self.advance(); // consume inner '['
                    let mut excluded = Vec::new();
                    while !self.is_at_end() && self.peek().kind != TokenKind::RBracket {
                        if self.peek().kind == TokenKind::LBrace
                            && self.peek_ahead(1).map(|t| &t.kind) == Some(&TokenKind::RBrace)
                        {
                            self.advance();
                            self.advance();
                            excluded.push(MacroDelimiter::Brace);
                        } else if self.peek().kind == TokenKind::LBracket
                            && self.peek_ahead(1).map(|t| &t.kind) == Some(&TokenKind::RBracket)
                        {
                            self.advance();
                            self.advance();
                            excluded.push(MacroDelimiter::Bracket);
                        } else if self.peek().kind == TokenKind::LParen
                            && self.peek_ahead(1).map(|t| &t.kind) == Some(&TokenKind::RParen)
                        {
                            self.advance();
                            self.advance();
                            excluded.push(MacroDelimiter::Paren);
                        } else if self.peek().kind == TokenKind::Less
                            && self.peek_ahead(1).map(|t| &t.kind) == Some(&TokenKind::Greater)
                        {
                            self.advance();
                            self.advance();
                            excluded.push(MacroDelimiter::Angle);
                        } else if self.peek().kind == TokenKind::Comma {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    self.consume(TokenKind::RBracket, "Expected ']' after excluded delimiter list")?;
                    self.consume(TokenKind::Caret, "Expected '^' after excluded delimiter array")?;
                    self.consume(TokenKind::LBracket, "Expected '[' for parameter list in exclusion pattern")?;
                    let mut group_params = Vec::new();
                    while !self.is_at_end() && self.peek().kind != TokenKind::RBracket {
                        let p = self.parse_single_param()?;
                        group_params.push(p.clone());
                        params.push(p);
                        if self.peek().kind == TokenKind::Comma {
                            self.advance();
                        }
                    }
                    self.consume(TokenKind::RBracket, "Expected ']' after parameter list")?;
                    self.consume(TokenKind::RBracket, "Expected outer ']' to close exclusion definition")?;
                    pattern.push(MacroPatternItem::ParamGroup {
                        delimiter: MacroDelimiter::Excluded(excluded),
                        params: group_params,
                    });
                } else {
                    let delimiter = match self.peek().kind {
                        TokenKind::LBracket => {
                            self.advance();
                            MacroDelimiter::Bracket
                        }
                        TokenKind::LBrace => {
                            self.advance();
                            MacroDelimiter::Brace
                        }
                        TokenKind::LParen => {
                            self.advance();
                            MacroDelimiter::Paren
                        }
                        TokenKind::Less => {
                            self.advance();
                            MacroDelimiter::Angle
                        }
                        TokenKind::Pipe => {
                            self.advance();
                            MacroDelimiter::Pipe
                        }
                        _ => MacroDelimiter::None,
                    };
                    let mut group_params = Vec::new();
                    let close_token = match &delimiter {
                        MacroDelimiter::Bracket => Some(TokenKind::RBracket),
                        MacroDelimiter::Brace => Some(TokenKind::RBrace),
                        MacroDelimiter::Paren => Some(TokenKind::RParen),
                        MacroDelimiter::Angle => Some(TokenKind::Greater),
                        MacroDelimiter::Pipe => Some(TokenKind::Pipe),
                        _ => None,
                    };

                    let stop_condition = |k: &TokenKind| {
                        if let Some(ref close) = close_token {
                            k == close
                        } else {
                            k == &TokenKind::RBracket
                        }
                    };

                    while !self.is_at_end() && !stop_condition(&self.peek().kind) {
                        let p = self.parse_single_param()?;
                        if delimiter == MacroDelimiter::None {
                            if matches!(&p.type_node, BaseType::Block { .. } | BaseType::Object) {
                                return Err(format!(
                                    "Syntax Error: block or object not allowed in non-delimiter parameter '{}'. You must use a delimiter instead.",
                                    p.name
                                ));
                            }
                        }
                        group_params.push(p.clone());
                        params.push(p);
                        if self.peek().kind == TokenKind::Comma {
                            self.advance();
                        }
                    }

                    if let Some(close) = close_token {
                        self.consume(close, "Expected closing delimiter for parameter group")?;
                    }
                    self.consume(TokenKind::RBracket, "Expected ']' to close macro parameter definition")?;

                    pattern.push(MacroPatternItem::ParamGroup {
                        delimiter,
                        params: group_params,
                    });
                }
            } else if self.peek().kind == TokenKind::LParen {
                self.advance(); // consume '('
                let mut group_params = Vec::new();
                while !self.is_at_end() && self.peek().kind != TokenKind::RParen {
                    let p = self.parse_single_param()?;
                    group_params.push(p.clone());
                    params.push(p);
                    if self.peek().kind == TokenKind::Comma {
                        self.advance();
                    }
                }
                self.consume(TokenKind::RParen, "Expected ')' after parameter list")?;
                pattern.push(MacroPatternItem::ParamGroup {
                    delimiter: MacroDelimiter::Paren,
                    params: group_params,
                });
            } else if self.peek().kind == TokenKind::Pipe {
                self.advance(); // consume '|'
                let mut group_params = Vec::new();
                while !self.is_at_end() && self.peek().kind != TokenKind::Pipe {
                    let p = self.parse_single_param()?;
                    group_params.push(p.clone());
                    params.push(p);
                    if self.peek().kind == TokenKind::Comma {
                        self.advance();
                    }
                }
                self.consume(TokenKind::Pipe, "Expected '|' after pipe parameters")?;
                pattern.push(MacroPatternItem::ParamGroup {
                    delimiter: MacroDelimiter::Pipe,
                    params: group_params,
                });
            } else {
                break;
            }
        }

        if self.peek().kind == TokenKind::Arrow {
            self.advance();
            if self.peek().kind != TokenKind::LBrace {
                let _ = self.parse_base_type()?;
            }
        }

        let body = if self.peek().kind == TokenKind::LBrace {
            self.advance();
            let mut stmts = Vec::new();
            while !self.is_at_end() && self.peek().kind != TokenKind::RBrace {
                let start_pos = self.current;
                match self.parse_statement(ScopeType::Block)? {
                    Some(stmt) => stmts.push(stmt),
                    None => {
                        if self.current == start_pos
                            && !self.is_at_end()
                            && self.peek().kind != TokenKind::RBrace
                        {
                            self.advance();
                        }
                    }
                }
            }
            self.consume(TokenKind::RBrace, "Expected '}' to close macro body")?;
            stmts
        } else {
            match self.parse_statement(ScopeType::Block)? {
                Some(stmt) => vec![stmt],
                None => vec![],
            }
        };

        if self.peek().kind == TokenKind::SemiColon {
            self.advance();
        }

        for g in &generics {
            if let BaseType::GenericParam(gen_name) = g {
                self.current_generics.remove(gen_name);
            }
        }

        let decl = Decl::MacroDecl {
            visibility: Visibility::Private,
            name,
            generics,
            params,
            pattern,
            body,
        };
        self.macro_patterns.push(decl.clone());
        Ok(decl)
    }
}
