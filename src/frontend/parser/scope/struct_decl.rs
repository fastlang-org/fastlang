use std::collections::HashMap;

use crate::frontend::lexer::token::TokenKind;
use crate::frontend::parser::ast::*;
use crate::frontend::parser::parser::Parser;

impl Parser {
    pub(crate) fn parse_struct_decl(&mut self) -> Result<Decl, String> {
        let mut enabled_settings: Vec<Setting> = Vec::new();
        let mut used_settings: Vec<Setting> = Vec::new();
        let mut constructor: Option<Vec<ConstructorDecl>> = None;
        let mut used_handles: Vec<HandleMethods> = Vec::new();
        let mut handle_block: Vec<Decl> = Vec::new();
        let mut public_block_ast: Vec<Decl> = Vec::new();
        let mut private_block_ast: Vec<Decl> = Vec::new();
        let mut static_block_ast: Vec<Decl> = Vec::new();
        self.advance(); // consume 'struct'
        let name = self.get_identifier("Expected struct name")?;

        let mut meta = TypeMetadata {
            name: name.clone(), //[*]
            ty: BaseType::Struct { name: name.clone(), fields: Box::new(HashMap::new()), methods: Box::new(HashMap::new()), generics: vec![] },
            fields: HashMap::new(), //[*] we will add all of variables in the struct
            constructor: None, //[*]
            methods: HashMap::new(), //[*]
            handles: Vec::new(), //[*]
            handle_signatures: HashMap::new(),
            vars: HashMap::new(), //[]
            variants: None,
        };
        enabled_settings.push(Setting::Private);
        enabled_settings.push(Setting::Public);
        enabled_settings.push(Setting::Static);
        // adding allowed handles

        if self.peek().kind == TokenKind::Arrow {
            self.advance();
        }
        self.consume(TokenKind::LBrace, "Expected '{' to open struct body")?;
        // now we are the same as the one being redirected by the scope parsing fn

        while !self.is_at_end() && self.peek().kind != TokenKind::RBrace {
            // we need to check if the token is valid for the setting
            let t = self.peek().kind.clone();
            //todo : check if the settings are in the settings vec
            if self.is_valid_setting(t.clone()) {
                // now need to know what is this section
                //====================================================================
                // constructor    _ () -> { ... }
                //====================================================================
                if t == TokenKind::Constructor {
                    if used_settings.contains(&Setting::Constructor) {
                        return Err(
                            format!(
                                "Syntax Error: Duplicate 'constructor' block in struct '{}'  at line {}, column {}",
                                name,
                                self.peek().line,
                                self.peek().column
                            )
                        );
                    } else {
                        used_settings.push(Setting::Constructor);
                        match self.parse_constructor_decl(&mut meta) {
                            Ok(c) => {
                                constructor = c;
                            }
                            Err(e) => {
                                eprintln!("Syntax Error in scope constructor: {}", e);
                                self.synchronize();
                            }
                        }
                        continue;
                    }
                }
                //====================================================================
                // handle -> { fn1 , fn2 , ... }
                //====================================================================
                if t == TokenKind::Handle {
                    if used_settings.contains(&Setting::Handle) {
                        return Err(
                            format!(
                                "Syntax Error: Duplicate 'handle' block in struct '{}'  at line {}, column {}",
                                name,
                                self.peek().line,
                                self.peek().column
                            )
                        );
                    } else {
                        used_settings.push(Setting::Handle);
                        self.advance(); // consume 'handle'
                        let allowed = self.get_allowed_handle(&meta.ty)?;
                        handle_block = self.parse_handle_body(&mut used_handles, allowed)?;
                        continue;
                    }
                }
                //====================================================================
                // public -> { ... }
                //====================================================================
                if self.peek().kind == TokenKind::Public {
                    if used_settings.contains(&Setting::Public) {
                        return Err(
                            format!(
                                "Syntax Error: Duplicate 'public' block in struct '{}'  at line {}, column {}",
                                name,
                                self.peek().line,
                                self.peek().column
                            )
                        );
                    } else {
                        used_settings.push(Setting::Public);
                        public_block_ast = self.parse_field_block(&mut meta, Visibility::Public)?;
                        continue;
                    }
                }
                //====================================================================
                // private -> { ... }
                //====================================================================
                if t == TokenKind::Private {
                    if used_settings.contains(&Setting::Private) {
                        return Err(
                            format!(
                                "Syntax Error: Duplicate 'private' block in struct '{}'  at line {}, column {}",
                                name,
                                self.peek().line,
                                self.peek().column
                            )
                        );
                    } else {
                        used_settings.push(Setting::Private);
                        private_block_ast = self.parse_field_block(&mut meta, Visibility::Private)?;
                        continue;
                    }
                }
                //====================================================================
                // static -> { ... }
                //====================================================================
                if t == TokenKind::Static {
                    if used_settings.contains(&Setting::Static) {
                        return Err(
                            format!(
                                "Syntax Error: Duplicate 'static' block in struct '{}'  at line {}, column {}",
                                name,
                                self.peek().line,
                                self.peek().column
                            )
                        );
                    } else {
                        used_settings.push(Setting::Static);
                        static_block_ast = self.parse_field_block(&mut meta, Visibility::Static)?;
                        continue;
                    }
                }
            } else {
                let stmt = self.parse_statement(ScopeType::Block)?;
                if let Some(Stmt::Declaration(decl)) = stmt {
                    public_block_ast.push(decl);
                } else {
                    return Err(
                        format!(
                            "Syntax Error: Invalid field [{}] declaration at line {}, column {}",
                            t.as_str(),
                            self.peek().line,
                            self.peek().column
                        )
                    );
                }
            }
        }
        self.consume(TokenKind::RBrace, "Expected '}' to close struct body")?;
        meta.handles = used_handles.clone();
        for decl in public_block_ast
            .iter()
            .chain(private_block_ast.iter())
            .chain(static_block_ast.iter()) {
            match decl {
                Decl::VarDecl { name, type_node, .. } => {
                    meta.fields.insert(name.clone(), type_node.clone());
                }
                Decl::DestructureDecl { assignments, type_node, .. } => {
                    for (name, _) in assignments {
                        meta.fields.insert(name.clone(), type_node.clone());
                    }
                }
                Decl::FnDecl { name, generics, params, return_type, .. } => {
                    meta.methods.insert(name.clone(), FnType {
                        name: name.clone(),
                        generics: generics.clone(),
                        params: params.clone(),
                        return_type: return_type.clone(),
                        mode: ExecutionMode::Runtime,
                    });
                }
                _ => {}
            }
        }
        for hdl in &handle_block {
            if let Decl::FnDecl { name, generics, params, return_type, .. } = hdl {
                meta.handle_signatures.insert(name.clone(), FnType {
                    name: name.clone(),
                    generics: generics.clone(),
                    params: params.clone(),
                    return_type: return_type.clone(),
                    mode: ExecutionMode::Runtime,
                });
            }
        }
        self.metadata.insert(name.clone(), meta);

        Ok(Decl::StructDecl {
            visibility: Visibility::Private,
            name,
            handles: used_handles,
            public_block: public_block_ast,
            private_block: private_block_ast,
            handle_block,
            static_block: static_block_ast,
            constructor,
        })
    }
}
