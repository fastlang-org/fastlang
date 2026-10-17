use super::super::*;
use std::collections::HashMap;

impl SemanticAnalyzer {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn visit_fn_decl(
        &mut self,
        visibility: &Visibility,
        name: &str,
        params: &[Param],
        return_type: &BaseType,
        body: &[Stmt],
        is_virtual: bool,
        is_abstract: bool,
        generics: &[BaseType],
    ) -> Result<(), String> {
        if is_virtual && self.scope.current_context.is_none() {
            return Err(format!(
                "Semantic Error: 'virtual' modifier is only allowed in 'class' methods, not in standalone function '{}'",
                name
            ));
        }
        let mut resolved_params = params.to_vec();
        for p in &mut resolved_params {
            if let Some(ref def_val) = p.default_value {
                let inferred = self.visit_expression(def_val)?;
                if matches!(p.type_node, BaseType::Unknown) {
                    if let Some(bp) = self.current_env.borrow().lookup_blueprint(&inferred) {
                        if bp.is_class {
                            p.type_node = BaseType::Class {
                                name: inferred.clone(),
                                fields: Box::new(HashMap::new()),
                                methods: Box::new(HashMap::new()),
                                constructor: None,
                                generics: Vec::new(),
                            };
                        } else {
                            p.type_node = BaseType::Struct {
                                name: inferred.clone(),
                                fields: Box::new(HashMap::new()),
                                methods: Box::new(HashMap::new()),
                                generics: Vec::new(),
                            };
                        }
                    } else if let Some(meta) = self.global_metadata.get(&inferred) {
                        if meta.ty.as_str().starts_with("class") {
                            p.type_node = BaseType::Class {
                                name: inferred.clone(),
                                fields: Box::new(HashMap::new()),
                                methods: Box::new(HashMap::new()),
                                constructor: None,
                                generics: Vec::new(),
                            };
                        } else {
                            p.type_node = BaseType::Struct {
                                name: inferred.clone(),
                                fields: Box::new(HashMap::new()),
                                methods: Box::new(HashMap::new()),
                                generics: Vec::new(),
                            };
                        }
                    } else {
                        p.type_node = match inferred.as_str() {
                            "int" | "int32" => BaseType::Int(Size::S32),
                            "int8" => BaseType::Int(Size::S8),
                            "int16" => BaseType::Int(Size::S16),
                            "int64" => BaseType::Int(Size::S64),
                            "uint" | "uint32" => BaseType::UInt(Size::S32),
                            "uint8" => BaseType::UInt(Size::S8),
                            "uint16" => BaseType::UInt(Size::S16),
                            "uint64" => BaseType::UInt(Size::S64),
                            "float" | "float32" => BaseType::Float(Size::S32),
                            "float64" => BaseType::Float(Size::S64),
                            "bool" => BaseType::Bool,
                            "char" => BaseType::Char,
                            "uchar" => BaseType::UChar,
                            "str" => BaseType::Str,
                            "ustr" => BaseType::UStr,
                            _ => BaseType::Blueprint {
                                name: inferred.clone(),
                                fields: Box::new(HashMap::new()),
                                methods: Box::new(HashMap::new()),
                                generics: Vec::new(),
                            },
                        };
                    }
                } else if !self.types_are_compatible(&p.type_node.as_str(), &inferred) {
                    return Err(format!(
                        "Semantic Error: Default value for parameter '{}' has type '{}', incompatible with declared type '{}'.",
                        p.name, inferred, p.type_node.as_str()
                    ));
                }
            }
        }

        let mut seen_default = false;
        for p in &resolved_params {
            if p.default_value.is_some() {
                seen_default = true;
            } else if seen_default {
                return Err(format!(
                    "Semantic Error: Required parameter '{}' cannot follow a parameter with a default value in function '{}'.",
                    p.name, name
                ));
            }
        }

        let is_member_method = self.scope.is_member_context();

        if !is_member_method {
            let _is_var = resolved_params
                .last()
                .map(|p| p.is_variadic)
                .unwrap_or(false);
            let min_args = resolved_params
                .iter()
                .take_while(|p| p.default_value.is_none() && !p.is_variadic)
                .count();

            if let Some(existing_sigs) = self.fn_overloads.get(name) {
                for ex in existing_sigs {
                    if ex.params == resolved_params
                        && ex.generics == *generics
                        && ex.return_type == *return_type
                    {
                        continue;
                    }
                    let ex_min = ex
                        .params
                        .iter()
                        .take_while(|p| p.default_value.is_none() && !p.is_variadic)
                        .count();
                    let ex_max = ex.params.len();
                    for ex_len in ex_min..=ex_max {
                        let ex_prefix: Vec<String> = ex.params[..ex_len]
                            .iter()
                            .map(|p| p.type_node.as_str())
                            .collect();
                        for len in min_args..=resolved_params.len() {
                            let prefix: Vec<String> = resolved_params[..len]
                                .iter()
                                .map(|p| p.type_node.as_str())
                                .collect();
                            if prefix == ex_prefix {
                                return Err(format!(
                                    "Semantic Error: Overload conflict for function '{}': default parameters create an ambiguity with existing overload.",
                                    name
                                ));
                            }
                        }
                    }
                }
            }

            let sigs = self.fn_overloads.entry(name.to_string()).or_default();
            if !sigs.iter().any(|s| s.params == resolved_params && s.generics == *generics) {
                sigs.push(FnSignature {
                    name: name.to_string(),
                    generics: generics.to_vec(),
                    params: resolved_params.clone(),
                    return_type: return_type.clone(),
                    is_virtual,
                    is_abstract,
                });
            }
        }
        let exec_mode = detect_execution_mode(body);
        let is_compilable_fn =
            name.starts_with("@compile::") || exec_mode == ExecutionMode::FullyCompilable;

        let fn_info = SymbolInfo {
            name: name.to_string(),
            kind: SymbolKind::Function {
                params: resolved_params.clone(),
                return_type: return_type.clone(),
                generics: generics.to_vec(),
                body: Some(body.to_vec()),
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
            .define(name.to_string(), fn_info)?;

        let prev_in_generic = self.scope.in_generic_template;
        if !generics.is_empty() {
            self.scope.in_generic_template = true;
        }
        let prev_flags = self.active_flags.clone();
        let prev_return = self.active_return_type.clone();
        let prev_context = self.scope.current_context.clone();
        let prev_deleted = self.memory.deleted_vars.clone();
        let prev_heap = self.memory.heap_allocated_vars.clone();
        self.memory.deleted_vars.clear();
        self.memory.heap_allocated_vars.clear();
        let ctx_name = if let Some(ref tname) = self.scope.current_type_name {
            format!("{}::{}", tname, name)
        } else {
            name.to_string()
        };
        self.scope.current_context = Some(ctx_name.clone());
        let sig_key = super::make_sig_key(name, &resolved_params);
        self.dependency_graph
            .entry(sig_key)
            .or_default()
            .insert(ctx_name.clone());

        self.active_flags.insert(Flag::HasReturn);
        self.active_flags.insert(Flag::HasThrow);
        self.active_flags.insert(Flag::HasYield);
        self.active_flags.insert(Flag::HasLeave);
        self.active_return_type = Some(return_type.clone());

        self.enter_scope();
        for g in generics {
            let g_name = match g {
                BaseType::GenericParam(name) => name.clone(),
                BaseType::New(name) => name.clone(),
                _ => g.as_str(),
            };
            if !g_name.is_empty() && g_name != "unknown" {
                let clean_name = g_name.trim_start_matches("...").to_string();
                let g_info = SymbolInfo {
                    name: clean_name.clone(),
                    kind: SymbolKind::Variable {
                        type_node: BaseType::Type(Box::new(BaseType::GenericParam(
                            clean_name.clone(),
                        ))),
                        editability: Editability::NotEditable,
                        is_array: false,
                    },
                    visibility: Visibility::Public,
                    dependencies: vec![],
                    is_used: true,
                    is_param: false,
                    is_uninitialized: false,
                    is_compilable: false,
                };
                let _ = self
                    .current_env
                    .borrow_mut()
                    .define(clean_name.clone(), g_info.clone());
                if g_name != clean_name {
                    let _ = self.current_env.borrow_mut().define(g_name, g_info);
                }
            }
        }
        for p in &resolved_params {
            self.record_type_dependencies(&p.type_node);
            let (param_type, is_arr) = if p.is_variadic {
                (
                    BaseType::Array {
                        base_type: Box::new(self.resolve_type(&p.type_node)),
                        size: Box::new(None),
                    },
                    true,
                )
            } else {
                (self.resolve_type(&p.type_node), false)
            };
            let param_info = SymbolInfo {
                name: p.name.clone(),
                kind: SymbolKind::Variable {
                    type_node: param_type,
                    editability: Editability::Editable,
                    is_array: is_arr,
                },
                visibility: Visibility::Public,
                dependencies: vec![],
                is_used: body.is_empty() || is_abstract || p.name.starts_with('_'),
                is_param: true,
                is_uninitialized: false,
                is_compilable: false,
            };
            self.current_env
                .borrow_mut()
                .define(p.name.clone(), param_info)?;
        }

        let mut resolved_fn_return = return_type.clone();
        if let BaseType::TypeExpr(expr) = return_type {
            let ret_t = self.visit_expression(expr)?;
            let clean_t = if ret_t.starts_with("type<") && ret_t.ends_with('>') {
                strip_wrapper(&ret_t, "type<").to_string()
            } else {
                ret_t
            };
            if generics.iter().any(|g| g.as_str() == clean_t)
                || clean_t == "generic"
                || clean_t.starts_with("Generic")
            {
                return Err("Semantic Error: 'typeof' is currently not supported for generic or runtime types. This feature will be supported in future versions.".to_string());
            }
            resolved_fn_return = BaseType::from_str(&clean_t);
            if let Some(sigs) = self.fn_overloads.get_mut(name) {
                for sig in sigs.iter_mut() {
                    if matches!(sig.return_type, BaseType::TypeExpr(_)) {
                        sig.return_type = resolved_fn_return.clone();
                    }
                }
            }
            let info_opt = self.current_env.borrow().lookup(name);
            if let Some(mut info) = info_opt {
                if let SymbolKind::Function { ref mut return_type, .. } = info.kind {
                    *return_type = resolved_fn_return.clone();
                }
                self.current_env.borrow_mut().define_or_update(name.to_string(), info);
            }
        }
        self.active_return_type = Some(resolved_fn_return.clone());

        self.record_type_dependencies(&resolved_fn_return);
        for s in body {
            self.visit_statement(s)?;
        }

        if name != "main"
            && !body.is_empty()
            && resolved_fn_return != BaseType::Void
            && !self.block_always_terminates(body)
        {
            return Err(
                format!(
                    "Semantic Error: Not all control paths return a value in function '{}' (declared return type '{}').",
                    name,
                    return_type.as_str()
                )
            );
        }

        self.leave_scope();

        self.active_flags = prev_flags;
        self.active_return_type = prev_return;
        self.scope.current_context = prev_context;
        self.memory.deleted_vars = prev_deleted;
        self.memory.heap_allocated_vars = prev_heap;
        self.scope.in_generic_template = prev_in_generic;
        Ok(())
    }

    pub(crate) fn visit_micro_decl(
        &mut self,
        visibility: &Visibility,
        name: &str,
        params: &[Param],
        return_type: &Option<BaseType>,
        body: &[Stmt],
        generics: &Option<Vec<String>>,
    ) -> Result<(), String> {
        for s in body {
            if matches!(s, Stmt::Declaration(Decl::VarDecl { .. } | Decl::DestructureDecl { .. } | Decl::ObjectDestructureDecl { .. } | Decl::ArrayDecl { .. })) {
                return Err(format!(
                    "Semantic Error: Local variable declarations are forbidden inside micro '{}'.",
                    name
                ));
            }
        }
        let prev_in_generic = self.scope.in_generic_template;
        if generics.as_ref().map(|g| !g.is_empty()).unwrap_or(false) {
            self.scope.in_generic_template = true;
        }
        let gen_types: Vec<BaseType> = generics
            .as_ref()
            .map(|v| v.iter().map(|s| BaseType::from_str(s)).collect())
            .unwrap_or_default();
        let micro_info = SymbolInfo {
            name: name.to_string(),
            kind: SymbolKind::Micro {
                params: params.to_vec(),
                generics: gen_types,
                body: body.to_vec(),
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
            .define_or_update(name.to_string(), micro_info);

        let prev_flags = self.active_flags.clone();
        let prev_return = self.active_return_type.clone();
        let prev_context = self.scope.current_context.clone();
        self.scope.current_context = Some(name.to_string());
        self.active_flags.insert(Flag::HasBreak);
        self.active_flags.insert(Flag::HasContinue);
        self.active_flags.insert(Flag::HasThrow);
        self.active_flags.insert(Flag::HasYield);
        self.active_flags.insert(Flag::HasLeave);
        self.active_flags.insert(Flag::HasReturn);
        self.enter_scope();
        for p in params {
            let param_info = SymbolInfo {
                name: p.name.clone(),
                kind: SymbolKind::Variable {
                    type_node: self.resolve_type(&p.type_node),
                    editability: Editability::Editable,
                    is_array: false,
                },
                visibility: Visibility::Public,
                dependencies: vec![],
                is_used: false,
                is_param: true,
                is_uninitialized: false,
                is_compilable: false,
            };
            self.current_env
                .borrow_mut()
                .define(p.name.clone(), param_info)?;
        }

        let mut resolved_method_return = return_type.clone();
        if let Some(BaseType::TypeExpr(expr)) = return_type {
            let ret_t = self.visit_expression(expr)?;
            let clean_t = if ret_t.starts_with("type<") && ret_t.ends_with('>') {
                strip_wrapper(&ret_t, "type<").to_string()
            } else {
                ret_t
            };
            if generics.as_ref().map(|v| v.iter().any(|g| g == &clean_t)).unwrap_or(false)
                || clean_t == "generic"
                || clean_t.starts_with("Generic")
            {
                return Err("Semantic Error: 'typeof' is currently not supported for generic or runtime types. This feature will be supported in future versions.".to_string());
            }
            resolved_method_return = Some(BaseType::from_str(&clean_t));
        }
        self.active_return_type = resolved_method_return;

        for s in body {
            self.visit_statement(s)?;
        }
        self.leave_scope();

        self.active_flags = prev_flags;
        self.active_return_type = prev_return;
        self.scope.current_context = prev_context;
        self.scope.in_generic_template = prev_in_generic;
        Ok(())
    }

    pub(crate) fn visit_macro_decl(
        &mut self,
        name: &str,
        params: &[Param],
        body: &[Stmt],
        visibility: &Visibility,
    ) -> Result<(), String> {
        let info = SymbolInfo {
            name: name.to_string(),
            kind: SymbolKind::Macro {
                params: params.to_vec(),
                return_type: BaseType::Void,
                body: body.to_vec(),
            },
            visibility: visibility.clone(),
            dependencies: vec![],
            is_used: false,
            is_param: false,
            is_uninitialized: false,
            is_compilable: false,
        };
        let _ = self.current_env.borrow_mut().define(name.to_string(), info);
        Ok(())
    }

    pub(crate) fn visit_compile_decl(
        &mut self,
        name: &str,
        decls: &[Decl],
    ) -> Result<(), String> {
        for d in decls {
            let mut namespaced_d = d.clone();
            match &mut namespaced_d {
                Decl::FnDecl {
                    name: fn_name,
                    visibility,
                    ..
                } => {
                    *fn_name = format!("{}::{}", name, fn_name);
                    *visibility = Visibility::Public;
                }
                Decl::MicroDecl {
                    name: m_name,
                    visibility,
                    ..
                } => {
                    *m_name = format!("{}::{}", name, m_name);
                    *visibility = Visibility::Public;
                }
                Decl::MacroDecl {
                    name: m_name,
                    visibility,
                    ..
                } => {
                    *m_name = format!("{}::{}", name, m_name);
                    *visibility = Visibility::Public;
                }
                _ => {}
            }
            self.visit_declaration(&namespaced_d)?;
            if let Decl::MacroDecl {
                name: ref m_name,
                params,
                body,
                visibility,
                ..
            } = namespaced_d
            {
                let prefix = format!("{}::", name);
                let short_name = m_name.replace(&prefix, "");
                let without_dollar = short_name.trim_start_matches('$').to_string();
                let aliases = vec![
                    format!("{}::{}", name, without_dollar),
                    short_name.clone(),
                    without_dollar.clone(),
                ];
                for alias in aliases {
                    if alias != *m_name {
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
                            is_compilable: false,
                        };
                        let _ = self.current_env.borrow_mut().define(alias, alias_info);
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn visit_extern_fn_decl(
        &mut self,
        name: &str,
        params: &[Param],
        return_type: &BaseType,
        alias: &Option<String>,
    ) -> Result<(), String> {
        let sym_name = alias.as_ref().unwrap_or(&name.to_string()).clone();
        let info = SymbolInfo {
            name: sym_name.clone(),
            kind: SymbolKind::Function {
                return_type: return_type.clone(),
                params: params.to_vec(),
                generics: vec![],
                body: None,
            },
            visibility: Visibility::Public,
            dependencies: vec![],
            is_used: false,
            is_param: false,
            is_uninitialized: false,
            is_compilable: false,
        };
        self.current_env
            .borrow_mut()
            .define(sym_name, info)?;
        Ok(())
    }

    pub(crate) fn visit_extern_block_decl(
        &mut self,
        decls: &[Decl],
    ) -> Result<(), String> {
        for d in decls {
            self.visit_declaration(d)?;
        }
        Ok(())
    }
}
