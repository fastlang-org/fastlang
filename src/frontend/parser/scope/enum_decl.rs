use crate::frontend::lexer::token::TokenKind;
use crate::frontend::parser::ast::*;
use crate::frontend::parser::parser::Parser;

impl Parser {
    pub(crate) fn parse_enum_decl(&mut self) -> Result<Decl, String> {
        let mut variants: Vec<EnumVariant> = Vec::new();
        if self.peek().kind == TokenKind::TypeEnum {
            self.advance(); // consume 'enum'
        }
        let name = self.get_identifier("Expected enum name")?;
        let mut generics = Vec::new();
        if self.peek().kind == TokenKind::Less {
            self.parse_generics(&mut generics)?;
            for g in &generics {
                if let BaseType::GenericParam(gen_name) = g {
                    self.current_generics.insert(gen_name.clone());
                }
            }
        }
        let mut underlying_type = None;
        if self.peek().kind == TokenKind::Colon {
            self.advance(); // consume ':'
            underlying_type = Some(self.parse_type()?);
        } else if self.peek().kind == TokenKind::Using {
            self.advance(); // consume 'using'
            underlying_type = Some(self.parse_type()?);
        }
        if self.peek().kind == TokenKind::Arrow {
            self.advance();
        }
        self.consume(TokenKind::LBrace, "Expected '{' to open enum body")?;
        let mut current_counter: i64 = 0;
        let mut step: i64 = 1;
        let mut is_first: bool = true;
        let mut saved_counter_before_jump: Option<i64> = None;
        let mut has_jumped: bool = false;

        while !self.is_at_end() && self.peek().kind != TokenKind::RBrace {
            let t = self.peek().kind.clone();

            if t == TokenKind::Set {
                self.advance(); // consume 'set'
                let var_name = self.get_identifier("Expected 'i' or 'iota' after 'set' in enum")?;
                if var_name != "i" && var_name != "iota" {
                    return Err(format!("Syntax Error: Expected 'i' or 'iota' after 'set' in enum, found '{}'", var_name));
                }
                self.consume(TokenKind::Assign, "Expected '=' after set i")?;
                let step_expr = self.parse_expression()?;
                let step_val = match step_expr {
                    Expr::LiteralInt(v) => v as i64,
                    Expr::LiteralUInt(v) => v as i64,
                    _ => return Err("Syntax Error: 'set i' step must be an integer literal".to_string()),
                };
                step = step_val;
                self.consume(TokenKind::SemiColon, "Expected ';' after set statement in enum")?;
                continue;
            }

            if t == TokenKind::Continue {
                self.advance(); // consume 'continue'
                self.consume(TokenKind::SemiColon, "Expected ';' after continue in enum")?;
                if let Some(saved) = saved_counter_before_jump.take() {
                    current_counter = saved;
                    has_jumped = false;
                }
                continue;
            }

            let is_paren_prefix = if self.peek().kind == TokenKind::LParen {
                let mut offset = 1;
                let mut depth = 1;
                while let Some(tok) = self.peek_ahead(offset) {
                    if tok.kind == TokenKind::LParen {
                        depth += 1;
                    } else if tok.kind == TokenKind::RParen {
                        depth -= 1;
                        if depth == 0 {
                            offset += 1;
                            break;
                        }
                    }
                    offset += 1;
                }
                if let Some(next) = self.peek_ahead(offset) {
                    next.kind == TokenKind::Assign
                } else {
                    false
                }
            } else {
                false
            };

            if t == TokenKind::LBracket || is_paren_prefix || matches!(t, TokenKind::Identifier(_)) {
                let mut is_default = false;
                let discriminant = if is_paren_prefix {
                    self.advance(); // consume '('
                    let disc_expr = if self.peek().kind == TokenKind::RParen {
                        None
                    } else {
                        Some(self.parse_expression()?)
                    };
                    self.consume(TokenKind::RParen, "Expected ')' after default enum variant")?;
                    self.consume(TokenKind::Assign, "Expected '=' after default variant")?;
                    is_default = true;
                    if let Some(ref expr) = disc_expr {
                        if let Expr::LiteralInt(v) = expr {
                            current_counter = *v as i64;
                        } else if let Expr::LiteralUInt(v) = expr {
                            current_counter = *v as i64;
                        }
                        is_first = false;
                        Some(expr.clone())
                    } else {
                        if is_first {
                            current_counter = 0;
                            is_first = false;
                        } else {
                            current_counter += step;
                        }
                        Some(Expr::LiteralInt(current_counter as i128))
                    }
                } else if self.peek().kind == TokenKind::LBracket {
                    self.advance(); // consume '['
                    let disc_expr = if self.peek().kind == TokenKind::RBracket {
                        None
                    } else {
                        Some(self.parse_expression()?)
                    };
                    self.consume(
                        TokenKind::RBracket,
                        "Expected ']' after enum variant discriminant",
                    )?;
                    self.consume(
                        TokenKind::Assign,
                        "Expected '=' after '[...]' in enum variant declaration",
                    )?;
                    if !is_first && !has_jumped {
                        saved_counter_before_jump = Some(current_counter);
                        has_jumped = true;
                    }
                    if let Some(ref expr) = disc_expr {
                        if let Expr::LiteralInt(v) = expr {
                            current_counter = *v as i64;
                        } else if let Expr::LiteralUInt(v) = expr {
                            current_counter = *v as i64;
                        }
                        is_first = false;
                        Some(expr.clone())
                    } else {
                        if is_first {
                            current_counter = 0;
                            is_first = false;
                        } else {
                            current_counter += step;
                        }
                        Some(Expr::LiteralInt(current_counter as i128))
                    }
                } else {
                    if is_first {
                        current_counter = 0;
                        is_first = false;
                    } else {
                        current_counter += step;
                    }
                    Some(Expr::LiteralInt(current_counter as i128))
                };

                let variant_name = self.get_identifier("Expected enum variant name")?;
                let mut payload = EnumVariantPayload::None;
                let mut data_type = None;

                if self.peek().kind == TokenKind::LParen {
                    self.advance(); // consume '('
                    let mut tuple_types = Vec::new();
                    if self.peek().kind != TokenKind::RParen {
                        loop {
                            let dt = self.parse_type()?;
                            tuple_types.push(dt);
                            if self.peek().kind == TokenKind::Comma {
                                self.advance();
                            } else {
                                break;
                            }
                        }
                    }
                    self.consume(TokenKind::RParen, "Expected ')' after enum variant payload")?;
                    data_type = tuple_types.first().cloned();
                    payload = EnumVariantPayload::Tuple(tuple_types);
                } else if self.peek().kind == TokenKind::LBrace {
                    self.advance(); // consume '{'
                    let mut struct_fields = Vec::new();
                    while !self.is_at_end() && self.peek().kind != TokenKind::RBrace {
                        // Support both: `Type name;` and `name: Type;`
                        let is_colon_syntax = if let Some(next_tok) =
                            self.tokens.get(self.current + 1)
                        {
                            next_tok.kind == TokenKind::Colon || next_tok.kind == TokenKind::Walrus
                        } else {
                            false
                        };

                        if is_colon_syntax {
                            let p = self.parse_single_param()?;
                            struct_fields.push(p);
                        } else {
                            let field_type = self.parse_type()?;
                            let field_name = self.get_identifier("Expected field name")?;
                            let default_val = if self.peek().kind == TokenKind::Assign
                                || self.peek().kind == TokenKind::Walrus
                            {
                                self.advance();
                                Some(self.parse_expression()?)
                            } else {
                                None
                            };
                            struct_fields.push(Param {
                                name: field_name,
                                type_node: field_type,
                                default_value: default_val,
                                is_variadic: false,
                            });
                        }
                        if self.peek().kind == TokenKind::SemiColon
                            || self.peek().kind == TokenKind::Comma
                        {
                            self.advance();
                        }
                    }
                    self.consume(
                        TokenKind::RBrace,
                        "Expected '}' to close struct variant body",
                    )?;
                    payload = EnumVariantPayload::Struct(struct_fields);
                }

                if self.peek().kind == TokenKind::Comma || self.peek().kind == TokenKind::SemiColon
                {
                    self.advance();
                }

                variants.push(EnumVariant {
                    name: variant_name,
                    discriminant,
                    payload,
                    data_type,
                    is_default,
                });
            } else {
                for g in &generics {
                    if let BaseType::GenericParam(gen_name) = g {
                        self.current_generics.remove(gen_name);
                    }
                }
                return Err(format!(
                    "Syntax Error: Unsupported token '{}' in enum at line {}, column {}",
                    t.as_str(),
                    self.peek().line,
                    self.peek().column
                ));
            }
        }
        let mut meta = TypeMetadata {
            name: name.clone(),
            ty: BaseType::Enum {
                name: name.clone(),
                underlying_type: underlying_type.clone().map(Box::new),
                variants: variants.clone(),
                methods: Box::new(std::collections::HashMap::new()),
                generics: generics.clone(),
            },
            fields: std::collections::HashMap::new(),
            constructor: None,
            methods: std::collections::HashMap::new(),
            handles: Vec::new(),
            handle_signatures: std::collections::HashMap::new(),
            vars: std::collections::HashMap::new(),
            variants: Some(variants.clone()),
        };
        for variant in &variants {
            meta.fields
                .insert(variant.name.clone(), BaseType::from_str(&name));

            let variant_full_name = format!("{}::{}", name, variant.name);
            let mut var_fields = std::collections::HashMap::new();
            match &variant.payload {
                EnumVariantPayload::None => {}
                EnumVariantPayload::Tuple(types) => {
                    if let Some(first_t) = types.first() {
                        var_fields.insert("this".to_string(), first_t.clone());
                    }
                    for (i, t) in types.iter().enumerate() {
                        var_fields.insert(format!("_{}", i), t.clone());
                    }
                }
                EnumVariantPayload::Struct(fields) => {
                    for f in fields {
                        var_fields.insert(f.name.clone(), f.type_node.clone());
                    }
                }
            }
            let var_meta = TypeMetadata {
                name: variant_full_name.clone(),
                ty: BaseType::Blueprint {
                    name: variant_full_name.clone(),
                    fields: Box::new(var_fields.clone()),
                    methods: Box::new(std::collections::HashMap::new()),
                    generics: generics.clone(),
                },
                fields: var_fields,
                constructor: None,
                methods: std::collections::HashMap::new(),
                handles: Vec::new(),
                handle_signatures: std::collections::HashMap::new(),
                vars: std::collections::HashMap::new(),
                variants: None,
            };
            self.metadata.insert(variant_full_name, var_meta);
        }
        self.metadata.insert(name.clone(), meta);

        self.consume(TokenKind::RBrace, "Expected '}' to close enum block")?;
        if self.peek().kind == TokenKind::SemiColon {
            self.advance();
        }

        for g in &generics {
            if let BaseType::GenericParam(gen_name) = g {
                self.current_generics.remove(gen_name);
            }
        }

        Ok(Decl::EnumDecl {
            visibility: Visibility::Private,
            name,
            underlying_type,
            generics,
            handles: Vec::new(),
            handle_block: Vec::new(),
            variants,
        })
    }
}
