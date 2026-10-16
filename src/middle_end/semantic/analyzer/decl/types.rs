use super::super::*;
use std::collections::HashMap;

impl SemanticAnalyzer {
    pub(crate) fn visit_class_decl(
        &mut self,
        visibility: &Visibility,
        name: &str,
        extends: Option<&str>,
        handles: &[HandleMethods],
        public_block: &[Decl],
        private_block: &[Decl],
        static_block: &[Decl],
        handle_block: &[Decl],
        constructor: &Option<Vec<ConstructorDecl>>,
    ) -> Result<(), String> {
        self.analyze_class_or_struct_decl(
            visibility.clone(),
            name,
            true,
            extends,
            handles,
            public_block,
            private_block,
            static_block,
            handle_block,
            constructor,
        )
    }

    pub(crate) fn visit_struct_decl(
        &mut self,
        visibility: &Visibility,
        name: &str,
        handles: &[HandleMethods],
        public_block: &[Decl],
        private_block: &[Decl],
        static_block: &[Decl],
        handle_block: &[Decl],
        constructor: &Option<Vec<ConstructorDecl>>,
    ) -> Result<(), String> {
        self.analyze_class_or_struct_decl(
            visibility.clone(),
            name,
            false,
            None,
            handles,
            public_block,
            private_block,
            static_block,
            handle_block,
            constructor,
        )
    }

    pub(crate) fn visit_enum_decl(
        &mut self,
        visibility: &Visibility,
        name: &str,
        handle_block: &[Decl],
        variants: &[EnumVariant],
    ) -> Result<(), String> {
        let info = self.make_blueprint_symbol(name, visibility.clone());
        self.current_env.borrow_mut().define(name.to_string(), info)?;
        for v in variants {
            let var_qualified = format!("{}::{}", name, v.name);
            let bp_var_info = self.make_blueprint_symbol(&var_qualified, visibility.clone());
            let _ = self
                .current_env
                .borrow_mut()
                .define(var_qualified.clone(), bp_var_info);

            self.dependency_graph
                .entry(v.name.clone())
                .or_default()
                .insert(name.to_string());
            self.dependency_graph
                .entry(name.to_string())
                .or_default()
                .insert(v.name.clone());
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
            let _ = self
                .current_env
                .borrow_mut()
                .define(v.name.clone(), var_symbol);
        }
        self.enter_scope();
        let prev_type = self.scope.current_type_name.clone();
        self.scope.current_type_name = Some(name.to_string());
        for d in handle_block {
            if let Decl::FnDecl {
                is_virtual: true, ..
            } = d
            {
                return Err(format!(
                    "Semantic Error: 'virtual' modifier is only allowed in 'class' methods, not in enum '{}'",
                    name
                ));
            }
            self.visit_declaration(d)?;
        }
        self.scope.current_type_name = prev_type;
        self.leave_scope();
        Ok(())
    }

    pub(crate) fn visit_machine_decl(
        &mut self,
        visibility: &Visibility,
        name: &str,
        labels: &[Decl],
        handle_block: &[Decl],
    ) -> Result<(), String> {
        let mut bp = BlueprintData::new(name);

        let mut label_map = std::collections::HashMap::new();
        for l in labels.iter() {
            if let Decl::LabelDecl {
                name: lbl_name,
                body: stmts,
                ..
            } = l
            {
                let mut lbl_fields = std::collections::HashMap::new();
                for s in stmts {
                    if let Stmt::Declaration(Decl::VarDecl {
                        name: var_name,
                        type_node,
                        ..
                    }) = s
                    {
                        lbl_fields.insert(var_name.clone(), type_node.clone());
                    }
                }
                label_map.insert(
                    lbl_name.clone(),
                    BaseType::Label {
                        name: lbl_name.clone(),
                        fields: Box::new(lbl_fields),
                    },
                );
            }
        }

        let info = SymbolInfo {
            name: name.to_string(),
            kind: SymbolKind::Variable {
                type_node: BaseType::Machine {
                    name: name.to_string(),
                    fields: Box::new(std::collections::HashMap::new()),
                    methods: Box::new(std::collections::HashMap::new()),
                    labels: Box::new(label_map),
                    mode: ExecutionMode::Runtime,
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

        if let Some(Decl::LabelDecl {
            name: last_lbl_name,
            body: last_body,
            ..
        }) = labels.last()
        {
            fn stmts_contain_continue(stmts: &[Stmt]) -> bool {
                for s in stmts {
                    match s {
                        Stmt::ContinueStmt => return true,
                        Stmt::Block(inner) | Stmt::ThisBlock(inner) => {
                            if stmts_contain_continue(inner) {
                                return true;
                            }
                        }
                        Stmt::IfStmt {
                            then_block,
                            else_block,
                            ..
                        } => {
                            if stmts_contain_continue(then_block) {
                                return true;
                            }
                            if let Some(eb) = else_block {
                                if stmts_contain_continue(eb) {
                                    return true;
                                }
                            }
                        }
                        _ => {}
                    }
                }
                false
            }

            if stmts_contain_continue(last_body) {
                return Err(format!(
                    "Semantic Error: Cannot 'continue' in final label '{}' of machine '{}' as there is no subsequent label to advance to.",
                    last_lbl_name, name
                ));
            }
        }

        let mut label_vars: HashMap<String, HashMap<String, BaseType>> = HashMap::new();
        for label_decl in labels {
            if let Decl::LabelDecl { name: l_name, body, .. } = label_decl {
                let clean_name = l_name.replace("@", "");
                let mut vars = HashMap::new();
                for s in body {
                    if let Stmt::Declaration(Decl::VarDecl {
                        name: v_name,
                        type_node,
                        ..
                    }) = s
                    {
                        vars.insert(v_name.clone(), type_node.clone());
                    }
                }
                label_vars.insert(clean_name.clone(), vars.clone());
                label_vars.insert(l_name.clone(), vars);
            }
        }
        self.machine_labels.insert(name.to_string(), label_vars);
        let prev_machine = self.scope.current_machine.clone();
        self.scope.current_machine = Some(name.to_string());
        self.enter_scope();

        for label_decl in labels {
            if let Decl::LabelDecl { body, .. } = label_decl {
                for s in body {
                    let _ = self.visit_statement(s);
                }
            }
        }

        for d in handle_block {
            if let Decl::FnDecl {
                name: f_name,
                params,
                return_type,
                is_virtual,
                is_abstract,
                generics,
                ..
            } = d
            {
                if *is_virtual {
                    return Err(format!(
                        "Semantic Error: 'virtual' modifier is only allowed in 'class' methods, not in machine '{}'",
                        name
                    ));
                }
                bp.methods.insert(
                    f_name.clone(),
                    crate::middle_end::semantic::environment::FnSignature {
                        name: f_name.clone(),
                        generics: generics.clone(),
                        params: params.clone(),
                        return_type: return_type.clone(),
                        is_virtual: *is_virtual,
                        is_abstract: *is_abstract,
                    },
                );
            }
            let _ = self.visit_declaration(d);
        }

        self.leave_scope();
        self.scope.current_machine = prev_machine;
        self.current_env
            .borrow_mut()
            .define_blueprint(name.to_string(), bp);
        Ok(())
    }

    pub(crate) fn visit_label_decl(
        &mut self,
        name: &str,
        body: &[Stmt],
    ) -> Result<(), String> {
        let info = SymbolInfo {
            name: name.to_string(),
            kind: SymbolKind::Label,
            visibility: Visibility::Private,
            dependencies: vec![],
            is_used: false,
            is_param: false,
            is_uninitialized: false,
            is_compilable: false,
        };
        self.current_env.borrow_mut().define(name.to_string(), info)?;
        let prev_flags = self.active_flags.clone();
        self.active_flags.insert(Flag::HasYield);
        self.active_flags.insert(Flag::HasLeave);
        self.active_flags.insert(Flag::HasReturn);
        self.enter_scope();
        for s in body {
            self.visit_statement(s)?;
        }
        self.leave_scope();
        self.active_flags = prev_flags;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn analyze_class_or_struct_decl(
        &mut self,
        visibility: Visibility,
        name: &str,
        is_class: bool,
        extends: Option<&str>,
        handles: &[HandleMethods],
        public_block: &[Decl],
        private_block: &[Decl],
        static_block: &[Decl],
        handle_block: &[Decl],
        constructor: &Option<Vec<ConstructorDecl>>,
    ) -> Result<(), String> {
        let prev_context = self.scope.current_context.clone();
        self.scope.current_context = Some(name.to_string());
        if let Some(parent) = extends {
            self.class_hierarchy
                .insert(name.to_string(), parent.to_string());
            self.record_dependency(parent.to_string());
        }
        if !is_class {
            for d in private_block
                .iter()
                .chain(public_block)
                .chain(static_block)
                .chain(handle_block)
            {
                if let Decl::FnDecl {
                    is_virtual: true, ..
                } = d
                {
                    return Err(format!(
                        "Semantic Error: 'virtual' modifier is only allowed in 'class' methods, not in struct '{}'",
                        name
                    ));
                }
            }
        }
        for d in private_block.iter().chain(public_block).chain(static_block) {
            if let Decl::VarDecl { type_node, .. } = d {
                self.record_type_dependencies(type_node);
            } else if let Decl::FnDecl {
                params,
                return_type,
                ..
            } = d
            {
                for p in params {
                    self.record_type_dependencies(&p.type_node);
                }
                self.record_type_dependencies(return_type);
            }
        }
        for d in handle_block {
            if let Decl::FnDecl {
                params,
                return_type,
                ..
            } = d
            {
                for p in params {
                    self.record_type_dependencies(&p.type_node);
                }
                self.record_type_dependencies(return_type);
            }
        }
        if let Some(constructors) = constructor {
            for ctor in constructors {
                for p in &ctor.params {
                    self.record_type_dependencies(&p.type_node);
                }
            }
        }
        let mut bp = BlueprintData::new(name);
        bp.is_class = is_class;
        if let Some(parent) = extends {
            let parent_bp = self
                .current_env
                .borrow()
                .lookup_blueprint(parent)
                .or_else(|| {
                    self.global_metadata
                        .get(parent)
                        .map(build_blueprint_from_metadata)
                });
            if let Some(parent_bp) = parent_bp {
                for (f_k, f_v) in &parent_bp.fields {
                    bp.fields.entry(f_k.clone()).or_insert_with(|| f_v.clone());
                }
                for (m_k, m_v) in &parent_bp.methods {
                    bp.methods.entry(m_k.clone()).or_insert_with(|| m_v.clone());
                }
                for h in &parent_bp.handles {
                    bp.handles.insert(*h);
                }
                for (h_k, h_v) in &parent_bp.handle_signatures {
                    bp.handle_signatures
                        .entry(h_k.clone())
                        .or_insert_with(|| h_v.clone());
                }
            }
        }
        for h in handles {
            bp.handles.insert(*h);
        }
        for d in handle_block {
            if let Decl::FnDecl {
                name: fn_name,
                params: fn_params,
                return_type,
                is_virtual,
                is_abstract,
                generics: fn_generics,
                ..
            } = d
            {
                if fn_name == "display" {
                    if !fn_params.is_empty() {
                        return Err(format!("Semantic Error: Handle method 'display' cannot take any parameters on target '{}'", name));
                    }
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
                                name
                            )
                        );
                    }
                }

                let hk = HandleMethods::from_str(fn_name.as_str());
                let sig = FnSignature {
                    name: fn_name.clone(),
                    generics: fn_generics.clone(),
                    params: fn_params.clone(),
                    return_type: return_type.clone(),
                    is_virtual: *is_virtual,
                    is_abstract: *is_abstract,
                };
                if hk != HandleMethods::NotFound {
                    bp.define_handle(hk, sig);
                } else {
                    bp.handle_signatures.insert(fn_name.clone(), sig);
                }
            }
        }
        for d in private_block.iter().chain(public_block).chain(static_block) {
            if let Decl::VarDecl {
                name: f_name,
                type_node,
                ..
            } = d
            {
                bp.fields.insert(f_name.clone(), type_node.clone());
            }
        }
        for d in private_block {
            if let Decl::VarDecl { name: f_name, .. } = d {
                bp.private_fields.insert(f_name.clone());
            } else if let Decl::FnDecl { name: fn_name, .. } = d {
                bp.private_methods.insert(fn_name.clone());
            }
        }
        for d in private_block.iter().chain(public_block).chain(static_block) {
            if let Decl::FnDecl {
                name: fn_name,
                params,
                return_type,
                is_virtual,
                is_abstract,
                generics,
                ..
            } = d
            {
                bp.methods.insert(
                    fn_name.clone(),
                    FnSignature {
                        name: fn_name.clone(),
                        generics: generics.clone(),
                        params: params.clone(),
                        return_type: return_type.clone(),
                        is_virtual: *is_virtual,
                        is_abstract: *is_abstract,
                    },
                );
            }
        }
        self.current_env
            .borrow_mut()
            .define_blueprint(name.to_string(), bp);

        let info = self.make_blueprint_symbol(name, visibility);
        self.current_env
            .borrow_mut()
            .define(name.to_string(), info)?;

        self.enter_scope();
        let prev_in_class = self.scope.in_class;
        let prev_in_struct = self.scope.in_struct;
        if is_class {
            self.scope.in_class = true;
        } else {
            self.scope.in_struct = true;
        }
        let prev_type_name = self.scope.current_type_name.clone();
        self.scope.current_type_name = Some(name.to_string());

        for d in private_block
            .iter()
            .chain(public_block)
            .chain(static_block)
            .chain(handle_block)
        {
            self.visit_declaration(d)?;
        }

        if let Some(constructors) = constructor {
            let prev_ctor_ctx = self.scope.current_context.clone();
            self.scope.current_context = Some(format!("{}::init", name));
            for ctor in constructors {
                self.enter_scope();
                for param in &ctor.params {
                    let pi = SymbolInfo {
                        name: param.name.clone(),
                        kind: SymbolKind::Variable {
                            type_node: self.resolve_type(&param.type_node),
                            editability: Editability::Editable,
                            is_array: false,
                        },
                        visibility: Visibility::Private,
                        dependencies: vec![],
                        is_used: false,
                        is_param: true,
                        is_uninitialized: false,
                        is_compilable: false,
                    };
                    self.current_env
                        .borrow_mut()
                        .define(param.name.clone(), pi)?;
                }
                let prev_stmt = self.scope.in_statement_scope;
                self.scope.in_statement_scope = true;
                for s in &ctor.body {
                    self.visit_statement(s)?;
                }
                self.scope.in_statement_scope = prev_stmt;
                self.leave_scope();
            }
            self.scope.current_context = prev_ctor_ctx;
        }

        self.leave_scope();
        self.scope.in_class = prev_in_class;
        self.scope.in_struct = prev_in_struct;
        self.scope.current_type_name = prev_type_name;
        self.scope.current_context = prev_context;
        Ok(())
    }
}
