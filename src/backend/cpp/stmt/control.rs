use std::mem::{replace, take};

use crate::backend::cpp::generator::CodeGenerator;
use crate::backend::cpp::stmt::types::*;
use crate::frontend::parser::ast::*;

impl CodeGenerator {
    pub(crate) fn format_param(&mut self, param: &Param) -> String {
        let cpp_t = type_to_cpp(&param.type_node);
        if param.is_variadic {
            let base_pack_type = if cpp_t.is_empty() || cpp_t == "any" || cpp_t == "auto" {
                "__Args".to_string()
            } else {
                cpp_t
                    .trim_start_matches('.')
                    .trim_end_matches('*')
                    .trim()
                    .to_string()
            };
            return format!("{}... {}", base_pack_type, param.name);
        }
        if let Some(ref def_val) = param.default_value {
            let def_code = self.visit_expression(def_val);
            format!("{} {} = {}", cpp_t, param.name, def_code)
        } else {
            format!("{} {}", cpp_t, param.name)
        }
    }

    pub(crate) fn visit_statement(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Block(stmts) | Stmt::ThisBlock(stmts) => {
                for s in stmts {
                    self.visit_statement(s);
                }
            }
            Stmt::Declaration(decl) => {
                self.visit_declaration(decl);
            }
            Stmt::CompileValidation { .. } => return,
            Stmt::ExpressionStmt(expr) => {
                if let Expr::Call { callee, .. } = expr {
                    if let Expr::NamespaceAccess { namespace, .. } = &**callee {
                        if namespace == "@compile" {
                            return;
                        }
                    }
                }
                let code = self.visit_expression(expr);
                self.emit(&format!("{};", code));
            }
            Stmt::ReassignStmt { target, value, op } => {
                let target_code = self.visit_expression(target);
                let val_code = self.visit_expression(value);

                let target_type = if let Expr::Identifier(var_name) = target {
                    self.vars.get(var_name).cloned()
                } else if let Expr::PropertyAccess { object, property } = target {
                    if matches!(**object, Expr::This) {
                        self.current_class_name.as_ref().and_then(|cls| {
                            self.struct_field_types
                                .get(cls)
                                .and_then(|fields| fields.get(property).cloned())
                        })
                    } else if let Expr::Identifier(obj_name) = &**object {
                        self.vars.get(obj_name).and_then(|obj_ty| {
                            let base = Self::clean_type_name(obj_ty);
                            self.struct_field_types
                                .get(&base)
                                .and_then(|fields| fields.get(property).cloned())
                        })
                    } else {
                        None
                    }
                } else {
                    None
                };

                if op == "->" {
                    if target_type
                        .as_deref()
                        .and_then(|t| self.find_assign_handle(t, &["arrow"]))
                        .is_some()
                    {
                        self.emit(&format!("{}.arrow({});", target_code, val_code));
                    } else if val_code.starts_with('{') && val_code.ends_with('}') {
                        self.emit(&format!("{}.arrow({});", target_code, val_code));
                    } else {
                        self.emit(&format!("{} = {};", target_code, val_code));
                    }
                } else if op == "=>" {
                    self.emit(&format!("{}.fat_arrow({});", target_code, val_code));
                } else if op == "=" {
                    self.emit(&format!("{} = {};", target_code, val_code));
                } else if op == "+=" {
                    if let Some(_t) = self.match_primitive_target(target, "add_reassign") {
                        self.emit(&format!("{}.add_reassign({});", target_code, val_code));
                    } else if let Some(t) = self.match_primitive_target(target, "add") {
                        self.emit(&format!(
                            "{} = fastlang_{}_add({}, {});",
                            target_code, t, target_code, val_code
                        ));
                    } else {
                        self.emit(&format!("{} += {};", target_code, val_code));
                    }
                } else {
                    self.emit(&format!("{} {} {};", target_code, op, val_code));
                }
            }
            Stmt::IfStmt {
                condition,
                then_block,
                else_block,
            } => {
                let cond_code = self.visit_expression(condition);
                self.emit(&format!("if ({}) {{", cond_code));
                self.indent_level += 1;
                for s in then_block {
                    self.visit_statement(s);
                }
                self.indent_level -= 1;
                if let Some(eb) = else_block {
                    if !crate::middle_end::semantic::analyzer::has_compile_directive(eb) {
                        self.emit("} else {");
                        self.indent_level += 1;
                        for s in eb {
                            self.visit_statement(s);
                        }
                        self.indent_level -= 1;
                    }
                }
                self.emit("}");
            }
            Stmt::ForInStmt {
                item,
                iterable,
                body,
            } => {
                let iterable_code = self.visit_expression(iterable);
                let item_code = if let Stmt::Declaration(Decl::VarDecl {
                    type_node, name, ..
                }) = &**item
                {
                    let cpp_type = type_to_cpp(type_node);
                    format!("{} {}", cpp_type, name)
                } else if let Stmt::ExpressionStmt(Expr::Identifier(name)) = &**item {
                    format!("auto {}", name)
                } else {
                    "auto item".to_string()
                };

                if self.variadic_packs.contains(&iterable_code) {
                    let item_var = item_code
                        .split_whitespace()
                        .last()
                        .unwrap_or(&item_code)
                        .trim_start_matches('*')
                        .trim_start_matches('&');
                    self.emit(&format!("([&](auto&& {}) {{", item_var));
                    self.indent_level += 1;
                    match body {
                        EitherBlock::Inline(stmts) => {
                            for stmt in stmts {
                                self.visit_statement(stmt);
                            }
                        }
                        EitherBlock::External(expr) => {
                            let expr_code = self.visit_expression(expr);
                            self.emit(&format!("{};", expr_code));
                        }
                    }
                    self.indent_level -= 1;
                    self.emit(&format!("}}({}), ...);", iterable_code));
                } else {
                    let it_idx = self.iter_counter;
                    self.iter_counter += 1;
                    self.emit("{");
                    self.indent_level += 1;
                    self.emit(&format!("auto&& __col_{} = ({});", it_idx, iterable_code));
                    self.emit(&format!(
                        "auto __it_{} = fastlang_get_iter(__col_{});",
                        it_idx, it_idx
                    ));
                    self.emit(&format!("while (!__it_{}.is_done_iter()) {{", it_idx));
                    self.indent_level += 1;
                    self.emit(&format!("{} = __it_{}.next();", item_code, it_idx));
                    match body {
                        EitherBlock::Inline(stmts) => {
                            for stmt in stmts {
                                self.visit_statement(stmt);
                            }
                        }
                        EitherBlock::External(expr) => {
                            let expr_code = self.visit_expression(expr);
                            self.emit(&format!("{};", expr_code));
                        }
                    }
                    self.indent_level -= 1;
                    self.emit("}");
                    self.indent_level -= 1;
                    self.emit("}");
                }
            }
            Stmt::WhileStmt { condition, body } => {
                let cond_code = self.visit_expression(condition);
                self.emit(&format!("while ({}) {{", cond_code));
                self.indent_level += 1;
                match body {
                    EitherBlock::Inline(stmts) => {
                        for s in stmts {
                            self.visit_statement(s);
                        }
                    }
                    EitherBlock::External(expr) => {
                        let expr_code = self.visit_expression(expr);
                        self.emit(&format!("{};", expr_code));
                    }
                }
                self.indent_level -= 1;
                self.emit("}");
            }
            Stmt::DoWhileStmt { body, condition } => {
                self.emit("do {");
                self.indent_level += 1;
                match body {
                    EitherBlock::Inline(stmts) => {
                        for s in stmts {
                            self.visit_statement(s);
                        }
                    }
                    EitherBlock::External(expr) => {
                        let expr_code = self.visit_expression(expr);
                        self.emit(&format!("{};", expr_code));
                    }
                }
                self.indent_level -= 1;
                let cond_code = self.visit_expression(condition);
                self.emit(&format!("}} while ({});", cond_code));
            }
            Stmt::LoopStmt { count, body } => {
                if let Some(c) = count {
                    let count_code = self.visit_expression(c);
                    self.emit(&format!(
                        "for (long long _i = 0, _limit = (long long)({}); _i < _limit; ++_i) {{",
                        count_code
                    ));
                } else {
                    self.emit("while (true) {");
                }
                self.indent_level += 1;
                match body {
                    EitherBlock::Inline(stmts) => {
                        for s in stmts {
                            self.visit_statement(s);
                        }
                    }
                    EitherBlock::External(expr) => {
                        let expr_code = self.visit_expression(expr);
                        self.emit(&format!("{};", expr_code));
                    }
                }
                self.indent_level -= 1;
                self.emit("}");
            }
            Stmt::SwitchStmt {
                condition, cases, ..
            } => {
                let cond_code = self.visit_expression(condition);
                let cond_val = if cond_code == "this" {
                    "(*this)".to_string()
                } else {
                    cond_code
                };
                self.emit("{");
                self.indent_level += 1;
                self.emit(&format!("auto&& __match_val = {};", cond_val));

                let mut first = true;
                for s in cases {
                    if let Stmt::CaseStmt { option, body, .. } = s {
                        if matches!(option, Expr::Identifier(name) if name == "void") {
                            if first {
                                self.emit("{");
                            } else {
                                self.emit("else {");
                            }
                            self.indent_level += 1;
                            for case_stmt in body {
                                self.visit_statement(case_stmt);
                            }
                            self.indent_level -= 1;
                            self.emit("}");
                        } else {
                            let branch_prefix = if first { "if" } else { "else if" };
                            match option {
                                Expr::Call { callee, args, .. } => {
                                    let callee_code = self.visit_expression(callee);
                                    let raw_variant = if let Some(idx) = callee_code.rfind("::") {
                                        &callee_code[idx + 2..]
                                    } else {
                                        &callee_code
                                    };
                                    let variant_name = raw_variant.trim_end_matches("()");
                                    self.emit(
                                        &format!(
                                            "{} (__match_val.tag == std::decay_t<decltype(__match_val)>::Tag::{}) {{",
                                            branch_prefix,
                                            variant_name
                                        )
                                    );
                                    self.indent_level += 1;
                                    if !args.is_empty() {
                                        let var_names: Vec<String> = args
                                            .iter()
                                            .filter_map(|a| {
                                                if let Expr::Identifier(n) = a {
                                                    Some(n.clone())
                                                } else {
                                                    None
                                                }
                                            })
                                            .collect();
                                        if !var_names.is_empty() {
                                            let mut unused_counter = 0;
                                            let cpp_var_names: Vec<String> = var_names
                                                .iter()
                                                .map(|vn| {
                                                    if vn == "_" {
                                                        unused_counter += 1;
                                                        format!("__unused_{}", unused_counter)
                                                    } else {
                                                        vn.clone()
                                                    }
                                                })
                                                .collect();
                                            let cond_type_opt =
                                                self.extract_cpp_type_from_expr(condition);
                                            let generic_args = cond_type_opt
                                                .as_deref()
                                                .map(extract_generic_type_args)
                                                .unwrap_or_default();
                                            for (i, vn) in var_names.iter().enumerate() {
                                                if vn != "_" {
                                                    self.pointer_vars.insert(vn.clone());
                                                    if let Some(arg_type) = generic_args.get(i) {
                                                        self.vars
                                                            .insert(vn.clone(), arg_type.clone());
                                                        if arg_type.ends_with('*')
                                                            || arg_type.starts_with("raw_ptr<")
                                                        {
                                                            self.raw_pointer_vars
                                                                .insert(vn.clone());
                                                        }
                                                    }
                                                }
                                            }
                                            self.emit(&format!(
                                                "auto&& [{}] = __match_val.storage.v_{};",
                                                cpp_var_names.join(", "),
                                                variant_name
                                            ));
                                        }
                                    }
                                    for case_stmt in body {
                                        self.visit_statement(case_stmt);
                                    }
                                    self.indent_level -= 1;
                                    self.emit("}");
                                }
                                Expr::Instantiate { target, args } => {
                                    let target_code = self.visit_expression(target);
                                    let raw_variant = if let Some(idx) = target_code.rfind("::") {
                                        &target_code[idx + 2..]
                                    } else {
                                        &target_code
                                    };
                                    let variant_name = raw_variant.trim_end_matches("()");
                                    self.emit(
                                        &format!(
                                            "{} (__match_val.tag == std::decay_t<decltype(__match_val)>::Tag::{}) {{",
                                            branch_prefix,
                                            variant_name
                                        )
                                    );
                                    self.indent_level += 1;
                                    if !args.is_empty() {
                                        if let Expr::ObjectLiteral(stmts) = &args[0] {
                                            let var_names: Vec<String> = stmts
                                                .iter()
                                                .filter_map(|s| match s {
                                                    Stmt::Declaration(Decl::VarDecl {
                                                        name,
                                                        ..
                                                    }) => Some(name.clone()),
                                                    Stmt::ReassignStmt { target, .. } => {
                                                        if let Expr::Identifier(n) = target {
                                                            Some(n.clone())
                                                        } else {
                                                            None
                                                        }
                                                    }
                                                    Stmt::ExpressionStmt(Expr::Identifier(n)) => {
                                                        Some(n.clone())
                                                    }
                                                    _ => None,
                                                })
                                                .collect();
                                            if !var_names.is_empty() {
                                                let cond_type_opt =
                                                    self.extract_cpp_type_from_expr(condition);
                                                let generic_args = cond_type_opt
                                                    .as_deref()
                                                    .map(extract_generic_type_args)
                                                    .unwrap_or_default();
                                                for (i, vn) in var_names.iter().enumerate() {
                                                    if vn != "_" {
                                                        self.pointer_vars.insert(vn.clone());
                                                        if let Some(arg_type) = generic_args.get(i)
                                                        {
                                                            self.vars.insert(
                                                                vn.clone(),
                                                                arg_type.clone(),
                                                            );
                                                            if arg_type.ends_with('*')
                                                                || arg_type.starts_with("raw_ptr<")
                                                            {
                                                                self.raw_pointer_vars
                                                                    .insert(vn.clone());
                                                            }
                                                        }
                                                    }
                                                }
                                                self.emit(&format!(
                                                    "auto&& [{}] = __match_val.storage.v_{};",
                                                    var_names.join(", "),
                                                    variant_name
                                                ));
                                            }
                                        }
                                    }
                                    for case_stmt in body {
                                        self.visit_statement(case_stmt);
                                    }
                                    self.indent_level -= 1;
                                    self.emit("}");
                                }
                                Expr::NamespaceAccess {
                                    namespace,
                                    property,
                                } => {
                                    let prop_code = self.visit_expression(property);
                                    let variant_name = prop_code.trim_end_matches("()");
                                    if self.simple_enum_types.contains(namespace) {
                                        self.emit(&format!(
                                            "{} (__match_val == {}::{}) {{",
                                            branch_prefix, namespace, variant_name
                                        ));
                                    } else {
                                        self.emit(
                                            &format!(
                                                "{} (__match_val.tag == std::decay_t<decltype(__match_val)>::Tag::{}) {{",
                                                branch_prefix,
                                                variant_name
                                            )
                                        );
                                    }
                                    self.indent_level += 1;
                                    for case_stmt in body {
                                        self.visit_statement(case_stmt);
                                    }
                                    self.indent_level -= 1;
                                    self.emit("}");
                                }
                                Expr::PropertyAccess { object, property } => {
                                    let obj_code = self.visit_expression(object);
                                    if self.simple_enum_types.contains(&obj_code) {
                                        self.emit(&format!(
                                            "{} (__match_val == {}::{}) {{",
                                            branch_prefix, obj_code, property
                                        ));
                                    } else {
                                        self.emit(
                                            &format!(
                                                "{} (__match_val.tag == std::decay_t<decltype(__match_val)>::Tag::{}) {{",
                                                branch_prefix,
                                                property
                                            )
                                        );
                                    }
                                    self.indent_level += 1;
                                    for case_stmt in body {
                                        self.visit_statement(case_stmt);
                                    }
                                    self.indent_level -= 1;
                                    self.emit("}");
                                }
                                Expr::BinaryOp { operator, .. } if operator == "|" => {
                                    fn flatten_pipe<'a>(e: &'a Expr, out: &mut Vec<&'a Expr>) {
                                        if let Expr::BinaryOp {
                                            left,
                                            operator,
                                            right,
                                        } = e
                                        {
                                            if operator == "|" {
                                                flatten_pipe(left, out);
                                                flatten_pipe(right, out);
                                                return;
                                            }
                                        }
                                        out.push(e);
                                    }
                                    let mut patterns = Vec::new();
                                    flatten_pipe(option, &mut patterns);

                                    let mut cond_strs = Vec::new();
                                    for pat in patterns {
                                        match pat {
                                            Expr::NamespaceAccess {
                                                namespace,
                                                property,
                                            } => {
                                                let prop_code = self.visit_expression(property);
                                                let variant_name = prop_code.trim_end_matches("()");
                                                if self.simple_enum_types.contains(namespace) {
                                                    cond_strs.push(format!(
                                                        "(__match_val == {}::{})",
                                                        namespace, variant_name
                                                    ));
                                                } else {
                                                    cond_strs.push(format!("(__match_val.tag == std::decay_t<decltype(__match_val)>::Tag::{})", variant_name));
                                                }
                                            }
                                            Expr::PropertyAccess { object, property } => {
                                                let obj_code = self.visit_expression(object);
                                                if self.simple_enum_types.contains(&obj_code) {
                                                    cond_strs.push(format!(
                                                        "(__match_val == {}::{})",
                                                        obj_code, property
                                                    ));
                                                } else {
                                                    cond_strs.push(format!("(__match_val.tag == std::decay_t<decltype(__match_val)>::Tag::{})", property));
                                                }
                                            }
                                            Expr::Identifier(ident) => {
                                                cond_strs.push(format!(
                                                    "fastlang_match_eq(__match_val, {})",
                                                    ident
                                                ));
                                            }
                                            _ => {
                                                let val_code = self.visit_expression(pat);
                                                cond_strs.push(format!(
                                                    "fastlang_match_eq(__match_val, {})",
                                                    val_code
                                                ));
                                            }
                                        }
                                    }
                                    self.emit(&format!(
                                        "{} ({}) {{",
                                        branch_prefix,
                                        cond_strs.join(" || ")
                                    ));
                                    self.indent_level += 1;
                                    for case_stmt in body {
                                        self.visit_statement(case_stmt);
                                    }
                                    self.indent_level -= 1;
                                    self.emit("}");
                                }
                                Expr::Identifier(ident) => {
                                    self.emit(&format!(
                                        "{} (fastlang_match_eq(__match_val, {})) {{",
                                        branch_prefix, ident
                                    ));
                                    self.indent_level += 1;
                                    for case_stmt in body {
                                        self.visit_statement(case_stmt);
                                    }
                                    self.indent_level -= 1;
                                    self.emit("}");
                                }
                                _ => {
                                    let val_code = self.visit_expression(option);
                                    self.emit(&format!(
                                        "{} (fastlang_match_eq(__match_val, {})) {{",
                                        branch_prefix, val_code
                                    ));
                                    self.indent_level += 1;
                                    for case_stmt in body {
                                        self.visit_statement(case_stmt);
                                    }
                                    self.indent_level -= 1;
                                    self.emit("}");
                                }
                            }
                        }
                        first = false;
                    }
                }
                self.indent_level -= 1;
                self.emit("}");
            }
            Stmt::DelStmt { target, is_array } => {
                let expr_code = self.visit_expression(target);
                if *is_array {
                    self.emit(&format!("delete[] {};", expr_code));
                } else {
                    let target_type = if let Expr::Identifier(var_name) = target {
                        self.vars.get(var_name).cloned()
                    } else if let Expr::PropertyAccess { object, property } = target {
                        if matches!(**object, Expr::This) {
                            self.current_class_name.as_ref().and_then(|cls| {
                                self.struct_field_types
                                    .get(cls)
                                    .and_then(|fields| fields.get(property).cloned())
                            })
                        } else if let Expr::Identifier(obj_name) = &**object {
                            self.vars.get(obj_name).and_then(|obj_ty| {
                                let base = Self::clean_type_name(obj_ty);
                                self.struct_field_types
                                    .get(&base)
                                    .and_then(|fields| fields.get(property).cloned())
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    let is_raw_ptr = match target {
                        Expr::Identifier(name) => self.raw_pointer_vars.contains(name),
                        _ => false,
                    };
                    let is_ptr = is_raw_ptr
                        || target_type
                            .as_deref()
                            .map_or(false, |t| t.ends_with('*') || t.starts_with("raw_ptr"));
                    let is_arr = target_type
                        .as_deref()
                        .map_or(false, |t| t.contains("array") || t.ends_with("[]"));
                    let clean_t = target_type.as_deref().map(|t| Self::clean_type_name(t));
                    let has_drop = clean_t
                        .as_deref()
                        .map_or(false, |t| self.types_with_drop.contains(t));

                    if is_ptr {
                        self.emit(&format!("delete {};", expr_code));
                    } else if is_arr {
                        // Array is managed via value or its backing buffer
                    } else if has_drop {
                        self.emit(&format!("{}.drop();", expr_code));
                    }
                }
            }
            Stmt::UsingStmt(name) => {
                self.used_enums.insert(name.clone());
                if self.is_known_namespace(name) {
                    self.emit(&format!("using namespace {};", name));
                }
            }
            Stmt::SetNamespace(_) => {}
            Stmt::ForStmt {
                init,
                condition,
                increment,
                body,
            } => {
                let init_code = if let Some(i) = init {
                    let old_out = take(&mut self.output);
                    self.visit_statement(i);
                    let s = replace(&mut self.output, old_out);
                    s.trim().trim_end_matches(';').to_string()
                } else {
                    "".to_string()
                };

                let cond_code = if let Some(c) = condition {
                    self.visit_expression(c)
                } else {
                    "".to_string()
                };

                let inc_code = if let Some(inc) = increment {
                    let old_out = take(&mut self.output);
                    self.visit_statement(inc);
                    let s = replace(&mut self.output, old_out);
                    s.trim().trim_end_matches(';').to_string()
                } else {
                    "".to_string()
                };

                self.emit(&format!(
                    "for ({}; {}; {}) {{",
                    init_code, cond_code, inc_code
                ));
                self.indent_level += 1;
                match body {
                    EitherBlock::Inline(stmts) => {
                        for s in stmts {
                            self.visit_statement(s);
                        }
                    }
                    EitherBlock::External(expr) => {
                        let expr_code = self.visit_expression(expr);
                        self.emit(&format!("{};", expr_code));
                    }
                }
                self.indent_level -= 1;
                self.emit("}");
            }
            Stmt::ForIn {
                item_decl,
                iterable,
                body,
            } => {
                let old_out = take(&mut self.output);
                self.visit_statement(item_decl);
                let mut decl_code = replace(&mut self.output, old_out);
                decl_code = decl_code.trim_end().trim_end_matches(';').to_string();

                let iter_code = self.visit_expression(iterable);
                if self.variadic_packs.contains(&iter_code) {
                    let item_var = decl_code
                        .split_whitespace()
                        .last()
                        .unwrap_or(&decl_code)
                        .trim_start_matches('*')
                        .trim_start_matches('&');
                    self.emit(&format!("([&](auto&& {}) {{", item_var));
                    self.indent_level += 1;
                    for s in body {
                        self.visit_statement(s);
                    }
                    self.indent_level -= 1;
                    self.emit(&format!("}}({}), ...);", iter_code));
                } else {
                    let it_idx = self.iter_counter;
                    self.iter_counter += 1;
                    self.emit("{");
                    self.indent_level += 1;
                    self.emit(&format!("auto&& __col_{} = ({});", it_idx, iter_code));
                    self.emit(&format!(
                        "auto __it_{} = fastlang_get_iter(__col_{});",
                        it_idx, it_idx
                    ));
                    self.emit(&format!("while (!__it_{}.is_done_iter()) {{", it_idx));
                    self.indent_level += 1;
                    self.emit(&format!("{} = __it_{}.next();", decl_code, it_idx));
                    for s in body {
                        self.visit_statement(s);
                    }
                    self.indent_level -= 1;
                    self.emit("}");
                    self.indent_level -= 1;
                    self.emit("}");
                }
            }
            Stmt::ReturnStmt(expr) => {
                if expr.is_none() || matches!(expr, Some(Expr::LiteralVoid)) {
                    if self.current_fn_name.as_deref() == Some("main") {
                        self.emit("return 0;");
                    } else {
                        self.emit("return;");
                    }
                } else {
                    let expr_code = self.visit_expression(&expr.clone().unwrap());
                    if expr_code == "this" {
                        self.emit("return *this;");
                    } else {
                        self.emit(&format!("return {};", expr_code));
                    }
                }
            }
            Stmt::BreakStmt => {
                self.emit("break;");
            }
            Stmt::LeaveStmt => {
                self.emit("this->__state = -1;");
                self.emit("return leave();");
            }
            Stmt::YieldStmt(expr) => {
                self.yield_counter += 1;
                let yid = self.yield_counter;
                self.emit("this->has_yielded = true;");
                self.emit(&format!("this->__state = {};", yid));
                if let Some(e) = expr {
                    let expr_code = self.visit_expression(e);
                    self.emit(&format!("return {};", expr_code));
                } else {
                    self.emit("return;");
                }
                self.emit(&format!("case {}:;", yid));
            }
            Stmt::GotoStmt(expr) => {
                let expr_code = self.visit_expression(expr);
                let safe_name = expr_code.replace("@", "");
                self.emit(&format!("goto {};", safe_name));
            }
            Stmt::CallStmt(expr) => {
                if let Expr::Call { callee, .. } = expr {
                    if let Expr::NamespaceAccess { namespace, .. } = &**callee {
                        if namespace == "@compile" {
                            return;
                        }
                    }
                }
                let expr_code = self.visit_expression(expr);
                self.emit(&format!("{};", expr_code));
            }
            Stmt::ContinueStmt => {
                self.emit("continue;");
            }
            Stmt::ThrowStmt(expr) => {
                let expr_code = self.visit_expression(expr);
                if expr_code.starts_with("new ") {
                    self.emit(&format!("throw *({});", expr_code));
                } else {
                    self.emit(&format!("throw {};", expr_code));
                }
            }
            Stmt::TryCatchStmt { try_block, catches } => {
                self.emit("try {");
                self.indent_level += 1;
                for s in try_block {
                    self.visit_statement(s);
                }
                self.indent_level -= 1;

                let mut has_catch_all = false;
                for c in catches {
                    if let Some(ref pt) = c.param_type {
                        let cpp_t = type_to_cpp(pt);
                        self.emit(&format!("}} catch (const {}& {}) {{", cpp_t, c.param));
                        self.indent_level += 1;
                        for s in &c.body {
                            self.visit_statement(s);
                        }
                        self.indent_level -= 1;
                        self.emit(&format!("}} catch (const {}* __{}_ptr) {{", cpp_t, c.param));
                        self.indent_level += 1;
                        self.emit(&format!(
                            "const {}& {} = *__{}_ptr;",
                            cpp_t, c.param, c.param
                        ));
                        for s in &c.body {
                            self.visit_statement(s);
                        }
                        self.indent_level -= 1;
                    } else {
                        has_catch_all = true;
                        self.emit(&format!("}} catch (const fast_std::Error& {}) {{", c.param));
                        self.indent_level += 1;
                        for s in &c.body {
                            self.visit_statement(s);
                        }
                        self.indent_level -= 1;
                        self.emit(&format!(
                            "}} catch (const fast_std::Error* __{}_ptr) {{",
                            c.param
                        ));
                        self.indent_level += 1;
                        self.emit(&format!(
                            "const fast_std::Error& {} = *__{}_ptr;",
                            c.param, c.param
                        ));
                        for s in &c.body {
                            self.visit_statement(s);
                        }
                        self.indent_level -= 1;
                        self.emit(&format!(
                            "}} catch (const std::exception& __{}_ex) {{",
                            c.param
                        ));
                        self.indent_level += 1;
                        self.emit(&format!(
                            "fast_std::Error {} = fast_std::Error(__{}_ex.what());",
                            c.param, c.param
                        ));
                        for s in &c.body {
                            self.visit_statement(s);
                        }
                        self.indent_level -= 1;
                        self.emit("} catch (...) {");
                        self.indent_level += 1;
                        self.emit(&format!(
                            "fast_std::Error {} = fast_std::Error(\"Unknown runtime exception\");",
                            c.param
                        ));
                        for s in &c.body {
                            self.visit_statement(s);
                        }
                        self.indent_level -= 1;
                    }
                }
                if !has_catch_all {
                    self.emit("} catch (...) {");
                    self.indent_level += 1;
                    self.emit("// Fallback for unhandled exception in typed catch");
                    self.indent_level -= 1;
                }
                self.emit("}");
            }
            _ => {
                self.emit(&format!(
                    "// TODO: unimplemented statement {:?} or something gone wrong",
                    stmt
                ));
            }
        }
    }
}
