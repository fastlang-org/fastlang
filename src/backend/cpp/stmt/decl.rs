use crate::backend::cpp::generator::CodeGenerator;
use crate::backend::cpp::stmt::types::*;
use crate::frontend::parser::ast::*;
use crate::middle_end::semantic::analyzer::detect_execution_mode;

impl CodeGenerator {
    pub(crate) fn visit_declaration(&mut self, decl: &Decl) {
        match decl {
            Decl::EnumDecl { .. } => self.visit_enum_decl(decl),
            Decl::ClassDecl { .. } => self.visit_class_decl(decl),
            Decl::StructDecl { .. } => self.visit_struct_decl(decl),
            Decl::MachineDecl { .. } => self.visit_machine_decl(decl),

            Decl::VarDecl {
                name,
                type_node,
                value,
                editability,
                assign_op,
                place: _,
                ..
            } => {
                let cpp_type = if matches!(type_node, BaseType::Unknown) || assign_op == ":=" {
                    "auto".to_string()
                } else {
                    type_to_cpp(type_node)
                };
                if is_raw_pointer(type_node, &cpp_type) {
                    self.raw_pointer_vars.insert(name.clone());
                    self.raw_pointer_vars.insert(format!("this->{}", name));
                    self.pointer_vars.insert(name.clone());
                    self.pointer_vars.insert(format!("this->{}", name));
                } else if matches!(type_node, BaseType::Address(_)) || cpp_type.ends_with('&') {
                    self.address_vars.insert(name.clone());
                    self.address_vars.insert(format!("this->{}", name));
                }
                let is_param = matches!(value, Expr::Identifier(s) if s == "__param__");
                let is_const = editability == &Editability::NotEditable;
                let const_prefix = if is_const { "const " } else { "" };
                let static_prefix = if self.in_static_block { "static inline " } else { "" };
                self.vars.insert(name.clone(), cpp_type.clone());
                if let Some(ref cls) = self.current_class_name {
                    self.struct_field_types
                        .entry(cls.clone())
                        .or_default()
                        .insert(name.clone(), cpp_type.clone());
                }
                let is_smart_or_raw = is_raw_pointer(type_node, &cpp_type)
                    || (cpp_type != "auto" && {
                        if let Expr::New {
                            type_node: inner, ..
                        } = value
                        {
                            type_to_cpp(inner) != cpp_type
                        } else {
                            false
                        }
                    });

                let mut val_code = if !is_smart_or_raw {
                    if let Expr::New {
                        type_node: inner,
                        target,
                    } = value
                    {
                        let inner_cpp = type_to_cpp(inner);
                        match &**target {
                            Expr::Instantiate { args, .. } => {
                                let arg_strs: Vec<String> =
                                    args.iter().map(|a| self.visit_expression(a)).collect();
                                format!("{}({})", inner_cpp, arg_strs.join(", "))
                            }
                            Expr::ArrayLiteral(elems) => {
                                let elem_strs: Vec<String> =
                                    elems.iter().map(|e| self.visit_expression(e)).collect();
                                format!("{}{{{}}}", inner_cpp, elem_strs.join(", "))
                            }
                            _ => {
                                let target_code = self.visit_expression(target);
                                if target_code == "__default__"
                                    || target_code == "{}"
                                    || target_code.is_empty()
                                {
                                    format!("{}()", inner_cpp)
                                } else {
                                    format!("{}({})", inner_cpp, target_code)
                                }
                            }
                        }
                    } else {
                        self.visit_expression(value)
                    }
                } else {
                    self.visit_expression(value)
                };

                if (matches!(type_node, BaseType::Address(_)) || cpp_type.ends_with('&'))
                    && val_code.starts_with('&')
                {
                    val_code = val_code[1..].to_string();
                }

                if cpp_type.contains('<') {
                    let raw_name = cpp_type.split('<').next().unwrap_or(&cpp_type);
                    let prefix = format!("{}::", raw_name);
                    if val_code.starts_with(&prefix) {
                        val_code = format!("{}::{}", cpp_type, &val_code[prefix.len()..]);
                    }
                }

                if self
                    .current_block_vars
                    .as_ref()
                    .is_some_and(|bv| bv.contains(name))
                {
                    if assign_op == "->" || (!val_code.is_empty() && val_code != "{}") {
                        self.emit(&format!("this->{} = {};", name, val_code));
                    }
                } else if is_param || (matches!(value, Expr::Default(None)) && assign_op.is_empty())
                {
                    self.emit(&format!("{}{} {};", const_prefix, cpp_type, name));
                } else if assign_op == "->" {
                    if self
                        .find_assign_handle(&cpp_type, &["arrow_assign"])
                        .is_some()
                        || (val_code.starts_with('{') && val_code.ends_with('}'))
                    {
                        self.emit(&format!("{}{} {};", const_prefix, cpp_type, name));
                        self.emit(&format!("{}.arrow_assign({});", name, val_code));
                    } else {
                        self.emit(&format!(
                            "{}{}{} {} = {};",
                            static_prefix, const_prefix, cpp_type, name, val_code
                        ));
                    }
                } else if assign_op == "=" || assign_op == ":=" || assign_op.is_empty() {
                    if self
                        .find_assign_handle(&cpp_type, &["equal_assign"])
                        .is_some()
                    {
                        self.emit(&format!("{}{}{} {};", static_prefix, const_prefix, cpp_type, name));
                        self.emit(&format!("{}.equal_assign({});", name, val_code));
                    } else {
                        self.emit(&format!(
                            "{}{}{} {} = {};",
                            static_prefix, const_prefix, cpp_type, name, val_code
                        ));
                    }
                } else {
                    self.emit(&format!(
                        "{}{}{} {} {} {};",
                        static_prefix, const_prefix, cpp_type, name, assign_op, val_code
                    ));
                }
            }
            Decl::DestructureDecl {
                visibility: _,
                editability,
                type_node,
                assignments,
                assign_op,
                ..
            } => {
                let cpp_type = type_to_cpp(type_node);
                let is_const = editability == &Editability::NotEditable;
                let const_prefix = if is_const { "const " } else { "" };

                for (name, val) in assignments {
                    let val_code = self.visit_expression(val);
                    if matches!(val, Expr::Default(None)) || val_code == "{}" {
                        self.emit(&format!("{}{} {};", const_prefix, cpp_type, name));
                    } else if assign_op == "->" || assign_op == "=" {
                        self.emit(&format!(
                            "{}{} {} = {};",
                            const_prefix, cpp_type, name, val_code
                        ));
                    } else {
                        self.emit(&format!(
                            "{}{} {} {} {};",
                            const_prefix, cpp_type, name, assign_op, val_code
                        ));
                    }
                }
            }
            Decl::ObjectDestructureDecl {
                editability,
                type_name,
                fields,
                rhs,
                ..
            } => {
                let rhs_code = self.visit_expression(rhs);
                let is_const = editability == &Editability::NotEditable;
                let const_prefix = if is_const { "const " } else { "" };

                let inferred_type = if let Some(t) = type_name {
                    Some(t.clone())
                } else if let Expr::Call { callee, .. } = rhs {
                    if let Expr::Identifier(fn_name) = &**callee {
                        self.fn_return_types.get(fn_name).cloned()
                    } else {
                        None
                    }
                } else if let Expr::Instantiate { target, .. } = rhs {
                    if let Expr::Identifier(t_name) = &**target {
                        Some(t_name.clone())
                    } else {
                        None
                    }
                } else {
                    None
                };

                let clean_inferred = inferred_type.map(|t| {
                    t.trim_start_matches("blueprint::")
                        .trim_start_matches("struct::")
                        .trim_start_matches("class::")
                        .to_string()
                });

                let total_fields = clean_inferred
                    .as_ref()
                    .and_then(|t| self.struct_field_counts.get(t))
                    .copied();

                if let Some(total_count) = total_fields {
                    if total_count > fields.len() {
                        self.yield_counter += 1;
                        let mut dummy_names = Vec::new();
                        for i in 0..total_count {
                            dummy_names.push(format!("__d_{}_{}", self.yield_counter, i));
                        }
                        self.emit(&format!(
                            "auto [{}] = {};",
                            dummy_names.join(", "),
                            rhs_code
                        ));
                        for (idx, (type_node, name)) in fields.iter().enumerate() {
                            let cpp_t = if matches!(type_node, BaseType::Unknown) {
                                "auto".to_string()
                            } else {
                                type_to_cpp(type_node)
                            };
                            self.emit(&format!(
                                "{}{} {} = {};",
                                const_prefix, cpp_t, name, dummy_names[idx]
                            ));
                        }
                        return;
                    }
                }

                let names: Vec<String> = fields.iter().map(|(_, name)| name.clone()).collect();
                self.emit(&format!(
                    "{}auto [{}] = {};",
                    const_prefix,
                    names.join(", "),
                    rhs_code
                ));
            }
            Decl::ArrayDecl {
                visibility: _,
                editability,
                type_node,
                name,
                length,
                value,
                assign_op: _,
            } => {
                let cpp_type = type_to_cpp(type_node);
                self.vars
                    .insert(name.clone(), format!("fast_std::array<{}>", cpp_type));
                let is_const = editability == &Editability::NotEditable;
                let const_prefix = if is_const { "const " } else { "" };

                let val_code = match value {
                    Expr::ArrayLiteral(elems) if elems.is_empty() => {
                        format!("fastlang_array_alloc<{}>(0)", cpp_type)
                    }
                    Expr::ArrayLiteral(elems)
                        if !elems.iter().any(|e| matches!(e, Expr::Spread(_))) =>
                    {
                        let elems_code: Vec<String> =
                            elems.iter().map(|e| self.visit_expression(e)).collect();
                        format!(
                            "fastlang_array_create<{}>({{{}}})",
                            cpp_type,
                            elems_code.join(", ")
                        )
                    }
                    Expr::Default(None) => {
                        let len_code = self.visit_expression(length);
                        if len_code != "0" && !len_code.is_empty() {
                            format!("fastlang_array_alloc<{}>({})", cpp_type, len_code)
                        } else {
                            format!("fastlang_array_alloc<{}>(0)", cpp_type)
                        }
                    }
                    _ => self.visit_expression(value),
                };

                if val_code == "__param__" {
                    let len_code = self.visit_expression(length);
                    self.emit(&format!(
                        "{}{} {}[{}];",
                        const_prefix, cpp_type, name, len_code
                    ));
                } else {
                    self.emit(&format!(
                        "{}fast_std::array<{}> {} = {};",
                        const_prefix, cpp_type, name, val_code
                    ));
                }
            }

            Decl::FnAliasDecl { name, target, .. } => {
                self.emit(&format!(
                    "template<typename... Args> inline auto {}(Args&&... args) -> decltype({}(std::forward<Args>(args)...)) {{ return {}(std::forward<Args>(args)...); }}",
                    name, target, target
                ));
            }

            Decl::FnDecl {
                name,
                generics,
                params,
                return_type,
                body,
                is_virtual,
                is_abstract,
                visibility: _,
            } => {
                if name.starts_with("@compile::")
                    || detect_execution_mode(body) == ExecutionMode::FullyCompilable
                {
                    return;
                }

                let has_variadic_param = params.iter().any(|p| p.is_variadic);
                let mut gen_names: Vec<String> = generics
                    .iter()
                    .map(|g| {
                        let s = g.as_str();
                        if s.starts_with("...") {
                            format!("typename... {}", s.trim_start_matches('.'))
                        } else {
                            format!("typename {}", s)
                        }
                    })
                    .collect();
                if gen_names.is_empty() && has_variadic_param {
                    gen_names.push("typename... __Args".to_string());
                }
                if !gen_names.is_empty() {
                    self.emit(&format!("template <{}>\n", gen_names.join(", ")));
                }

                let ret_type_str = if name == "main" {
                    "int".to_string()
                } else {
                    type_to_cpp(return_type)
                };

                let prev_vars = self.vars.clone();
                let prev_raw_pointers = self.raw_pointer_vars.clone();
                let prev_pointers = self.pointer_vars.clone();
                let prev_address_vars = self.address_vars.clone();

                let mut param_strs = Vec::new();
                let mut added_variadics = Vec::new();
                let is_already_fwd = self.forward_declared_fns.contains(name);
                for param in params {
                    if param.is_variadic {
                        self.variadic_packs.insert(param.name.clone());
                        added_variadics.push(param.name.clone());
                    }
                    let cpp_pt = type_to_cpp(&param.type_node);
                    if is_raw_pointer(&param.type_node, &cpp_pt) {
                        self.raw_pointer_vars.insert(param.name.clone());
                        self.pointer_vars.insert(param.name.clone());
                    } else if matches!(&param.type_node, BaseType::Address(_))
                        || cpp_pt.ends_with('&')
                    {
                        self.address_vars.insert(param.name.clone());
                    }
                    self.vars.insert(param.name.clone(), cpp_pt);
                    if is_already_fwd {
                        let cpp_t = type_to_cpp(&param.type_node);
                        if param.is_variadic {
                            let base_pack_type =
                                if cpp_t.is_empty() || cpp_t == "any" || cpp_t == "auto" {
                                    "__Args".to_string()
                                } else {
                                    cpp_t
                                        .trim_start_matches('.')
                                        .trim_end_matches('*')
                                        .trim()
                                        .to_string()
                                };
                            param_strs.push(format!("{}... {}", base_pack_type, param.name));
                        } else {
                            param_strs.push(format!("{} {}", cpp_t, param.name));
                        }
                    } else {
                        param_strs.push(self.format_param(param));
                    }
                }

                let safe_name_str = cpp_safe_name(name);
                let safe_name = safe_name_str.as_str();

                if *is_abstract {
                    self.emit(&format!(
                        "virtual {} {}({}) = 0;",
                        ret_type_str,
                        safe_name,
                        param_strs.join(", ")
                    ));
                    return;
                }

                if body.is_empty() {
                    return;
                }

                let virtual_prefix = if *is_virtual { "virtual " } else { "" };
                let static_prefix = if self.in_static_block { "static " } else { "" };
                let inline_prefix =
                    if !self.in_static_block && !self.in_class_or_scope && safe_name != "main" && !*is_virtual {
                        "inline "
                    } else {
                        ""
                    };
                let prev_fn_name = self.current_fn_name.clone();
                self.current_fn_name = Some(name.clone());

                if safe_name == "main" {
                    self.emit("int main(int fl_argc, char* fl_argv[]) {");
                } else {
                    self.emit(&format!(
                        "{}{}{}{} {}({}) {{",
                        static_prefix,
                        inline_prefix,
                        virtual_prefix,
                        ret_type_str,
                        safe_name,
                        param_strs.join(", ")
                    ));
                }

                let was_static = self.in_static_block;
                self.in_static_block = false;

                self.indent_level += 1;
                if !self.in_class_or_scope {
                    if stmts_use_flag(body, &Flag::Broken) {
                        self.emit("bool broken = false;");
                    }
                    if stmts_use_flag(body, &Flag::IsDone) {
                        self.emit("bool is_done = false;");
                    }
                    if stmts_use_flag(body, &Flag::Yielded) {
                        self.emit("bool yielded = false;");
                    }
                    if stmts_use_flag(body, &Flag::Leaved) {
                        self.emit("bool leaved = false;");
                    }
                    if stmts_use_flag(body, &Flag::Returned) {
                        self.emit("bool returned = false;");
                    }
                    if stmts_use_flag(body, &Flag::Continued) {
                        self.emit("bool continued = false;");
                    }
                }

                let error_handle = if let Some(handles) = self.function_handles.get(name) {
                    handles
                        .iter()
                        .find(|h| {
                            if let Decl::FnDecl { name: h_name, .. } = h {
                                h_name == "error" || h_name == "throw" || h_name == "has_error"
                            } else {
                                false
                            }
                        })
                        .cloned()
                } else {
                    None
                };

                if let Some(Decl::FnDecl {
                    params: err_params,
                    body: err_body,
                    ..
                }) = error_handle
                {
                    self.emit("try {");
                    self.indent_level += 1;
                    for s in body {
                        self.visit_statement(s);
                    }
                    self.indent_level -= 1;
                    let catch_type = if !err_params.is_empty() {
                        let p_type = type_to_cpp(&err_params[0].type_node);
                        if p_type == "Error" || p_type == "fast_std::Error" {
                            "const fast_std::Error&".to_string()
                        } else {
                            format!("const {}&", p_type)
                        }
                    } else {
                        "const fast_std::Error&".to_string()
                    };
                    let catch_param = if !err_params.is_empty() {
                        err_params[0].name.clone()
                    } else {
                        "__e".to_string()
                    };
                    self.emit(&format!("}} catch ({} {}) {{", catch_type, catch_param));
                    self.indent_level += 1;
                    for s in &err_body {
                        self.visit_statement(s);
                    }
                    self.indent_level -= 1;
                    self.emit("}");
                } else {
                    if safe_name == "main" {
                        self.emit("try {");
                        self.indent_level += 1;
                        if params.len() == 1 {
                            let p_name = &params[0].name;
                            let cpp_p_type = type_to_cpp(&params[0].type_node);
                            self.emit(&format!(
                                "size_t __argc_count = (fl_argc > 1) ? (size_t)(fl_argc - 1) : 0;\n{} {} = fastlang_array_alloc<fast_std::array<char>>(__argc_count);\nfor (size_t __i = 1; __i < (size_t)fl_argc; ++__i) {{\n    {}[__i - 1] = fastlang_string_create_from_ptr(fl_argv[__i]);\n}}",
                                cpp_p_type, p_name, p_name
                            ));
                        } else if params.len() == 2 {
                            let p0_name = &params[0].name;
                            let p1_name = &params[1].name;
                            let cpp_p1_type = type_to_cpp(&params[1].type_node);
                            self.emit(&format!(
                                "int32_t {} = fl_argc;\nsize_t __argc_count = (fl_argc > 0) ? (size_t)fl_argc : 0;\n{} {} = fastlang_array_alloc<fast_std::array<char>>(__argc_count);\nfor (size_t __i = 0; __i < (size_t)fl_argc; ++__i) {{\n    {}[__i] = fastlang_string_create_from_ptr(fl_argv[__i]);\n}}",
                                p0_name, cpp_p1_type, p1_name, p1_name
                            ));
                        }
                        for s in body {
                            self.visit_statement(s);
                        }
                        if !body
                            .last()
                            .map(|s| matches!(s, Stmt::ReturnStmt(_)))
                            .unwrap_or(false)
                        {
                            self.emit("return 0;");
                        }
                        self.indent_level -= 1;
                        self.emit("} catch (const fast_std::Error& __e) {");
                        self.indent_level += 1;
                        self.emit("std::fprintf(stderr, \"\\n[FastLang Runtime Panic] Uncaught Error: %s\\n\", fastlang_as_str(__e));");
                        self.emit("return 1;");
                        self.indent_level -= 1;
                        self.emit("} catch (const std::exception& __e) {");
                        self.indent_level += 1;
                        self.emit("std::fprintf(stderr, \"\\n[FastLang Runtime Panic] Fatal Exception: %s\\n\", __e.what());");
                        self.emit("return 1;");
                        self.indent_level -= 1;
                        self.emit("}");
                    } else {
                        for s in body {
                            self.visit_statement(s);
                        }
                        if ret_type_str != "void"
                            && !body
                                .last()
                                .map(|s| matches!(s, Stmt::ReturnStmt(_)))
                                .unwrap_or(false)
                        {
                            self.emit("return {};");
                        }
                    }
                }
                self.indent_level -= 1;

                self.emit("}");
                self.in_static_block = was_static;
                self.current_fn_name = prev_fn_name;
                for v in added_variadics {
                    self.variadic_packs.remove(&v);
                }
                self.vars = prev_vars;
                self.raw_pointer_vars = prev_raw_pointers;
                self.pointer_vars = prev_pointers;
                self.address_vars = prev_address_vars;
            }
            Decl::LabelDecl { name, body, .. } => {
                self.emit(&format!("void {}() {{", name));
                self.indent_level += 1;
                for s in body {
                    self.visit_statement(s);
                }
                self.indent_level -= 1;
                self.emit("}");
            }
            Decl::BlockDecl {
                visibility,
                name,
                return_type,
                statements,
                ..
            } => {
                if *visibility == Visibility::Public {
                    self.emit(&format!("namespace {} {{", name));
                    self.indent_level += 1;
                    for s in statements {
                        self.visit_statement(s);
                    }
                    self.indent_level -= 1;
                    self.emit("}");
                    return;
                }
                let has_yield = stmts_use_flag(statements, &Flag::HasYield);
                let ret_cpp = match return_type {
                    Some(t) => type_to_cpp(t),
                    None => "void".to_string(),
                };
                self.emit(&format!("struct __block_type_{} {{", name));
                self.indent_level += 1;
                self.emit("int32_t __state = 0;");
                self.emit("bool has_yielded = false;");
                self.emit("bool is_done = false;");
                self.emit("bool has_error = false;");
                let mut block_vars: Vec<(String, BaseType)> = Vec::new();
                if has_yield {
                    collect_block_vars(statements, &mut block_vars);
                    for (f_name, type_node) in &block_vars {
                        let cpp_t = type_to_cpp(type_node);
                        self.emit(&format!("{} {};", cpp_t, f_name));
                    }
                } else {
                    for s in statements {
                        if let Stmt::Declaration(Decl::VarDecl {
                            name: f_name,
                            type_node,
                            ..
                        }) = s
                        {
                            let cpp_t = type_to_cpp(type_node);
                            self.emit(&format!("{} {};", cpp_t, f_name));
                        }
                    }
                }
                for s in statements {
                    if matches!(s, Stmt::Declaration(Decl::FnDecl { .. })) {
                        self.visit_statement(s);
                    }
                }
                self.emit(&format!("{} operator()() {{", ret_cpp));
                self.indent_level += 1;
                if has_yield {
                    let var_set: std::collections::HashSet<String> =
                        block_vars.into_iter().map(|(n, _)| n).collect();
                    self.current_block_vars = Some(var_set);
                    self.emit("switch(this->__state) {");
                    self.emit("case 0:");
                    self.indent_level += 1;
                }
                for s in statements {
                    match s {
                        Stmt::Declaration(Decl::FnDecl { .. }) => {
                            // Already emitted outside operator()()
                        }
                        _ => {
                            self.visit_statement(s);
                        }
                    }
                }
                if has_yield {
                    self.current_block_vars = None;
                    self.indent_level -= 1;
                    self.emit("}");
                    self.emit("this->has_yielded = false;");
                    self.emit("this->is_done = true;");
                }
                if return_type.as_ref().is_some_and(|t| *t != BaseType::Void) {
                    self.emit("return {};");
                }
                self.indent_level -= 1;
                self.emit("}");
                self.indent_level -= 1;
                self.emit(&format!("}} {};", name));
            }
            Decl::ExternFnDecl { name, alias, .. } => {
                if let Some(alias_name) = alias {
                    self.emit(&format!("#define {} {}", alias_name, name));
                }
            }
            Decl::ExternBlockDecl { decls, .. } => {
                for d in decls {
                    if let Decl::ExternFnDecl {
                        name,
                        alias: Some(alias_name),
                        ..
                    } = d
                    {
                        self.emit(&format!("#define {} {}", alias_name, name));
                    }
                }
            }
            Decl::DefineDecl {
                name,
                type_alias,
                value,
                ..
            } => {
                if let Some(t) = type_alias {
                    let cpp_t = type_to_cpp(t);
                    self.emit(&format!("using {} = {};", name, cpp_t));
                } else if let Some(expr) = value {
                    let val_str = self.visit_expression(expr);
                    self.emit(&format!("constexpr auto {} = {};", name, val_str));
                }
            }
            Decl::Import {
                module_path, abi, ..
            } => {
                if let Some(_abi_str) = abi {
                    if let Some(header) = module_path.first() {
                        if header.ends_with(".h")
                            || header.ends_with(".hpp")
                            || !header.contains('/')
                        {
                            let inc = if header.starts_with('<') || header.starts_with('"') {
                                format!("#include {}", header)
                            } else {
                                format!("#include <{}>", header)
                            };
                            self.c_includes.insert(inc);
                        }
                    }
                } else if let Some(header) = module_path.first() {
                    if header.ends_with(".h") || header.ends_with(".hpp") {
                        let inc = if header.starts_with('<') || header.starts_with('"') {
                            format!("#include {}", header)
                        } else {
                            format!("#include <{}>", header)
                        };
                        self.c_includes.insert(inc);
                    }
                }
            }

            Decl::ImplDecl { .. } => {
                // Handled during Blueprint / Scope generation
            }
            Decl::CompileDecl { .. } => {
                // Compile-time only constructs
            }
            Decl::NamespaceDecl { name, decls, .. } => {
                self.emit(&format!("namespace {} {{", name));
                self.indent_level += 1;
                for d in decls {
                    self.visit_declaration(d);
                }
                self.indent_level -= 1;
                self.emit("}");
            }
            Decl::MacroDecl { .. } => {
                // Macros are pure compile-time AST replacements and expansions.
                // Never emit standalone C++ functions for them.
            }
            _ => {
                self.emit(&format!("// TODO: unimplemented declaration {:?}", decl));
            }
        }
    }
}
