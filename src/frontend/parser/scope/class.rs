use std::collections::HashMap;

use crate::frontend::lexer::token::TokenKind;
use crate::frontend::parser::ast::*;
use crate::frontend::parser::parser::Parser;

impl Parser {
    pub(crate) fn parse_class_decl(&mut self) -> Result<Decl, String> {
        let mut enabled_settings: Vec<Setting> = Vec::new();
        let mut used_settings: Vec<Setting> = Vec::new();
        let mut constructor: Option<Vec<ConstructorDecl>> = None;
        let mut used_handles: Vec<HandleMethods> = Vec::new();
        let mut handle_block: Vec<Decl> = Vec::new();
        let mut public_block: Vec<Decl> = Vec::new();
        let mut private_block: Vec<Decl> = Vec::new();
        let mut static_block: Vec<Decl> = Vec::new();
        let mut generics: Vec<BaseType> = Vec::new();

        //we need to ensure no duplicated extends
        let mut extends = None;
        let mut has_extends = false;
        self.advance(); // 'class'
        let name = self.get_identifier("Expected class name")?;
        //====================================================================
        // generic <> :  class name<...> extends <name> -> { ... }
        //====================================================================
        if self.peek().kind == TokenKind::Less {
            self.parse_generics(&mut generics)?;
            for g in &generics {
                if let BaseType::GenericParam(gen_name) = g {
                    self.current_generics.insert(gen_name.clone());
                }
            }
        }
        let mut meta = TypeMetadata {
            name: name.clone(),
            ty: BaseType::Class { name: name.clone(), fields: Box::new(HashMap::new()), methods: Box::new(HashMap::new()), constructor: None, generics: vec![] },
            fields: HashMap::new(),
            constructor: None,
            methods: HashMap::new(),
            handles: Vec::new(),
            handle_signatures: HashMap::new(),
            vars: HashMap::new(),
            variants: None,
        };
        //adding the default settings to the class scope
        enabled_settings.push(Setting::Private);
        enabled_settings.push(Setting::Public);
        enabled_settings.push(Setting::Static);
        enabled_settings.push(Setting::Extends);
        // adding allowed handles
        //we have display , iterator , next , length , size
        if self.peek().kind == TokenKind::Extends {
            self.advance();
            extends = Some(self.get_identifier("Expected parent class name after 'extends'")?);
            has_extends = true;
        }

        if self.peek().kind == TokenKind::Arrow {
            self.advance();
        }
        self.consume(TokenKind::LBrace, "Expected '{' to open class body")?;

        while !self.is_at_end() && self.peek().kind != TokenKind::RBrace {
            // we need to check if the token is valid for the setting
            let t = self.peek().kind.clone();
            if self.is_valid_setting(t.clone()) {
                // now need to know what is this section
                //====================================================================
                // constructor    _ () -> { ... }
                //====================================================================
                if t == TokenKind::Constructor && !used_settings.contains(&Setting::Constructor) {
                    match self.parse_constructor_decl(&mut meta) {
                        Ok(c) => {
                            constructor = c;
                        }
                        Err(e) => {
                            self.synchronize();
                            return Err(format!("Syntax Error in scope constructor: {}", e));
                        }
                    }
                    used_settings.push(Setting::Constructor);
                    continue;
                }
                //====================================================================
                // handle -> { fn1 , fn2 , ... }
                //====================================================================
                if t == TokenKind::Handle && !used_settings.contains(&Setting::Handle) {
                    self.advance(); // consume 'handle'
                    let allowed = self.get_allowed_handle(&meta.ty)?;
                    handle_block = self.parse_handle_body(&mut used_handles, allowed)?;
                    used_settings.push(Setting::Handle);
                    continue;
                }
                //====================================================================
                // public -> { ... }
                //====================================================================
                if
                    self.peek().kind == TokenKind::Public &&
                    !used_settings.contains(&Setting::Public)
                {
                    public_block = self.parse_field_block(&mut meta, Visibility::Public)?;
                    used_settings.push(Setting::Public);
                    continue;
                }
                //====================================================================
                // private -> { ... }
                //====================================================================
                if t == TokenKind::Private && !used_settings.contains(&Setting::Private) {
                    private_block = self.parse_field_block(&mut meta, Visibility::Private)?;
                    used_settings.push(Setting::Private);
                    continue;
                }
                //====================================================================
                // static -> { ... }
                //====================================================================
                if t == TokenKind::Static && !used_settings.contains(&Setting::Static) {
                    static_block = self.parse_field_block(&mut meta, Visibility::Static)?;
                    used_settings.push(Setting::Static);
                    continue;
                }

                //====================================================================
                // extends -> <name>;
                //====================================================================
                if t == TokenKind::Extends && !used_settings.contains(&Setting::Extends) {
                    if has_extends {
                        return Err(
                            "Syntax Error: Class can only have one extends and you already have one".to_string()
                        );
                    } else {
                        self.advance(); // 'extends'
                        self.consume(TokenKind::Arrow, "Expected '->' after 'extends'")?;
                        extends = Some(
                            self.get_identifier("Expected parent class name after 'extends'")?
                        );
                        used_settings.push(Setting::Extends);
                        continue;
                    }
                }

                return Err(format!(
                    "Syntax Error: Duplicate or misplaced section '{:?}' in class '{}' at line {}, column {}",
                    t,
                    name,
                    self.peek().line,
                    self.peek().column
                ));
            } else {
                print!(
                    "DEBUG: Invalid field found : {} , that is not allow in the array typed scope to use it \n\t - use custom typed scope with enable some setting it will work if it valid",
                    t.as_str()
                );
                return Err(
                    format!(
                        "Syntax Error: Invalid field ''{:?}'' declaration at line {}, column {}",
                        t,
                        self.peek().line,
                        self.peek().column
                    )
                );
            }
        }
        self.consume(TokenKind::RBrace, "Expected '}' to close class body")?;
        if self.peek().kind == TokenKind::SemiColon {
            self.advance();
        }

        meta.handles = used_handles.clone();
        for decl in public_block
            .iter()
            .chain(private_block.iter())
            .chain(static_block.iter())
        {
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
        for g in &generics {
            if let BaseType::GenericParam(gen_name) = g {
                self.current_generics.remove(gen_name);
            }
        }
        Ok(Decl::ClassDecl {
            visibility: Visibility::Private,
            name,
            extends,
            handles: used_handles,
            public_block,
            private_block,
            static_block,
            generics,
            handle_block,
            constructor,
        })
    }
}
