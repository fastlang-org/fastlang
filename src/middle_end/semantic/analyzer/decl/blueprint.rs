use super::super::*;

impl SemanticAnalyzer {
    pub(crate) fn visit_blueprint_decl(
        &mut self,
        name: &str,
        visibility: &Visibility,
        definition: &BlueprintDef,
        share_directive: &Option<crate::frontend::parser::ast::BlueprintShareDirective>,
    ) -> Result<(), String> {
        if let Some(ref sd) = share_directive {
            let is_self_shareable = self
                .current_env
                .borrow()
                .lookup_blueprint(name)
                .map(|bp| bp.is_shareable())
                .unwrap_or(false);
            if is_self_shareable {
                return Err(format!(
                    "Semantic Error: Blueprint '{}' cannot declare 'share' directive because it is already shareable (nested sharing is forbidden).",
                    name
                ));
            }
            if let crate::frontend::parser::ast::BlueprintDef::Explicit(fields) = definition {
                let field_opt = fields.iter().find(|f| f.name == sd.target_field);
                match field_opt {
                    None => {
                        return Err(format!(
                            "Semantic Error: Share target field '{}' not found in blueprint '{}'.",
                            sd.target_field, name
                        ));
                    }
                    Some(field) => {
                        let target_type_name = match &field.type_node {
                            BaseType::RawPointer(inner) => inner.as_str(),
                            other => other.as_str(),
                        };
                        if let Some(bp) = self
                            .current_env
                            .borrow()
                            .lookup_blueprint(&target_type_name)
                        {
                            if !bp.is_shareable() {
                                return Err(format!(
                                    "Semantic Error: Field '{}' of type '{}' in blueprint '{}' is not shareable (target type must define handle share).",
                                    sd.target_field,
                                    field.type_node.as_str(),
                                    name
                                ));
                            }
                        }
                    }
                }
            }
        }
        let info = self.make_blueprint_symbol(name, visibility.clone());
        self.current_env.borrow_mut().define(name.to_string(), info)?;
        Ok(())
    }

    pub(crate) fn visit_impl_decl(
        &mut self,
        target: &str,
        target_generics: &[BaseType],
        _is_handle_impl: bool,
        methods: &[Decl],
        handle_block: &[Decl],
        static_block: &[Decl],
    ) -> Result<(), String> {
        let is_builtin = Self::is_primitive_numeric(target)
            || matches!(
                target,
                "char"
                    | "uchar"
                    | "bool"
                    | "flag"
                    | "array"
                    | "byte"
                    | "usize"
                    | "isize"
                    | "type"
            );
        let symbol_opt = self.current_env.borrow().lookup(target);
        let meta_opt = self.global_metadata.get(target).cloned();

        if !is_builtin && symbol_opt.is_none() && meta_opt.is_none() {
            return Err(format!(
                "Semantic Error: Target '{}' not found for impl block",
                target
            ));
        }

        let expected_generics = if let Some(meta) = &meta_opt {
            match &meta.ty {
                BaseType::Enum { generics, .. }
                | BaseType::Blueprint { generics, .. }
                | BaseType::Class { generics, .. }
                | BaseType::Struct { generics, .. } => generics.len(),
                BaseType::Array { .. } => 1,
                _ => 0,
            }
        } else if let Some(bp) = self.current_env.borrow().lookup_blueprint(target) {
            bp.generics.len()
        } else if target == "array" {
            1
        } else {
            0
        };

        if expected_generics > 0 && target_generics.len() != expected_generics {
            return Err(format!(
                "Semantic Error: Type '{}' requires {} generic parameter(s) in 'impl {}<...>', found {}",
                target, expected_generics, target, target_generics.len()
            ));
        }

        let is_target_class = if let Some(meta) = &meta_opt {
            matches!(&meta.ty, BaseType::Class { .. })
        } else if let Some(bp) = self.current_env.borrow().lookup_blueprint(target) {
            bp.is_class
        } else {
            false
        };

        if !is_target_class {
            for m in methods.iter().chain(static_block.iter()).chain(handle_block.iter()) {
                if let Decl::FnDecl {
                    is_virtual: true, ..
                } = m
                {
                    return Err(format!(
                        "Semantic Error: 'virtual' modifier is only allowed in 'class' methods, not in '{}'",
                        target
                    ));
                }
            }
        }

        let spec_key = if target_generics.is_empty()
            || target_generics
                .iter()
                .all(|g| matches!(g, BaseType::GenericParam(_)))
        {
            target.to_string()
        } else {
            let g_str: Vec<String> = target_generics.iter().map(|g| g.as_str()).collect();
            format!("{}<{}>", target, g_str.join(", "))
        };

        if !self.global_metadata.contains_key(&spec_key) {
            self.global_metadata.insert(
                spec_key.clone(),
                TypeMetadata {
                    name: spec_key.clone(),
                    ty: BaseType::from_impl_target(target, target_generics),
                    fields: std::collections::HashMap::new(),
                    methods: std::collections::HashMap::new(),
                    constructor: None,
                    handles: vec![],
                    handle_signatures: std::collections::HashMap::new(),
                    vars: std::collections::HashMap::new(),
                    variants: None,
                },
            );
        } else if let Some(entry) = self.global_metadata.get_mut(&spec_key) {
            if !target_generics.is_empty() {
                entry.ty = BaseType::from_impl_target(target, target_generics);
            }
        }

        let is_fn = symbol_opt
            .as_ref()
            .map(|s| matches!(s.kind, SymbolKind::Function { .. }))
            .unwrap_or(false);
        let is_block = symbol_opt
            .as_ref()
            .map(
                |s|
                    matches!(s.kind, SymbolKind::Variable { ref type_node, .. } if matches!(type_node, BaseType::Block { .. } | BaseType::Machine { .. }))
            )
            .unwrap_or(false);

        let op_names = [
            "add",
            "sub",
            "mul",
            "div",
            "mod",
            "index_access",
            "arrow",
            "arrow_assign",
            "equal",
            "not_equal",
            "less_than",
            "greater_than",
            "less_than_equal",
            "greater_than_equal",
        ];

        for h in methods.iter().chain(static_block.iter()).chain(handle_block.iter()) {
            if let Decl::FnDecl {
                name: h_name,
                params,
                return_type,
                ..
            } = h
            {
                self.record_type_dependencies(return_type);
                for p in params {
                    self.record_type_dependencies(&p.type_node);
                }
                if is_fn {
                    if op_names.contains(&h_name.as_str()) || h_name == "call" {
                        return Err(
                            format!("Semantic Error: Operator overloading and 'call' handles are not allowed on function '{}'", target)
                        );
                    }
                } else if is_block
                    && op_names.contains(&h_name.as_str()) {
                        return Err(
                            format!("Semantic Error: Operator overloading handles are not allowed on block '{}'", target)
                        );
                    }

                if matches!(
                    h_name.as_str(),
                    "display"
                        | "iterator"
                        | "iter"
                        | "copy"
                        | "next"
                        | "break"
                        | "continue"
                ) {
                    if !params.is_empty() {
                        return Err(
                            format!(
                                "Semantic Error: Handle method '{}' cannot take any parameters on target '{}'",
                                h_name,
                                target
                            )
                        );
                    }
                    if h_name == "display" {
                        let resolved_ret = self.resolve_type(return_type);
                        let is_valid = match &resolved_ret {
                            BaseType::Str | BaseType::UStr | BaseType::Char | BaseType::UChar => true,
                            BaseType::Array { base_type, .. } => {
                                matches!(base_type.as_ref(), BaseType::Char | BaseType::UChar)
                            }
                            _ => false,
                        };
                        if !is_valid {
                            return Err(
                                format!(
                                    "Semantic Error: Handle method 'display' must return 'str', 'char', or 'char[]', found '{}' on target '{}'",
                                    return_type.as_str(),
                                    target
                                )
                            );
                        }
                    }
                }
            }
        }

        self.enter_scope();
        let prev_in_generic = self.scope.in_generic_template;
        if !target_generics.is_empty() {
            self.scope.in_generic_template = true;
        }
        self.active_flags.insert(Flag::HasReturn);
        let prev = self.scope.in_struct;
        self.scope.in_struct = true;

        let this_type = if target == "array" {
            BaseType::Array {
                base_type: Box::new(
                    target_generics
                        .first()
                        .cloned()
                        .unwrap_or(BaseType::Unknown),
                ),
                size: Box::new(None),
            }
        } else if target == "char" {
            BaseType::Char
        } else if target == "bool" || target == "flag" {
            BaseType::Bool
        } else if target.starts_with("int") {
            BaseType::Int(Size::S32)
        } else if target.starts_with("uint") {
            BaseType::UInt(Size::S32)
        } else if target.starts_with("float") {
            BaseType::Float(Size::S64)
        } else {
            BaseType::from_str(target)
        };

        let prev_type = self.scope.current_type_name.clone();
        self.scope.current_type_name = Some(this_type.as_str());

        for g in target_generics {
            let g_name = g.as_str();
            let g_info = SymbolInfo {
                name: g_name.clone(),
                kind: SymbolKind::Variable {
                    type_node: BaseType::Type(Box::new(g.clone())),
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
            let _ = self.current_env.borrow_mut().define(g_name, g_info);
        }

        let this_info = SymbolInfo {
            name: "this".to_string(),
            kind: SymbolKind::Variable {
                type_node: this_type,
                editability: Editability::Editable,
                is_array: target == "array",
            },
            visibility: Visibility::Private,
            dependencies: vec![],
            is_used: false,
            is_param: true,
            is_uninitialized: false,
            is_compilable: false,
        };
        let _ = self
            .current_env
            .borrow_mut()
            .define("this".to_string(), this_info);

        for m in methods.iter().chain(static_block.iter()) {
            self.visit_declaration(m)?;
            if let Decl::FnDecl {
                name,
                generics,
                params,
                return_type,
                ..
            } = m
            {
                if let Some(meta) = self.global_metadata.get_mut(target) {
                    meta.methods.insert(
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
                self.current_env.borrow_mut().mutate_blueprint(target, |bp| {
                    let gen_params: Vec<String> = target_generics
                        .iter()
                        .filter_map(|g| match g {
                            BaseType::GenericParam(n) => Some(n.clone()),
                            _ => None,
                        })
                        .collect();
                    if !gen_params.is_empty() {
                        bp.generics = gen_params;
                    }
                    bp.methods.insert(
                        name.clone(),
                        FnSignature {
                            name: name.clone(),
                            generics: generics.clone(),
                            params: params.clone(),
                            return_type: return_type.clone(),
                            is_virtual: false,
                            is_abstract: false,
                        },
                    );
                });
            }
        }

        for h in handle_block {
            self.visit_declaration(h)?;
            if let Decl::FnDecl {
                name,
                generics,
                params,
                return_type,
                ..
            } = h
            {
                let hk = HandleMethods::from_str(name.as_str());
                if let Some(meta) = self.global_metadata.get_mut(target) {
                    if hk != HandleMethods::NotFound && !meta.handles.contains(&hk) {
                        meta.handles.push(hk);
                    }
                    meta.handle_signatures.insert(
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
                self.current_env.borrow_mut().mutate_blueprint(target, |bp| {
                    let gen_params: Vec<String> = target_generics
                        .iter()
                        .filter_map(|g| match g {
                            BaseType::GenericParam(n) => Some(n.clone()),
                            _ => None,
                        })
                        .collect();
                    if !gen_params.is_empty() {
                        bp.generics = gen_params;
                    }
                    if hk != HandleMethods::NotFound && !bp.handles.contains(&hk) {
                        bp.handles.insert(hk);
                    }
                    bp.handle_signatures.insert(
                        name.clone(),
                        FnSignature {
                            name: name.clone(),
                            generics: generics.clone(),
                            params: params.clone(),
                            return_type: return_type.clone(),
                            is_virtual: false,
                            is_abstract: false,
                        },
                    );
                });
            }
        }

        self.scope.current_type_name = prev_type;
        self.scope.in_struct = prev;
        self.active_flags.remove(&Flag::HasReturn);
        self.leave_scope();
        self.scope.in_generic_template = prev_in_generic;
        Ok(())
    }
}
