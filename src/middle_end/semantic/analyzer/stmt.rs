use super::*;

impl SemanticAnalyzer {
    pub(crate) fn visit_statement(&mut self, stmt: &Stmt) -> Result<(), String> {
        match stmt {
            Stmt::Block(stmts) | Stmt::ThisBlock(stmts) => {
                self.enter_scope();
                for s in stmts {
                    self.visit_statement(s)?;
                }
                self.leave_scope();
            }

            Stmt::Declaration(decl) => {
                self.visit_declaration(decl)?;
            }

            Stmt::ReassignStmt { target, value, op } => {
                self.analyze_reassign(target, value, op)?;
            }

            Stmt::ExpressionStmt(expr) | Stmt::CallStmt(expr) => {
                self.visit_expression(expr)?;
            }

            Stmt::CompileValidation { body, args } => {
                for a in args {
                    let _ = self.visit_expression(a);
                }
                self.enter_scope();
                for s in body {
                    self.visit_statement(s)?;
                }
                self.leave_scope();
            }

            Stmt::CaseStmt { option, body, .. } => {
                self.enter_scope();
                if let Expr::Call { args, .. } = option {
                    for arg in args {
                        if let Expr::Identifier(var_name) = arg {
                            let info = SymbolInfo {
                                name: var_name.clone(),
                                kind: SymbolKind::Variable {
                                    type_node: BaseType::Unknown,
                                    editability: Editability::Editable,
                                    is_array: false,
                                },
                                visibility: Visibility::Private,
                                dependencies: vec![],
                                is_used: true,
                                is_param: false,
                                is_uninitialized: false,
                                is_compilable: false,
                            };
                            self
                                .current_env
                                .borrow_mut()
                                .define_or_update(var_name.clone(), info);
                        }
                    }
                } else if let Expr::Instantiate { args, .. } = option {
                    if args.len() == 1 {
                        if let Expr::ObjectLiteral(stmts) = &args[0] {
                            for s in stmts {
                                let var_name = match s {
                                    Stmt::Declaration(Decl::VarDecl { name, .. }) => {
                                        Some(name.clone())
                                    }
                                    Stmt::ReassignStmt { target, .. } => {
                                        if let Expr::Identifier(n) = target {
                                            Some(n.clone())
                                        } else {
                                            None
                                        }
                                    }
                                    Stmt::ExpressionStmt(Expr::Identifier(n)) => Some(n.clone()),
                                    Stmt::ReturnStmt(Some(Expr::Identifier(n))) => Some(n.clone()),
                                    _ => None,
                                };
                                if let Some(name) = var_name {
                                    let info = SymbolInfo {
                                        name: name.clone(),
                                        kind: SymbolKind::Variable {
                                            type_node: BaseType::Unknown,
                                            editability: Editability::Editable,
                                            is_array: false,
                                        },
                                        visibility: Visibility::Private,
                                        dependencies: vec![],
                                        is_used: true,
                                        is_param: false,
                                        is_uninitialized: false,
                                        is_compilable: false,
                                    };
                                    self
                                        .current_env
                                        .borrow_mut()
                                        .define_or_update(name.clone(), info);
                                }
                            }
                        }
                    }
                }
                for s in body {
                    self.visit_statement(s)?;
                }
                self.leave_scope();
            }

            Stmt::IfStmt {
                condition,
                then_block,
                else_block,
            } => {
                let cond_type = self.visit_expression(condition)?;
                let is_bool_like = cond_type == "bool"
                    || cond_type == "unknown"
                    || cond_type.ends_with('*');
                if !is_bool_like {
                    return Err("Semantic Error: if condition must be a boolean".to_string());
                }

                let static_val = self.eval_static_bool_expr(condition);
                if let Some(true) = static_val {
                    self.enter_scope();
                    for s in then_block {
                        self.visit_statement(s)?;
                    }
                    self.leave_scope();
                    // Dead branch elimination: else branch is eliminated, do not analyze or throw!
                    return Ok(());
                } else if let Some(false) = static_val {
                    if let Some(eb) = else_block {
                        self.enter_scope();
                        for s in eb {
                            self.visit_statement(s)?;
                        }
                        self.leave_scope();
                    }
                    return Ok(());
                }

                let prev_in_generic = self.scope.in_generic_template;
                self.scope.in_generic_template = true;
                self.enter_scope();
                for s in then_block {
                    self.visit_statement(s)?;
                }
                self.leave_scope();
                if let Some(eb) = else_block {
                    self.enter_scope();
                    for s in eb {
                        self.visit_statement(s)?;
                    }
                    self.leave_scope();
                }
                self.scope.in_generic_template = prev_in_generic;
            }

            Stmt::ForInStmt {
                item,
                iterable,
                body,
            } => {
                let iterable_type = self.visit_expression(iterable)?;
                let (base_name, type_args) = extract_type_args_from_str(&iterable_type);
                let bp_opt = self.current_env.borrow().lookup_blueprint(&base_name);
                let meta_opt = if bp_opt.is_none() {
                    self.global_metadata
                        .get(&base_name)
                        .or_else(|| self.global_metadata.get(&iterable_type))
                        .cloned()
                } else {
                    None
                };
                let item_type = if iterable_type.starts_with("array<")
                    || iterable_type.ends_with("[]")
                {
                    iterable_type
                        .trim_start_matches("array<")
                        .trim_end_matches('>')
                        .trim_end_matches("[]")
                        .to_string()
                } else if let Some(bp) = bp_opt {
                    let specialized_bp = bp.specialize(&type_args);
                    let has_iter = specialized_bp.handle_signatures.contains_key("iter")
                        || specialized_bp.handles.contains(&HandleMethods::Iter);
                    let has_is_done = specialized_bp
                        .handle_signatures
                        .contains_key("is_done_iter")
                        || specialized_bp.handles.contains(&HandleMethods::IterDone);
                    let has_next = specialized_bp.handle_signatures.contains_key("next")
                        || specialized_bp.handles.contains(&HandleMethods::Next);

                    if has_iter && (has_is_done || has_next) {
                        return Err(format!(
                            "Semantic Error: Type '{}' cannot implement both 'iter' and cursor handles ('is_done_iter'/'next'). An Iterable must return a separate Iterator cursor.",
                            iterable_type
                        ));
                    }

                    if has_iter {
                        self.record_dependency(base_name.clone());
                        self.record_dependency(format!("{}::iter", base_name));

                        let iter_sig = specialized_bp
                            .handle_signatures
                            .get("iter")
                            .unwrap();
                        let ret_str = iter_sig.return_type.as_str();
                        let (ret_base, ret_generics) = extract_type_args_from_str(&ret_str);

                        if ret_base == "void" {
                            return Err(format!(
                                "Semantic Error: 'iter' handle of type '{}' cannot return 'void'. It must return an Iterator cursor.",
                                iterable_type
                            ));
                        }
                        if ret_base == base_name {
                            return Err(format!(
                                "Semantic Error: 'iter' handle of type '{}' cannot return itself. An Iterable must return a separate Iterator cursor.",
                                iterable_type
                            ));
                        }

                        let target_bp_opt = self.current_env.borrow().lookup_blueprint(&ret_base);
                        if let Some(target_bp) = target_bp_opt {
                            let target_spec = target_bp.specialize(&ret_generics);
                            let t_has_iter = target_spec.handle_signatures.contains_key("iter")
                                || target_spec.handles.contains(&HandleMethods::Iter);
                            if t_has_iter {
                                return Err(format!(
                                    "Semantic Error: Iterator cursor '{}' returned by '{}::iter()' cannot implement 'iter'. It must only implement 'is_done_iter' and 'next'.",
                                    ret_base, iterable_type
                                ));
                            }
                            let t_has_done =
                                target_spec.handle_signatures.contains_key("is_done_iter")
                                    || target_spec.handles.contains(&HandleMethods::IterDone);
                            let t_has_next = target_spec.handle_signatures.contains_key("next")
                                || target_spec.handles.contains(&HandleMethods::Next);
                            if !t_has_done || !t_has_next {
                                return Err(format!(
                                    "Semantic Error: Iterator cursor '{}' returned by '{}::iter()' must implement 'is_done_iter' and 'next' handles.",
                                    ret_base, iterable_type
                                ));
                            }
                            self.record_dependency(ret_base.clone());
                            self.record_dependency(format!("{}::is_done_iter", ret_base));
                            self.record_dependency(format!("{}::next", ret_base));

                            if let Some(sig) = target_spec.handle_signatures.get("next") {
                                sig.return_type.as_str()
                            } else if !ret_generics.is_empty() {
                                ret_generics[0].as_str()
                            } else {
                                "int32".to_string()
                            }
                        } else if let Some(target_meta) =
                            self.global_metadata.get(&ret_base).cloned()
                        {
                            let t_has_iter = target_meta.handle_signatures.contains_key("iter")
                                || target_meta.handles.contains(&HandleMethods::Iter);
                            if t_has_iter {
                                return Err(format!(
                                    "Semantic Error: Iterator cursor '{}' returned by '{}::iter()' cannot implement 'iter'. It must only implement 'is_done_iter' and 'next'.",
                                    ret_base, iterable_type
                                ));
                            }
                            let t_has_done =
                                target_meta.handle_signatures.contains_key("is_done_iter")
                                    || target_meta.handles.contains(&HandleMethods::IterDone);
                            let t_has_next = target_meta.handle_signatures.contains_key("next")
                                || target_meta.handles.contains(&HandleMethods::Next);
                            if !t_has_done || !t_has_next {
                                return Err(format!(
                                    "Semantic Error: Iterator cursor '{}' returned by '{}::iter()' must implement 'is_done_iter' and 'next' handles.",
                                    ret_base, iterable_type
                                ));
                            }
                            self.record_dependency(ret_base.clone());
                            self.record_dependency(format!("{}::is_done_iter", ret_base));
                            self.record_dependency(format!("{}::next", ret_base));

                            if let Some(next_fn) = target_meta.handle_signatures.get("next") {
                                let mut map = HashMap::new();
                                let generics = match &target_meta.ty {
                                    BaseType::Struct { generics, .. }
                                    | BaseType::Class { generics, .. }
                                    | BaseType::Enum { generics, .. }
                                    | BaseType::Blueprint { generics, .. } => generics.clone(),
                                    _ => vec![],
                                };
                                for (g_param, g_arg) in generics
                                    .iter()
                                    .zip(ret_generics.iter().chain(type_args.iter()))
                                {
                                    map.insert(g_param.as_str(), g_arg.clone());
                                }
                                let specialized_ret = next_fn.return_type.substitute_generics(&map);
                                specialized_ret.as_str()
                            } else if !ret_generics.is_empty() {
                                ret_generics[0].as_str()
                            } else {
                                "int32".to_string()
                            }
                        } else {
                            return Err(format!(
                                "Semantic Error: Iterator cursor '{}' returned by '{}::iter()' is undefined.",
                                ret_base, iterable_type
                            ));
                        }
                    } else if has_is_done && has_next {
                        self.record_dependency(base_name.clone());
                        self.record_dependency(format!("{}::is_done_iter", base_name));
                        self.record_dependency(format!("{}::next", base_name));

                        if let Some(sig) = specialized_bp.handle_signatures.get("next") {
                            sig.return_type.as_str()
                        } else if !type_args.is_empty() {
                            type_args[0].as_str()
                        } else {
                            "int32".to_string()
                        }
                    } else {
                        return Err(format!(
                            "Semantic Error: Type '{}' cannot be iterated with 'for-in'. It must implement either 'iter' (returning an Iterator) or cursor handles ('is_done_iter' and 'next').",
                            iterable_type
                        ));
                    }
                } else if let Some(meta) = meta_opt {
                    let has_iter = meta.handle_signatures.contains_key("iter")
                        || meta.handles.contains(&HandleMethods::Iter);
                    let has_is_done = meta.handle_signatures.contains_key("is_done_iter")
                        || meta.handles.contains(&HandleMethods::IterDone);
                    let has_next = meta.handle_signatures.contains_key("next")
                        || meta.handles.contains(&HandleMethods::Next);

                    if has_iter && (has_is_done || has_next) {
                        return Err(format!(
                            "Semantic Error: Type '{}' cannot implement both 'iter' and cursor handles ('is_done_iter'/'next'). An Iterable must return a separate Iterator cursor.",
                            iterable_type
                        ));
                    }

                    if has_iter {
                        self.record_dependency(base_name.clone());
                        self.record_dependency(format!("{}::iter", base_name));

                        let iter_sig = meta
                            .handle_signatures
                            .get("iter")
                            .unwrap();
                        let ret_str = iter_sig.return_type.as_str();
                        let (ret_base, ret_generics) = extract_type_args_from_str(ret_str.as_str());

                        if ret_base == "void" {
                            return Err(format!(
                                "Semantic Error: 'iter' handle of type '{}' cannot return 'void'. It must return an Iterator cursor.",
                                iterable_type
                            ));
                        }
                        if ret_base == base_name {
                            return Err(format!(
                                "Semantic Error: 'iter' handle of type '{}' cannot return itself. An Iterable must return a separate Iterator cursor.",
                                iterable_type
                            ));
                        }

                        let target_bp_opt = self.current_env.borrow().lookup_blueprint(&ret_base);
                        if let Some(target_bp) = target_bp_opt {
                            let target_spec = target_bp.specialize(&ret_generics);
                            let t_has_iter = target_spec.handle_signatures.contains_key("iter")
                                || target_spec.handles.contains(&HandleMethods::Iter);
                            if t_has_iter {
                                return Err(format!(
                                    "Semantic Error: Iterator cursor '{}' returned by '{}::iter()' cannot implement 'iter'. It must only implement 'is_done_iter' and 'next'.",
                                    ret_base, iterable_type
                                ));
                            }
                            let t_has_done =
                                target_spec.handle_signatures.contains_key("is_done_iter")
                                    || target_spec.handles.contains(&HandleMethods::IterDone);
                            let t_has_next = target_spec.handle_signatures.contains_key("next")
                                || target_spec.handles.contains(&HandleMethods::Next);
                            if !t_has_done || !t_has_next {
                                return Err(format!(
                                    "Semantic Error: Iterator cursor '{}' returned by '{}::iter()' must implement 'is_done_iter' and 'next' handles.",
                                    ret_base, iterable_type
                                ));
                            }
                            self.record_dependency(ret_base.clone());
                            self.record_dependency(format!("{}::is_done_iter", ret_base));
                            self.record_dependency(format!("{}::next", ret_base));

                            if let Some(sig) = target_spec.handle_signatures.get("next") {
                                sig.return_type.as_str()
                            } else if !ret_generics.is_empty() {
                                ret_generics[0].as_str()
                            } else {
                                "int32".to_string()
                            }
                        } else if let Some(target_meta) =
                            self.global_metadata.get(&ret_base).cloned()
                        {
                            let t_has_iter = target_meta.handle_signatures.contains_key("iter")
                                || target_meta.handles.contains(&HandleMethods::Iter);
                            if t_has_iter {
                                return Err(format!(
                                    "Semantic Error: Iterator cursor '{}' returned by '{}::iter()' cannot implement 'iter'. It must only implement 'is_done_iter' and 'next'.",
                                    ret_base, iterable_type
                                ));
                            }
                            let t_has_done =
                                target_meta.handle_signatures.contains_key("is_done_iter")
                                    || target_meta.handles.contains(&HandleMethods::IterDone);
                            let t_has_next = target_meta.handle_signatures.contains_key("next")
                                || target_meta.handles.contains(&HandleMethods::Next);
                            if !t_has_done || !t_has_next {
                                return Err(format!(
                                    "Semantic Error: Iterator cursor '{}' returned by '{}::iter()' must implement 'is_done_iter' and 'next' handles.",
                                    ret_base, iterable_type
                                ));
                            }
                            self.record_dependency(ret_base.clone());
                            self.record_dependency(format!("{}::is_done_iter", ret_base));
                            self.record_dependency(format!("{}::next", ret_base));

                            if let Some(next_fn) = target_meta.handle_signatures.get("next") {
                                let mut map = HashMap::new();
                                let generics = match &target_meta.ty {
                                    BaseType::Struct { generics, .. }
                                    | BaseType::Class { generics, .. }
                                    | BaseType::Enum { generics, .. }
                                    | BaseType::Blueprint { generics, .. } => generics.clone(),
                                    _ => vec![],
                                };
                                for (g_param, g_arg) in generics
                                    .iter()
                                    .zip(ret_generics.iter().chain(type_args.iter()))
                                {
                                    map.insert(g_param.as_str(), g_arg.clone());
                                }
                                let specialized_ret = next_fn.return_type.substitute_generics(&map);
                                specialized_ret.as_str()
                            } else if !ret_generics.is_empty() {
                                ret_generics[0].as_str()
                            } else {
                                "int32".to_string()
                            }
                        } else {
                            return Err(format!(
                                "Semantic Error: Iterator cursor '{}' returned by '{}::iter()' is undefined.",
                                ret_base, iterable_type
                            ));
                        }
                    } else if has_is_done && has_next {
                        self.record_dependency(base_name.clone());
                        self.record_dependency(format!("{}::is_done_iter", base_name));
                        self.record_dependency(format!("{}::next", base_name));

                        if let Some(next_fn) = meta.handle_signatures.get("next") {
                            let mut map = HashMap::new();
                            let generics = match &meta.ty {
                                BaseType::Struct { generics, .. }
                                | BaseType::Class { generics, .. }
                                | BaseType::Enum { generics, .. }
                                | BaseType::Blueprint { generics, .. } => generics.clone(),
                                _ => vec![],
                            };
                            for (g_param, g_arg) in generics.iter().zip(type_args.iter()) {
                                map.insert(g_param.as_str(), g_arg.clone());
                            }
                            let specialized_ret = next_fn.return_type.substitute_generics(&map);
                            specialized_ret.as_str()
                        } else if !type_args.is_empty() {
                            type_args[0].as_str()
                        } else {
                            "int32".to_string()
                        }
                    } else {
                        return Err(format!(
                            "Semantic Error: Type '{}' cannot be iterated with 'for-in'. It must implement either 'iter' (returning an Iterator) or cursor handles ('is_done_iter' and 'next').",
                            iterable_type
                        ));
                    }
                } else {
                    return Err(format!(
                        "Semantic Error: Type '{}' cannot be iterated with 'for-in'. It must implement either 'iter' (returning an Iterator) or cursor handles ('is_done_iter' and 'next').",
                        iterable_type
                    ));
                };

                self.enter_scope();

                if let Stmt::Declaration(Decl::VarDecl {
                    type_node, name, ..
                }) = &**item
                {
                    let declared_type = type_node.as_str();
                    if item_type != "unknown"
                        && !self.types_are_compatible(&declared_type, &item_type)
                    {
                        return Err(
                            format!(
                                "Semantic Error: Type mismatch in for-in loop. Iterable elements are '{}', but item is declared as '{}'",
                                item_type,
                                declared_type
                            )
                        );
                    }
                    self.visit_statement(item)?;
                    self.current_env.borrow_mut().mark_initialized(name);
                } else if let Stmt::ExpressionStmt(Expr::Identifier(var_name)) = &**item {
                    let info = SymbolInfo {
                        name: var_name.clone(),
                        kind: SymbolKind::Variable {
                            type_node: BaseType::from_str(&item_type),
                            editability: Editability::Editable,
                            is_array: false,
                        },
                        visibility: Visibility::Private,
                        dependencies: vec![],
                        is_used: false,
                        is_param: false,
                        is_uninitialized: false,
                        is_compilable: false,
                    };
                    self.current_env
                        .borrow_mut()
                        .define(var_name.clone(), info)?;
                } else {
                    self.visit_statement(item)?;
                }

                match body {
                    EitherBlock::Inline(stmts) => {
                        let was_break = self.active_flags.contains(&Flag::HasBreak);
                        self.active_flags.insert(Flag::HasBreak);
                        for s in stmts {
                            self.visit_statement(s)?;
                        }
                        if !was_break {
                            self.active_flags.remove(&Flag::HasBreak);
                        }
                    }
                    EitherBlock::External(expr) => {
                        self.visit_expression(expr)?;
                    }
                }
                self.leave_scope();
            }

            Stmt::WhileStmt { condition, body } => {
                let cond_type = self.visit_expression(condition)?;
                if cond_type != "bool" && cond_type != "unknown" {
                    return Err("Semantic Error: loop condition must be a boolean".to_string());
                }
                match body {
                    EitherBlock::Inline(stmts) => {
                        self.enter_scope();
                        let was_break = self.active_flags.contains(&Flag::HasBreak);
                        self.active_flags.insert(Flag::HasBreak);
                        for s in stmts {
                            self.visit_statement(s)?;
                        }
                        if !was_break {
                            self.active_flags.remove(&Flag::HasBreak);
                        }
                        self.leave_scope();
                    }
                    EitherBlock::External(expr) => {
                        self.visit_expression(expr)?;
                    }
                }
            }

            Stmt::DoWhileStmt { body, condition } => {
                let cond_type = self.visit_expression(condition)?;
                if cond_type != "bool" && cond_type != "unknown" {
                    return Err("Semantic Error: do-while condition must be a boolean".to_string());
                }
                match body {
                    EitherBlock::Inline(stmts) => {
                        self.enter_scope();
                        let was_break = self.active_flags.contains(&Flag::HasBreak);
                        self.active_flags.insert(Flag::HasBreak);
                        for s in stmts {
                            self.visit_statement(s)?;
                        }
                        if !was_break {
                            self.active_flags.remove(&Flag::HasBreak);
                        }
                        self.leave_scope();
                    }
                    EitherBlock::External(expr) => {
                        self.visit_expression(expr)?;
                    }
                }
            }

            Stmt::LoopStmt { count, body } => {
                if let Some(c) = count {
                    let c_type = self.visit_expression(c)?;
                    if !c_type.starts_with("int")
                        && !c_type.starts_with("uint")
                        && c_type != "unknown"
                    {
                        return Err("Semantic Error: loop count must be an integer".to_string());
                    }
                }
                match body {
                    EitherBlock::Inline(stmts) => {
                        self.enter_scope();
                        let was_break = self.active_flags.contains(&Flag::HasBreak);
                        self.active_flags.insert(Flag::HasBreak);
                        for s in stmts {
                            self.visit_statement(s)?;
                        }
                        if !was_break {
                            self.active_flags.remove(&Flag::HasBreak);
                        }
                        self.leave_scope();
                    }
                    EitherBlock::External(expr) => {
                        self.visit_expression(expr)?;
                    }
                }
            }

            Stmt::SwitchStmt {
                condition, cases, ..
            } => {
                self.visit_expression(condition)?;
                self.enter_scope();
                let was_break = self.active_flags.contains(&Flag::HasBreak);
                self.active_flags.insert(Flag::HasBreak);
                for s in cases {
                    if let Stmt::CaseStmt { option, .. } = s {
                        Self::validate_match_pattern(option)?;
                    }
                    self.visit_statement(s)?;
                }
                if !was_break {
                    self.active_flags.remove(&Flag::HasBreak);
                }
                self.leave_scope();
            }

            Stmt::DelStmt { target, is_array } => {
                if let Expr::Identifier(name) = target {
                    let var_info = if let Some(info) = self.current_env.borrow().lookup(name) {
                        info
                    } else {
                        return Err(format!(
                            "Semantic Error: Cannot delete non-existent variable '{}'",
                            name
                        ));
                    };
                    self.current_env.borrow_mut().mark_used(name);

                    // Stage 1: Double-Delete Check
                    if self.memory.deleted_vars.contains(name) {
                        return Err(format!(
                            "Semantic Error: Variable '{}' has already been deleted.",
                            name
                        ));
                    }

                    let type_str = var_info.type_str();
                    let is_pointer_or_ref = type_str.ends_with('*')
                        || type_str.starts_with("array<")
                        || type_str.ends_with("[]")
                        || *is_array;

                    let is_on_heap = self.memory.heap_allocated_vars.contains(name)
                        || is_pointer_or_ref
                        || self.is_class_type(&type_str);

                    // Stage 2: Non-heap primitive check
                    if self.is_primitive_stack_type(&type_str) && !is_pointer_or_ref && !is_on_heap
                    {
                        return Err(format!("Semantic Error: Cannot delete variable '{}' because it is not allocated on the heap.", name));
                    }

                    // Stage 3: Class, Struct, Blueprint check for drop handle
                    if self.is_class_type(&type_str) {
                        if !self.type_has_drop_handle(&type_str) {
                            return Err(format!(
                                "Semantic Error: Cannot delete object '{}' of class '{}' because it does not implement a 'drop' handle.",
                                name, type_str
                            ));
                        }
                    } else if self.is_struct_or_blueprint_type(&type_str) && !is_pointer_or_ref
                        && !self.type_has_drop_handle(&type_str) {
                            return Err(format!(
                                "Semantic Error: Cannot delete object '{}' of type '{}' because it does not implement a 'drop' handle.",
                                name, type_str
                            ));
                        }

                    // Mark as deleted and remove from active heap variables
                    self.memory.deleted_vars.insert(name.clone());
                    self.memory.heap_allocated_vars.remove(name);
                } else {
                    self.visit_expression(target)?;
                }
            }

            Stmt::ForStmt {
                init,
                condition,
                increment,
                body,
            } => {
                self.enter_scope();
                if let Some(i) = init {
                    self.visit_statement(i)?;
                }
                if let Some(c) = condition {
                    let cond_type = self.visit_expression(c)?;
                    if cond_type != "bool" && cond_type != "unknown" {
                        return Err("Semantic Error: for condition must be a boolean".to_string());
                    }
                }
                if let Some(inc) = increment {
                    self.visit_statement(inc)?;
                }
                match body {
                    EitherBlock::Inline(stmts) => {
                        self.enter_scope();
                        let was_break = self.active_flags.contains(&Flag::HasBreak);
                        self.active_flags.insert(Flag::HasBreak);
                        for s in stmts {
                            self.visit_statement(s)?;
                        }
                        if !was_break {
                            self.active_flags.remove(&Flag::HasBreak);
                        }
                        self.leave_scope();
                    }
                    EitherBlock::External(expr) => {
                        self.visit_expression(expr)?;
                    }
                }
                self.leave_scope();
            }

            Stmt::ReturnStmt(expr) => {
                if !self.active_flags.contains(&Flag::HasReturn) {
                    return Err(
                        "Semantic Error: Return statement is not allowed in this scope. 'has_return' flag is not enabled.".to_string()
                    );
                }
                let actual_type = if expr.is_none() {
                    "void".to_string()
                } else {
                    self.visit_expression(&expr.clone().unwrap())?
                };
                if let Some(expected_type) = &self.active_return_type.clone() {
                    let expected_str = expected_type.as_str();
                    let is_main = self.scope.current_context.as_deref() == Some("main");
                    let main_allow = is_main
                        && (actual_type == "void"
                            || ((expected_str == "void" || expected_str == "int" || expected_str == "int32")
                                && (actual_type == "int" || actual_type == "int32" || actual_type == "void")));
                    if !main_allow
                        && actual_type != "unknown"
                        && expected_str != "unknown"
                        && !self.types_are_compatible(&expected_str, &actual_type)
                    {
                        return Err(format!(
                            "Semantic Error: Return type mismatch. Expected '{}', got '{}'",
                            expected_str, actual_type
                        ));
                    }
                }
            }

            Stmt::BreakStmt => {
                if !self.active_flags.contains(&Flag::HasBreak) {
                    return Err(
                        "Semantic Error: Break statement is not allowed outside loops or switch statements.".to_string()
                    );
                }
            }

            Stmt::ThrowStmt(expr) => {
                if !self.active_flags.contains(&Flag::HasThrow) {
                    return Err(
                        "Semantic Error: Throw statement is not allowed here. The scope must have 'has_throw' enabled (e.g. inside try block or custom scope with error handle).".to_string()
                    );
                }
                let thrown_type = self.visit_expression(expr)?;
                if thrown_type != "unknown" {
                    let thrown_base = BaseType::from_str(&thrown_type);
                    let bp_name = thrown_base
                        .blueprint_name()
                        .unwrap_or(&thrown_type);
                    let is_throwable = if let Some(bp) = self.current_env.borrow().lookup_blueprint(bp_name) {
                        bp.handles.contains(&HandleMethods::Throw)
                            || bp.handle_signatures.contains_key("throw")
                            || bp.handle_signatures.contains_key("$throw")
                    } else if let Some(meta) = self.global_metadata.get(bp_name) {
                        meta.handles.contains(&HandleMethods::Throw)
                            || meta.handle_signatures.contains_key("throw")
                            || meta.handle_signatures.contains_key("$throw")
                    } else {
                        false
                    };
                    if !is_throwable {
                        return Err(
                            format!("Semantic Error: Type '{}' is not throwable. Only types implementing a 'throw' handle can be thrown.", thrown_type)
                        );
                    }
                }
            }

            Stmt::TryCatchStmt {
                try_block,
                catches,
            } => {
                self.enter_scope();
                self.active_flags.insert(Flag::HasThrow);
                for s in try_block {
                    self.visit_statement(s)?;
                }
                self.active_flags.remove(&Flag::HasThrow);
                self.leave_scope();

                for catch_clause in catches {
                    self.enter_scope();
                    let catch_type = catch_clause.param_type.as_ref().cloned().unwrap_or_else(|| {
                        self.record_dependency("Error".to_string());
                        BaseType::from_str("Error")
                    });
                    let resolved_type = self.resolve_type(&catch_type);
                    let info = SymbolInfo {
                        name: catch_clause.param.clone(),
                        kind: SymbolKind::Variable {
                            type_node: resolved_type,
                            editability: Editability::NotEditable,
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
                        .define(catch_clause.param.clone(), info)?;
                    for s in &catch_clause.body {
                        self.visit_statement(s)?;
                    }
                    self.leave_scope();
                }
            }

            Stmt::GotoStmt(_) => {}

            Stmt::YieldStmt(expr) => {
                if !self.active_flags.contains(&Flag::HasYield) {
                    return Err(
                        "Semantic Error: Yield statement is not allowed in this scope (forbidden in global scope).".to_string()
                    );
                }
                if let Some(e) = expr {
                    self.visit_expression(e)?;
                }
            }

            Stmt::LeaveStmt => {
                if !self.active_flags.contains(&Flag::HasLeave) {
                    return Err(
                        "Semantic Error: Leave statement is not allowed in this scope (forbidden in global scope).".to_string()
                    );
                }
            }

            Stmt::UsingStmt(name) => {
                if let Some(meta) = self.global_metadata.get(name) {
                    if let Some(ref variants) = meta.variants {
                        for v in variants {
                            let var_symbol = SymbolInfo {
                                name: v.name.clone(),
                                kind: SymbolKind::Variable {
                                    type_node: BaseType::from_str(name),
                                    editability: Editability::NotEditable,
                                    is_array: false,
                                },
                                visibility: Visibility::Public,
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
                    }
                }
                let bp_opt = self.current_env.borrow().lookup_blueprint(name);
                if let Some(bp) = bp_opt {
                    if !bp.variants.is_empty() {
                        for v in &bp.variants {
                            let var_symbol = SymbolInfo {
                                name: v.name.clone(),
                                kind: SymbolKind::Variable {
                                    type_node: BaseType::from_str(name),
                                    editability: Editability::NotEditable,
                                    is_array: false,
                                    },
                                visibility: Visibility::Public,
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
                    }
                }
                let ns_env_opt = self.current_env.borrow().lookup_namespace(name);
                if let Some(ns_env) = ns_env_opt {
                    let (symbols, blueprints) = {
                        let borrowed = ns_env.borrow();
                        (borrowed.symbols.clone(), borrowed.blueprints.clone())
                    };
                    for (sym_name, sym_info) in symbols {
                        self.current_env
                            .borrow_mut()
                            .define_or_update(sym_name, sym_info);
                    }
                    for (bp_name, bp_data) in blueprints {
                        self.current_env
                            .borrow_mut()
                            .define_blueprint(bp_name, bp_data);
                    }
                }
            }
            Stmt::SetNamespace(_) => {}
            _ => {}
        }
        Ok(())
    }

    // ----------------------------------------------------------
    // analyze_reassign - ReassignStmt with full operator awareness
    // ----------------------------------------------------------
    pub(crate) fn analyze_reassign(
        &mut self,
        target: &Expr,
        value: &Expr,
        op: &str,
    ) -> Result<(), String> {
        // Scope flags are strictly read-only
        if let Expr::Identifier(name) = target {
            let flag = Flag::from_str(name.trim());
            if !matches!(flag, Flag::Custom(_)) {
                return Err(format!(
                    "Semantic Error: Scope flag '{}' is read-only and cannot be manually modified",
                    name
                ));
            }
        }
        // Check mutability and pointer lifecycle
        if let Expr::Identifier(name) = target {
            if let Some(info) = self.current_env.borrow().lookup(name) {
                if !info.is_editable() {
                    return Err(format!(
                        "Semantic Error: Cannot reassign constant '{}'",
                        name
                    ));
                }
            }
        }

        let expr_type = self.visit_expression(value)?;
        if let Expr::Identifier(name) = target {
            if self.memory.deleted_vars.contains(name) {
                if matches!(value, Expr::Instantiate { .. }) {
                    self.memory.deleted_vars.remove(name);
                    self.memory.heap_allocated_vars.insert(name.clone());
                } else {
                    return Err(format!("Semantic Error: Cannot assign to variable '{}' because it has been deleted.", name));
                }
            } else if matches!(value, Expr::Instantiate { .. }) {
                self.memory.heap_allocated_vars.insert(name.clone());
            }
            if expr_type != "undefined" {
                self.current_env.borrow_mut().mark_initialized(name);
            }
        }
        let target_type = self.visit_expression(target)?;

        if target_type.starts_with("address<") || target_type.ends_with('&') {
            return Err(format!(
                "Semantic Error: Cannot assign to read-only address reference '{}'",
                target_type
            ));
        }

        if op != "=" {
            return self.verify_operator_overload(&target_type, &expr_type, op, target);
        }

        self.verify_type_assignment(&target_type, &expr_type, target)
    }

    // ----------------------------------------------------------
    // verify_operator_overload
    // ----------------------------------------------------------
    pub(crate) fn verify_operator_overload(
        &mut self,
        target_type: &str,
        expr_type: &str,
        op: &str,
        _target: &Expr,
    ) -> Result<(), String> {
        let mut expr_type = expr_type;
        if expr_type == "int" {
            expr_type = "int32";
        }
        // Compound operators on primitive numerics are always allowed
        // e.g. counter += 1; x -= 2; x *= 2;

        // Complex types: look up handle
        let target_base = BaseType::from_str(target_type);
        if target_base.is_complex() {
            let bp_name = target_base
                .blueprint_name()
                .unwrap_or(target_type);

            return match resolve_handle_for_op(&self.current_env, bp_name, op) {
                HandleLookupResult::Found(bp) => {
                    let handle = op_to_handle(op);
                    if !bp.handle_accepts_type(handle, expr_type) {
                        Err(
                            format!(
                                "Semantic Error: Handle '{}' in '{}' does not accept type '{}'. Check the handle's parameter type.",
                                handle.as_str(),
                                bp_name,
                                expr_type
                            )
                        )
                    } else {
                        Ok(())
                    }
                }
                HandleLookupResult::BlueprintNotFound => Ok(()),
                HandleLookupResult::HandleMissing { handle } => {
                    if let Some(forwarded_target) = self.resolve_property_access_target_type(target_type) {
                        return self.verify_operator_overload(&forwarded_target, expr_type, op, _target);
                    }
                    Err(
                        format!(
                            "Semantic Error: Type '{}' does not support the '{}' operator (missing handle '{}').",
                            bp_name,
                            op,
                            handle.as_str()
                        )
                    )
                }
                HandleLookupResult::UnknownOp => Ok(()),
            };
        }

        // Primitive with unknown operator -> allow if types unknown
        if target_type == "unknown" || expr_type == "unknown" {
            return Ok(());
        }

        Ok(())
    }

    // ----------------------------------------------------------
    // verify_type_assignment - normal "=" assignment check
    // ----------------------------------------------------------
    pub(crate) fn verify_type_assignment(
        &mut self,
        target_type: &str,
        expr_type: &str,
        _target: &Expr,
    ) -> Result<(), String> {
        if expr_type == "default" || expr_type == "unknown" {
            return Ok(());
        }

        // ----------------------------------------------------------
        if target_type == expr_type || target_type == "unknown" || expr_type == "unknown" {
            return Ok(());
        }
        if target_type == "type" || target_type.starts_with("type<") {
            return Ok(());
        }

        if !self.types_are_compatible(target_type, expr_type) {
            return Err(format!(
                "Semantic Error: Cannot assign '{}' to type '{}'",
                expr_type, target_type
            ));
        }
        Ok(())
    }

    pub(crate) fn block_always_terminates(&self, stmts: &[Stmt]) -> bool {
        for s in stmts {
            if self.stmt_always_terminates(s) {
                return true;
            }
        }
        false
    }

    pub(crate) fn stmt_always_terminates(&self, stmt: &Stmt) -> bool {
        match stmt {
            Stmt::ReturnStmt(_)
            | Stmt::ThrowStmt(_)
            | Stmt::LeaveStmt
            | Stmt::YieldStmt(_)
            | Stmt::GotoStmt(_)
            | Stmt::SwitchStmt { .. }
            | Stmt::CaseStmt { .. } => true,
            Stmt::IfStmt {
                then_block,
                else_block: Some(eb),
                ..
            } => self.block_always_terminates(then_block) && self.block_always_terminates(eb),
            Stmt::IfStmt { .. } => false,
            Stmt::TryCatchStmt {
                try_block,
                catches,
            } => {
                self.block_always_terminates(try_block)
                    && !catches.is_empty()
                    && catches.iter().all(|c| self.block_always_terminates(&c.body))
            }
            Stmt::DoWhileStmt { body, .. } => match body {
                EitherBlock::Inline(stmts) => self.block_always_terminates(stmts),
                EitherBlock::External(_) => false,
            },
            _ => false,
        }
    }

    pub(crate) fn validate_match_pattern(expr: &Expr) -> Result<(), String> {
        if let Expr::BinaryOp {
                left,
                operator,
                right,
            } = expr {
            if operator == "||" || operator == "or" {
                return Err(
                    "Semantic Error: Use single pipe '|' for pattern alternation in match, '||' and 'or' are not allowed in match patterns.".to_string()
                );
            }
            Self::validate_match_pattern(left)?;
            Self::validate_match_pattern(right)?;
        }
        Ok(())
    }
}
