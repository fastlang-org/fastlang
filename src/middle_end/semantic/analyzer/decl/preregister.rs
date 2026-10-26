use super::super::*;
use std::rc::Rc;
use std::collections::HashMap;

pub fn make_sig_key(name: &str, params: &[Param]) -> String {
    let types: Vec<String> = params.iter().map(|p| p.type_node.as_str()).collect();
    format!("{}#{}", name, types.join(","))
}

impl SemanticAnalyzer {
    pub fn pre_register_decl(&mut self, decl: &Decl) -> Result<(), String> {
        match decl {
            Decl::NamespaceDecl { name, decls, .. } => {
                let parent_env = Rc::clone(&self.current_env);
                let ns_env = self.current_env.borrow_mut().get_or_create_namespace(name, parent_env);
                let prev_env = Rc::clone(&self.current_env);
                self.current_env = Rc::clone(&ns_env);
                for d in decls {
                    self.pre_register_decl(d)?;
                }
                self.current_env = prev_env;
            }
            Decl::CompileDecl { name, decls } => {
                for d in decls {
                    match d {
                        Decl::MacroDecl {
                            name: m_name,
                            params,
                            body,
                            visibility,
                            ..
                        } => {
                            let namespaced_name = format!("{}::{}", name, m_name);
                            let short_name = m_name.as_str();
                            let without_dollar = short_name.trim_start_matches('$');
                            let mut aliases = vec![
                                namespaced_name.clone(),
                                format!("{}::{}", name, without_dollar),
                                short_name.to_string(),
                                without_dollar.to_string(),
                            ];
                            aliases.sort();
                            aliases.dedup();
                            for alias in aliases {
                                let alias_info = SymbolInfo {
                                    name: alias.clone(),
                                    kind: SymbolKind::Macro {
                                        params: params.clone(),
                                        return_type: BaseType::Void,
                                        body: body.clone(),
                                    },
                                    visibility: visibility.clone(),
                                    dependencies: vec![],
                                    is_used: false,
                                    is_param: false,
                                    is_uninitialized: false,
                                    is_compilable: true,
                                };
                                self.current_env
                                    .borrow_mut()
                                    .define_or_update(alias, alias_info);
                            }
                        }
                        Decl::FnDecl {
                            name: fn_name,
                            generics,
                            params,
                            return_type,
                            body,
                            ..
                        } => {
                            let f_name = format!("{}::{}", name, fn_name);
                            let fn_info = SymbolInfo {
                                name: f_name.clone(),
                                kind: SymbolKind::Function {
                                    params: params.clone(),
                                    return_type: return_type.clone(),
                                    generics: generics.clone(),
                                    body: Some(body.clone()),
                                },
                                visibility: Visibility::Public,
                                dependencies: vec![],
                                is_used: false,
                                is_param: false,
                                is_uninitialized: false,
                                is_compilable: true,
                            };
                            self.current_env
                                .borrow_mut()
                                .define_or_update(f_name.clone(), fn_info);

                            let sig = FnSignature {
                                name: f_name.clone(),
                                generics: generics.clone(),
                                params: params.clone(),
                                return_type: return_type.clone(),
                                is_virtual: false,
                                is_abstract: false,
                            };
                            let entry = self.fn_overloads.entry(f_name.clone()).or_default();
                            if !entry.iter().any(|s| s.params == sig.params && s.generics == sig.generics) {
                                entry.push(sig);
                            }
                        }
                        Decl::ImplDecl {
                            target,
                            target_generics,
                            methods,
                            handle_block,
                            static_block,
                            ..
                        } => {
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
                        let entry =
                            self.global_metadata
                                .entry(spec_key.clone())
                                .or_insert_with(|| TypeMetadata {
                                    name: spec_key.clone(),
                                    ty: BaseType::from_impl_target(target, target_generics),
                                    fields: std::collections::HashMap::new(),
                                    methods: std::collections::HashMap::new(),
                                    constructor: None,
                                    handles: vec![],
                                    handle_signatures: std::collections::HashMap::new(),
                                    vars: std::collections::HashMap::new(),
                                    variants: None,
                                });
                        if !target_generics.is_empty() {
                            entry.ty = BaseType::from_impl_target(target, target_generics);
                        }
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
                        for h in handle_block {
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
                    }
                    _ => {}
                }
            }
        }
            Decl::MacroDecl {
                name,
                params,
                body,
                visibility,
                ..
            } => {
                // Ghost macro names (_name_) must NOT be stripped of underscores
                // or aliased to plain names -- doing so causes false conflicts with
                // regular functions that happen to share the inner word (e.g.,
                // `_range_` must never inject `"range"` into the symbol table).
                // Only register the exact name and the `\$`-stripped variant.
                let without_dollar = name.trim_start_matches('$');
                let mut aliases = vec![name.clone()];
                if without_dollar != name {
                    aliases.push(without_dollar.to_string());
                }
                aliases.dedup();
                for alias in aliases {
                    self.macro_overloads
                        .entry(alias.clone())
                        .or_default()
                        .push((params.clone(), BaseType::Void, body.clone()));
                    let alias_info = SymbolInfo {
                        name: alias.clone(),
                        kind: SymbolKind::Macro {
                            params: params.clone(),
                            return_type: BaseType::Void,
                            body: body.clone(),
                        },
                        visibility: visibility.clone(),
                        dependencies: vec![],
                        is_used: false,
                        is_param: false,
                        is_uninitialized: false,
                        is_compilable: true,
                    };
                    self.current_env
                        .borrow_mut()
                        .define_or_update(alias, alias_info);
                }
            }
            Decl::ImplDecl {
                target,
                target_generics,
                methods,
                handle_block,
                static_block,
                ..
            } => {
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
                    .global_metadata
                    .entry(spec_key.clone())
                    .or_insert_with(|| TypeMetadata {
                        name: spec_key.clone(),
                        ty: BaseType::from_impl_target(target, target_generics),
                        fields: std::collections::HashMap::new(),
                        methods: std::collections::HashMap::new(),
                        constructor: None,
                        handles: vec![],
                        handle_signatures: std::collections::HashMap::new(),
                        vars: std::collections::HashMap::new(),
                        variants: None,
                    });
                if !target_generics.is_empty() {
                    entry.ty = BaseType::from_impl_target(target, target_generics);
                }
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
                for h in handle_block {
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
                let mut bp_entry = self
                    .current_env
                    .borrow()
                    .lookup_blueprint(target)
                    .unwrap_or_else(|| BlueprintData::new(target.clone()));
                for h in handle_block {
                    if let Decl::FnDecl { name: h_name, .. } = h {
                        let hk = HandleMethods::from_str(h_name.as_str());
                        if hk != HandleMethods::NotFound && !bp_entry.handles.contains(&hk) {
                            bp_entry.handles.insert(hk);
                        }
                    }
                }
                self.current_env
                    .borrow_mut()
                    .define_blueprint(target.clone(), bp_entry);
            }
            Decl::FnDecl {
                visibility,
                name,
                generics,
                params,
                return_type,
                body,
                is_virtual,
                is_abstract,
                ..
            } => {
                let sig = FnSignature {
                    name: name.clone(),
                    generics: generics.clone(),
                    params: params.clone(),
                    return_type: return_type.clone(),
                    is_virtual: *is_virtual,
                    is_abstract: *is_abstract,
                };
                let entry = self.fn_overloads.entry(name.clone()).or_default();
                if !entry.iter().any(|s| s.params == sig.params && s.generics == sig.generics) {
                    entry.push(sig);
                }

                let exec_mode = detect_execution_mode(body);
                let is_compilable_fn =
                    name.starts_with("@compile::") || exec_mode == ExecutionMode::FullyCompilable;

                let fn_info = SymbolInfo {
                    name: name.clone(),
                    kind: SymbolKind::Function {
                        params: params.clone(),
                        return_type: return_type.clone(),
                        generics: generics.clone(),
                        body: Some(body.clone()),
                    },
                    visibility: visibility.clone(),
                    dependencies: vec![],
                    is_used: false,
                    is_param: false,
                    is_uninitialized: false,
                    is_compilable: is_compilable_fn,
                };
                self.current_env
                    .borrow_mut()
                    .define_or_update(name.clone(), fn_info);
            }
            Decl::FnAliasDecl { visibility, name, target } => {
                self.fn_aliases.insert(name.clone(), target.clone());
                if let Some(target_sigs) = self.fn_overloads.get(target).cloned() {
                    let mut alias_sigs = Vec::new();
                    for mut sig in target_sigs {
                        sig.name = name.clone();
                        alias_sigs.push(sig);
                    }
                    self.fn_overloads.insert(name.clone(), alias_sigs);
                }
                let fn_info = SymbolInfo {
                    name: name.clone(),
                    kind: SymbolKind::Function {
                        params: vec![],
                        return_type: BaseType::Unknown,
                        generics: vec![],
                        body: None,
                    },
                    visibility: visibility.clone(),
                    dependencies: vec![target.clone()],
                    is_used: false,
                    is_param: false,
                    is_uninitialized: false,
                    is_compilable: true,
                };
                self.current_env
                    .borrow_mut()
                    .define_or_update(name.clone(), fn_info);
            }
            Decl::BlueprintDecl {
                visibility,
                name,
                generics,
                definition,
                ..
            } => {
                let mut bp = self
                    .current_env
                    .borrow()
                    .lookup_blueprint(name)
                    .unwrap_or_else(|| BlueprintData::new(name.clone()));
                bp.generics = generics.iter().map(|g| g.as_str()).collect();
                if let BlueprintDef::Explicit(fields) = definition {
                    for f in fields {
                        bp.fields.insert(f.name.clone(), f.type_node.clone());
                    }
                }
                self.current_env
                    .borrow_mut()
                    .define_blueprint(name.to_string(), bp);
                let info = self.make_blueprint_symbol(name, visibility.clone());
                self.current_env
                    .borrow_mut()
                    .define_or_update(name.clone(), info);
            }
            Decl::ClassDecl {
                visibility,
                name,
                generics,
                extends,
                ..
            } => {
                let mut bp = self
                    .current_env
                    .borrow()
                    .lookup_blueprint(name)
                    .unwrap_or_else(|| BlueprintData::new(name.clone()));
                bp.is_class = true;
                bp.generics = generics.iter().map(|g| g.as_str()).collect();
                self.current_env
                    .borrow_mut()
                    .define_blueprint(name.to_string(), bp);
                let info = self.make_blueprint_symbol(name, visibility.clone());
                self.current_env
                    .borrow_mut()
                    .define_or_update(name.clone(), info);
                if let Some(parent) = extends {
                    self.class_hierarchy.insert(name.clone(), parent.clone());
                }
            }
            Decl::StructDecl {
                visibility,
                name,
                ..
            } => {
                let bp = self
                    .current_env
                    .borrow()
                    .lookup_blueprint(name)
                    .unwrap_or_else(|| BlueprintData::new(name.clone()));
                self.current_env
                    .borrow_mut()
                    .define_blueprint(name.to_string(), bp);
                let info = self.make_blueprint_symbol(name, visibility.clone());
                self.current_env
                    .borrow_mut()
                    .define_or_update(name.clone(), info);
            }
            Decl::EnumDecl {
                visibility,
                name,
                underlying_type,
                variants,
                generics,
                ..
            } => {
                let info = self.make_blueprint_symbol(name, visibility.clone());
                self.current_env
                    .borrow_mut()
                    .define_or_update(name.clone(), info);
                for v in variants {
                    let var_qualified = format!("{}::{}", name, v.name);
                    let bp_var_info = self.make_blueprint_symbol(&var_qualified, visibility.clone());
                    self.current_env
                        .borrow_mut()
                        .define_or_update(var_qualified.clone(), bp_var_info);
                    let var_symbol = SymbolInfo {
                        name: v.name.clone(),
                        kind: SymbolKind::Variable {
                            type_node: BaseType::from_str(name),
                            editability: Editability::NotEditable,
                            is_array: false,
                        },
                        visibility: visibility.clone(),
                        dependencies: vec![],
                        is_used: false,
                        is_param: false,
                        is_uninitialized: false,
                        is_compilable: false,
                    };
                    self.current_env
                        .borrow_mut()
                        .define_or_update(var_qualified, var_symbol);
                }
                self.global_metadata.insert(
                    name.clone(),
                    TypeMetadata {
                        name: name.clone(),
                        ty: BaseType::Enum {
                            name: name.clone(),
                            underlying_type: underlying_type.clone().map(Box::new),
                            variants: variants.clone(),
                            methods: Box::new(HashMap::new()),
                            generics: generics.clone(),
                        },
                        fields: HashMap::new(),
                        methods: HashMap::new(),
                        constructor: None,
                        handles: vec![],
                        handle_signatures: HashMap::new(),
                        vars: HashMap::new(),
                        variants: Some(variants.clone()),
                    },
                );
            }
            Decl::DefineDecl {
                name,
                type_alias: Some(t),
                visibility,
                ..
            } => {
                let info = SymbolInfo {
                    name: name.clone(),
                    kind: SymbolKind::Variable {
                        type_node: BaseType::Type(Box::new(t.clone())),
                        editability: Editability::NotEditable,
                        is_array: false,
                    },
                    visibility: visibility.clone(),
                    dependencies: vec![],
                    is_used: false,
                    is_param: false,
                    is_uninitialized: false,
                    is_compilable: true,
                };
                self.current_env
                    .borrow_mut()
                    .define_or_update(name.clone(), info);
            }
            Decl::DefineDecl { .. } => {}
            _ => {}
        }
        Ok(())
    }
}
