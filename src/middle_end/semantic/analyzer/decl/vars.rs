use super::super::*;
use std::rc::Rc;

impl SemanticAnalyzer {
    pub(crate) fn visit_destructure_decl(
        &mut self,
        visibility: &Visibility,
        editability: &Editability,
        type_node: &BaseType,
        assignments: &[(String, Expr)],
        assign_op: &str,
    ) -> Result<(), String> {
        for (name, val) in assignments {
            self.analyze_var_decl(
                visibility,
                editability,
                type_node,
                name,
                val,
                assign_op,
            )?;
        }
        Ok(())
    }

    pub(crate) fn visit_object_destructure_decl(
        &mut self,
        visibility: &Visibility,
        editability: &Editability,
        type_name: &Option<String>,
        fields: &[(BaseType, String)],
        rhs: &Expr,
    ) -> Result<(), String> {
        let rhs_type = self.visit_expression(rhs)?;
        let clean_rhs = if let Some(t) = type_name {
            t.clone()
        } else {
            BaseType::from_str(&rhs_type)
                .blueprint_name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| rhs_type.clone())
        };
        let bp_data = self.current_env.borrow().lookup_blueprint(&clean_rhs);
        let bp_meta = self.global_metadata.get(&clean_rhs).cloned();
        for (idx, (type_node, name)) in fields.iter().enumerate() {
            let actual_type = if matches!(type_node, BaseType::Unknown) {
                if let Some(ref bp) = bp_data {
                    if let Some(ty) = bp.fields.get(name) {
                        ty.clone()
                    } else {
                        BaseType::Unknown
                    }
                } else if let Some(ref meta) = bp_meta {
                    if let Some(ty) = meta.fields.get(name) {
                        ty.clone()
                    } else {
                        let params = meta
                            .constructor
                            .as_ref()
                            .and_then(|ctors| ctors.first())
                            .map(|c| &c.params[..]);
                        if let Some(p) = params {
                            if idx < p.len() {
                                p[idx].type_node.clone()
                            } else {
                                BaseType::Unknown
                            }
                        } else {
                            BaseType::Unknown
                        }
                    }
                } else {
                    BaseType::Unknown
                }
            } else {
                type_node.clone()
            };
            let info = SymbolInfo {
                name: name.clone(),
                kind: SymbolKind::Variable {
                    type_node: actual_type,
                    editability: editability.clone(),
                    is_array: false,
                },
                visibility: visibility.clone(),
                dependencies: vec![],
                is_used: false,
                is_param: false,
                is_uninitialized: false,
                is_compilable: false,
            };
            self.current_env.borrow_mut().define(name.clone(), info)?;
        }
        Ok(())
    }

    pub(crate) fn visit_array_decl(
        &mut self,
        visibility: &Visibility,
        editability: &Editability,
        type_node: &BaseType,
        name: &str,
        length: &Expr,
        value: &Expr,
    ) -> Result<(), String> {
        let expr_type = self.visit_expression(value)?;
        self.visit_expression(length)?;

        let declared_type = type_node.as_str();
        let array_inner = if expr_type.starts_with("array<") {
            expr_type
                .trim_start_matches("array<")
                .trim_end_matches('>')
                .to_string()
        } else {
            expr_type.clone()
        };
        let array_inner_base = array_inner.split('[').next().unwrap_or(&array_inner);

        if expr_type != "unknown"
            && expr_type != "default"
            && !self.types_are_compatible(&declared_type, &expr_type)
            && !self.types_are_compatible(&declared_type, &array_inner)
            && !self.types_are_compatible(&declared_type, array_inner_base)
        {
            return Err(format!(
                "Semantic Error: Type mismatch for array '{}'. Declared '{}', got '{}'",
                name, declared_type, expr_type
            ));
        }

        let info = SymbolInfo {
            name: name.to_string(),
            kind: SymbolKind::Variable {
                type_node: type_node.clone(),
                editability: editability.clone(),
                is_array: true,
            },
            visibility: visibility.clone(),
            dependencies: vec![],
            is_used: false,
            is_param: false,
            is_uninitialized: false,
            is_compilable: false,
        };
        self.current_env.borrow_mut().define(name.to_string(), info)?;
        if matches!(value, Expr::Instantiate { .. }) || type_node.as_str().ends_with('*') {
            self.memory.heap_allocated_vars.insert(name.to_string());
        }
        Ok(())
    }

    pub(crate) fn visit_block_decl(
        &mut self,
        visibility: &Visibility,
        name: &str,
        return_type: &Option<BaseType>,
        statements: &[Stmt],
    ) -> Result<(), String> {
        let mut bp = BlueprintData::new(name);
        let prev_flags = self.active_flags.clone();
        let prev_return = self.active_return_type.clone();
        self.active_flags.insert(Flag::HasYield);
        self.active_flags.insert(Flag::HasLeave);
        self.active_flags.insert(Flag::HasReturn);
        self.active_flags.insert(Flag::HasBreak);
        self.active_flags.insert(Flag::HasContinue);
        self.active_flags.insert(Flag::HasThrow);
        self.active_return_type = return_type.clone();

        self.enter_scope();
        for s in statements {
            if let Stmt::Declaration(Decl::VarDecl {
                name: f_name,
                type_node,
                ..
            }) = s
            {
                bp.fields.insert(f_name.clone(), type_node.clone());
            }
            if let Stmt::Declaration(Decl::FnDecl {
                name: fn_name,
                params,
                return_type,
                generics,
                ..
            }) = s
            {
                bp.methods.insert(
                    fn_name.clone(),
                    FnSignature {
                        name: fn_name.clone(),
                        generics: generics.clone(),
                        params: params.clone(),
                        return_type: return_type.clone(),
                        is_virtual: false,
                        is_abstract: false,
                    },
                );
            }
            self.visit_statement(s)?;
        }
        self.leave_scope();
        self.active_flags = prev_flags;
        self.active_return_type = prev_return;

        let mut block_fields = std::collections::HashMap::new();
        for (fname, ftype) in bp.fields.iter() {
            block_fields.insert(fname.clone(), ftype.clone());
        }
        let mut block_methods = std::collections::HashMap::new();
        for (mname, msig) in bp.methods.iter() {
            block_methods.insert(
                mname.clone(),
                FnType {
                    name: mname.clone(),
                    generics: vec![],
                    params: msig.params.clone(),
                    return_type: msig.return_type.clone(),
                    mode: ExecutionMode::Runtime,
                },
            );
        }

        self.current_env
            .borrow_mut()
            .define_blueprint(name.to_string(), bp);

        let info = SymbolInfo {
            name: name.to_string(),
            kind: SymbolKind::Variable {
                type_node: BaseType::Block {
                    name: name.to_string(),
                    fields: Box::new(block_fields),
                    methods: Box::new(block_methods),
                },
                editability: Editability::Editable,
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
            .define(name.to_string(), info)?;
        Ok(())
    }

    pub(crate) fn visit_define_decl(
        &mut self,
        name: &str,
        type_alias: &Option<BaseType>,
        value: &Option<Expr>,
        visibility: &Visibility,
    ) -> Result<(), String> {
        let (type_node, is_compilable) = if let Some(t) = type_alias {
            (BaseType::Type(Box::new(t.clone())), true)
        } else if let Some(val) = value {
            let t_str = self.visit_expression(val)?;
            (BaseType::from_str(&t_str), true)
        } else {
            (BaseType::Int(Size::S32), false)
        };
        let info = SymbolInfo {
            name: name.to_string(),
            kind: SymbolKind::Variable {
                type_node,
                editability: Editability::NotEditable,
                is_array: false,
            },
            visibility: visibility.clone(),
            dependencies: vec![],
            is_used: false,
            is_param: false,
            is_uninitialized: false,
            is_compilable,
        };
        self.current_env.borrow_mut().define_or_update(name.to_string(), info);
        Ok(())
    }

    pub(crate) fn visit_namespace_decl(
        &mut self,
        name: &str,
        decls: &[Decl],
    ) -> Result<(), String> {
        let parent_env = Rc::clone(&self.current_env);
        let ns_env = self.current_env.borrow_mut().get_or_create_namespace(name, parent_env);
        let prev_env = Rc::clone(&self.current_env);
        self.current_env = Rc::clone(&ns_env);
        for d in decls {
            self.visit_declaration(d)?;
        }
        self.current_env = prev_env;
        Ok(())
    }

    // ----------------------------------------------------------
    // analyze_var_decl - handle-aware VarDecl analysis
    // ----------------------------------------------------------
    pub(crate) fn analyze_var_decl(
        &mut self,
        visibility: &Visibility,
        editability: &Editability,
        type_node: &BaseType,
        name: &str,
        value: &Expr,
        assign_op: &str,
    ) -> Result<(), String> {
        let prev_context = self.scope.current_context.clone();
        self.scope.current_context = Some(name.to_string());

        if matches!(type_node, BaseType::Flag) {
            return Err(
                format!("Semantic Error: 'flag' is a reserved state inspection type and cannot be declared as a user variable '{}'. Use 'bool' instead.", name)
            );
        }

        if matches!(type_node, BaseType::Block { .. }) {
            return Err(
                "Semantic Error: Blocks cannot be stored in variables or assigned to values. Blocks are strictly execution scopes; use 'method' or 'fn' for first-class callable entities.".to_string(),
            );
        }

        if !self.is_known_type(type_node) {
            let type_str = type_node
                .type_name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| type_node.as_str());
            return Err(format!(
                "Semantic Error: Type '{}' is not defined in this scope.",
                type_str
            ));
        }

        if assign_op == ":="
            && self.current_env.borrow().symbols.contains_key(name) {
                return Err(
                    format!(
                        "Semantic Error: Variable '{}' is already defined in this scope. Cannot redeclare using ':='.",
                        name
                    )
                );
            }

        let mut expr_type: String = self.visit_expression(value)?;
        if expr_type == "int" {
            expr_type = "int32".to_string();
        }
        if assign_op == ":=" && expr_type == "undefined" {
            return Err(
                format!(
                    "Semantic Error: Cannot infer type for variable '{}' from 'undefined'. Specify an explicit type (e.g. 'Type {} = undefined;').",
                    name, name
                )
            );
        }
        let declared_type = type_node.as_str();
        self.record_type_dependencies(type_node);

        // Propagate deps recorded under the var name into the containing function context.
        // This ensures tree-shaking can trace: fn_context -> deps_used_in_var_init.
        if let Some(ref prev_ctx) = prev_context.clone() {
            let var_deps: Vec<String> = self
                .dependency_graph
                .get(name)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect();
            for dep in var_deps {
                self.dependency_graph
                    .entry(prev_ctx.clone())
                    .or_default()
                    .insert(dep);
            }
        }

        if expr_type != "unknown" {
            let base_decl: &str = declared_type.split('<').next().unwrap_or(&declared_type);

            match base_decl {
                // complex types with possible handle overloading
                "custom" | "class" | "struct" | "enum" => {
                    let bp_name = type_node
                        .blueprint_name()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| declared_type.clone());

                    // Check if bp_name is a type alias defined via define
                    let is_alias_compatible =
                        if let Some(info) = self.current_env.borrow().lookup(&bp_name) {
                            if let SymbolKind::Variable {
                                type_node: BaseType::Type(inner),
                                ..
                            } = &info.kind {
                                self.types_are_compatible(&inner.as_str(), &expr_type)
                            } else {
                                false
                            }
                        } else {
                            false
                        };

                    // Direct compatibility: X = X
                    let directly_compatible = is_alias_compatible
                        || self.types_are_compatible(&declared_type, &expr_type)
                        || expr_type == bp_name;

                    // If types are directly compatible, always accept regardless of op
                    // e.g. MathScope a -> new MathScope() is fine even with ->
                    if directly_compatible {
                        // accepted
                    } else if assign_op == "=" {
                        return Err(
                            format!(
                                "Semantic Error: Type mismatch for '{}'. Declared '{}', got '{}'",
                                name,
                                declared_type,
                                expr_type
                            )
                        );
                    } else {
                        // operator overloading via handle
                        let effective_op = if assign_op == "->" {
                            "arrow_assign"
                        } else {
                            assign_op
                        };
                        match resolve_handle_for_op(&self.current_env, &bp_name, effective_op) {
                            HandleLookupResult::Found(bp) => {
                                let handle = op_to_handle(effective_op);
                                if !bp.handle_accepts_type(handle, &expr_type) {
                                    return Err(
                                        format!(
                                            "Semantic Error: Handle '{}' in '{}' does not accept type '{}'. Expected a compatible type for the '{}' operator.",
                                            handle.as_str(),
                                            bp_name,
                                            expr_type,
                                            assign_op
                                        )
                                    );
                                }
                                // OK - handle overloading accepts this
                            }
                            HandleLookupResult::BlueprintNotFound => {
                                // Not in env yet (generic or not-yet-defined) - allow
                            }
                            HandleLookupResult::HandleMissing { handle } => {
                                return Err(
                                    format!(
                                        "Semantic Error: Type '{}' does not support the '{}' operator. Handle '{}' is not defined in its handle block.",
                                        bp_name,
                                        assign_op,
                                        handle.as_str()
                                    )
                                );
                            }
                            HandleLookupResult::UnknownOp => {
                                // Unknown operator - allow
                            }
                        }
                    }
                }

                // bare type name (e.g. "MathScope" instead of "custom<MathScope>")
                // could be a user-defined type - look up in blueprints
                name_key
                    if self
                        .current_env
                        .borrow()
                        .lookup_blueprint(name_key)
                        .is_some() =>
                {
                    // It is a known blueprint type used without "custom<>" prefix
                    if assign_op != "=" {
                        let effective_op = if assign_op == "->" {
                            "arrow_assign"
                        } else {
                            assign_op
                        };
                        match resolve_handle_for_op(&self.current_env, name_key, effective_op) {
                            HandleLookupResult::Found(bp) => {
                                let handle = op_to_handle(effective_op);
                                if !bp.handle_accepts_type(handle, &expr_type) {
                                    return Err(
                                        format!(
                                            "Semantic Error: Handle '{}' in '{}' does not accept type '{}'.",
                                            handle.as_str(),
                                            name_key,
                                            expr_type
                                        )
                                    );
                                }
                            }
                            HandleLookupResult::BlueprintNotFound => {}
                            HandleLookupResult::HandleMissing { handle } => {
                                return Err(
                                    format!(
                                        "Semantic Error: Type '{}' does not support '{}' (missing handle '{}').",
                                        name_key,
                                        assign_op,
                                        handle.as_str()
                                    )
                                );
                            }
                            HandleLookupResult::UnknownOp => {}
                        }
                    }
                    // For "=", just accept - they're using the type by its raw name
                }

                // simple primitive types
                _ => {
                    if let Expr::Lambda { params, .. } = value {
                        let is_lambda_untyped = matches!(type_node, BaseType::Unknown)
                            || matches!(type_node, BaseType::Lambda { params, .. } if params.is_empty());
                        if is_lambda_untyped
                            && params.iter().any(|p| p.type_node == BaseType::Unknown)
                        {
                            return Err("Semantic Error: Lambda parameters must have explicit type annotations when target type is not declared (e.g. '|x: int, y: int|')".to_string());
                        }
                    }

                    // Allow -> operator for any type (it's used as "default init" syntax)
                    // Allow := operator (inferred type)
                    if assign_op != "->"
                        && assign_op != ":="
                        && declared_type != "unknown"
                        && expr_type != "default"
                        && !self.types_are_compatible(&declared_type, &expr_type)
                    {
                        return Err(
                            format!(
                                "Semantic Error: Type mismatch for '{}'. Declared '{}', got '{}'",
                                name,
                                declared_type,
                                expr_type
                            )
                        );
                    }
                }
            }
        }

        let deps = self
            .dependency_graph
            .get(name)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect();

        let final_type_node = if matches!(type_node, BaseType::Unknown | BaseType::Auto | BaseType::Number) {
            if matches!(type_node, BaseType::Number) {
                if expr_type.starts_with("float") || expr_type == "float" {
                    BaseType::Float(Size::S64)
                } else if expr_type.starts_with("uint") {
                    BaseType::from_str(&expr_type)
                } else {
                    BaseType::Int(Size::S32)
                }
            } else if expr_type == "undefined" || expr_type == "unknown" {
                if matches!(type_node, BaseType::Auto) {
                    BaseType::Auto
                } else {
                    BaseType::Unknown
                }
            } else if expr_type == "int" {
                BaseType::Int(Size::S32)
            } else if expr_type == "float" {
                BaseType::Float(Size::S64)
            } else if expr_type == "bool" {
                BaseType::Bool
            } else if expr_type == "char" {
                BaseType::Char
            } else {
                BaseType::from_str(&expr_type)
            }
        } else {
            self.resolve_type(type_node)
        };

        self.record_type_dependencies(&final_type_node);

        let is_custom_or_struct = match &final_type_node {
            BaseType::Class { .. }
            | BaseType::Struct { .. }
            | BaseType::Blueprint { .. }
            | BaseType::Enum { .. }
            | BaseType::Array { .. } => true,
            _ => self
                .current_env
                .borrow()
                .lookup_blueprint(&final_type_node.as_str())
                .is_some(),
        };

        let is_uninit = match value {
            Expr::LiteralUndefined => true,
            Expr::Default(None) => !is_custom_or_struct,
            _ => false,
        };
        let info = SymbolInfo {
            name: name.to_string(),
            kind: SymbolKind::Variable {
                type_node: final_type_node,
                editability: editability.clone(),
                is_array: false,
            },
            visibility: visibility.clone(),
            dependencies: deps,
            is_used: false,
            is_param: false,
            is_compilable: false,
            is_uninitialized: is_uninit,
        };
        self.current_env
            .borrow_mut()
            .define(name.to_string(), info)?;
        if matches!(value, Expr::Instantiate { .. }) || self.is_class_type(&declared_type) {
            self.memory.heap_allocated_vars.insert(name.to_string());
        }
        self.scope.current_context = prev_context;
        Ok(())
    }
}
