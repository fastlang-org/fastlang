use super::*;
use std::collections::HashMap;
use crate::frontend::parser::ast::*;

impl Interpreter {
    pub fn eval_macro(
        &mut self,
        body: &[Stmt],
        params: &[Param],
        args: &[Expr],
    ) -> Result<(), String> {
        // Bind args to params
        for (i, param) in params.iter().enumerate() {
            if param.is_variadic {
                let rest: Vec<Expr> = args[i..].to_vec();
                self.env
                    .define(param.name.clone(), Expr::ArrayLiteral(rest));
                break;
            }
            if let Some(arg) = args.get(i) {
                // Try to evaluate the argument (simple resolution)
                let val = self.eval_expr(arg).unwrap_or_else(|_| arg.clone());
                self.env.define(param.name.clone(), val);
            }
        }

        for stmt in body {
            self.eval_stmt(stmt)?;
        }
        Ok(())
    }

    pub fn eval_method(
        &mut self,
        body: &[Stmt],
        params: &[Param],
        args: &[Expr],
    ) -> Result<Option<Expr>, String> {
        // Bind args to params
        for (i, param) in params.iter().enumerate() {
            if param.is_variadic {
                let rest: Vec<Expr> = args[i..].to_vec();
                self.env
                    .define(param.name.clone(), Expr::ArrayLiteral(rest));
                break;
            }
            if let Some(arg) = args.get(i) {
                let val = self.eval_expr(arg).unwrap_or_else(|_| arg.clone());
                self.env.define(param.name.clone(), val);
            }
        }

        for stmt in body {
            if let Some(ret) = self.eval_stmt(stmt)? {
                return Ok(Some(ret));
            }
        }
        Ok(None)
    }

    pub fn eval_stmt(&mut self, stmt: &Stmt) -> Result<Option<Expr>, String> {
        match stmt {
            Stmt::CompileValidation { body, args } => {
                self.env.push_scope();
                for (i, arg) in args.iter().enumerate() {
                    let val = self.eval_expr(arg).unwrap_or_else(|_| arg.clone());
                    if let Expr::Identifier(id) = arg {
                        self.env.define(id.clone(), val.clone());
                    }
                    self.env.define(format!("arg_{}", i), val);
                }
                let mut res = Ok(None);
                for s in body {
                    if let Err(e) = self.eval_stmt(s) {
                        res = Err(e);
                        break;
                    }
                }
                let mut writebacks = Vec::new();
                for arg in args {
                    if let Expr::Identifier(id) = arg {
                        if let Ok(new_val) = self.env.get(id) {
                            writebacks.push((id.clone(), new_val));
                        }
                    }
                }
                self.env.pop_scope();
                for (id, val) in writebacks {
                    let _ = self.env.assign(&id, val);
                }
                res
            }
            Stmt::ForInStmt {
                item,
                iterable,
                body,
            } => {
                let iter_val = self.eval_expr(iterable)?;
                let elements = match iter_val {
                    Expr::ArrayLiteral(elems) => elems,
                    Expr::Identifier(id) => {
                        if let Some(Expr::ArrayLiteral(elems)) = self.env.get_ref(&id) {
                            elems.clone()
                        } else {
                            Vec::new()
                        }
                    }
                    _ => Vec::new(),
                };

                let var_name = match &**item {
                    Stmt::Declaration(Decl::VarDecl { name, .. }) => Some(name.clone()),
                    Stmt::ExpressionStmt(Expr::Identifier(name)) => Some(name.clone()),
                    _ => None,
                };

                if let Some(var_name) = var_name {
                    for elem in elements {
                        self.env.define(var_name.clone(), elem.clone());
                        let elem_ty = self.get_expr_type(&elem);
                        self.var_types.insert(var_name.clone(), elem_ty);
                        match body {
                            EitherBlock::Inline(stmts) => {
                                for s in stmts {
                                    if let Some(ret) = self.eval_stmt(s)? {
                                        return Ok(Some(ret));
                                    }
                                }
                            }
                            EitherBlock::External(expr) => {
                                self.eval_expr(expr)?;
                            }
                        }
                    }
                }
                Ok(None)
            }
            Stmt::ThrowStmt(expr) => {
                let val = self.eval_expr(expr)?;
                let msg = self.extract_string_from_expr(&val);
                self.fastlang_throw(&msg)?;
                Ok(None)
            }
            Stmt::ReturnStmt(opt_expr) => {
                let val = if let Some(expr) = opt_expr {
                    self.eval_expr(expr)?
                } else {
                    Expr::LiteralVoid
                };
                Ok(Some(val))
            }
            Stmt::Declaration(Decl::VarDecl { name, value, .. }) => {
                let val = self.eval_expr(value)?;
                self.env.define(name.clone(), val);
                Ok(None)
            }
            Stmt::ReassignStmt { target, value, op } => {
                let val = self.eval_expr(value)?;
                if let Expr::Identifier(name) = target {
                    if op == "=" {
                        self.env.assign(name, val)?;
                    } else if let Some(cur) = self.env.get_ref(name) {
                        if let Some(res) = apply_reassign_op(cur, op, &val) {
                            self.env.assign(name, res)?;
                        }
                    }
                }
                Ok(None)
            }
            Stmt::IfStmt {
                condition,
                then_block,
                else_block,
            } => {
                let cond_val = self.eval_expr(condition)?;
                let is_true = match cond_val {
                    Expr::LiteralBool(b) => b,
                    _ => false, // Simplistic
                };

                if is_true {
                    for s in then_block {
                        if let Some(ret) = self.eval_stmt(s)? {
                            return Ok(Some(ret));
                        }
                    }
                } else if let Some(else_branch_stmts) = else_block {
                    for s in else_branch_stmts {
                        if let Some(ret) = self.eval_stmt(s)? {
                            return Ok(Some(ret));
                        }
                    }
                }
                Ok(None)
            }
            Stmt::CallStmt(expr) | Stmt::ExpressionStmt(expr) => {
                self.eval_expr(expr)?;
                Ok(None)
            }
            _ => Ok(None), // Ignore other statements for now
        }
    }


    #[allow(clippy::type_complexity)]
    pub fn try_split_mixed_branches(
        &mut self,
        decl: &Decl,
    ) -> Option<(String, Vec<Param>, Vec<(Option<Expr>, Decl, String)>)> {
        if let Decl::FnDecl {
            name,
            generics,
            params,
            return_type,
            body,
            visibility,
            is_virtual,
            is_abstract,
        } = decl
        {
            if name == "main" || generics.is_empty() || name.contains("_br") {
                return None;
            }
            for (idx, s) in body.iter().enumerate() {
                let mut raw_branches: Vec<(Option<Expr>, Vec<Stmt>)> = Vec::new();

                if let Stmt::IfStmt { condition, .. } = s {
                    if !expr_references_params(condition, params, generics) {
                        let mut cond_clone = condition.clone();
                        if self.fold_expr(&mut cond_clone).is_ok()
                            && matches!(cond_clone, Expr::LiteralBool(_)) {
                                continue;
                            }
                    }
                    if is_compile_condition(condition) {
                        let mut curr = s;
                        while let Stmt::IfStmt {
                            condition: c,
                            then_block: tb,
                            else_block: eb,
                        } = curr
                        {
                            raw_branches.push((Some(c.clone()), tb.clone()));
                            if let Some(else_stmts) = eb {
                                if else_stmts.len() == 1
                                    && matches!(&else_stmts[0], Stmt::IfStmt { .. })
                                {
                                    curr = &else_stmts[0];
                                    continue;
                                } else {
                                    raw_branches.push((None, else_stmts.clone()));
                                    break;
                                }
                            } else {
                                break;
                            }
                        }
                    }
                } else if let Stmt::SwitchStmt {
                    condition, cases, ..
                } = s
                {
                    if is_compile_condition(condition)
                        || cases.iter().any(|c| match c {
                            Stmt::CaseStmt { option, .. } => is_compile_condition(option),
                            _ => false,
                        })
                    {
                        for c in cases {
                            if let Stmt::CaseStmt { option, body, .. } = c {
                                if matches!(option, Expr::Identifier(ref id) if id == "void") {
                                    raw_branches.push((None, body.clone()));
                                } else {
                                    let cond = Expr::BinaryOp {
                                        left: Box::new(condition.clone()),
                                        operator: "==".to_string(),
                                        right: Box::new(option.clone()),
                                    };
                                    raw_branches.push((Some(cond), body.clone()));
                                }
                            }
                        }
                    }
                }

                if !raw_branches.is_empty() {
                    let pre = body[..idx].to_vec();
                    let post = body[idx + 1..].to_vec();
                    let mut branches = Vec::new();

                    for (k, (cond_opt, branch_stmts)) in raw_branches.into_iter().enumerate() {
                        let br_n = format!("{}_br{}", name, k + 1);
                        let mut br_body = pre.clone();
                        br_body.extend(branch_stmts);
                        br_body.extend(post.clone());

                        let br_decl = Decl::FnDecl {
                            name: br_n.clone(),
                            generics: generics.clone(),
                            params: params.clone(),
                            return_type: return_type.clone(),
                            body: br_body,
                            visibility: visibility.clone(),
                            is_virtual: *is_virtual,
                            is_abstract: *is_abstract,
                        };
                        branches.push((cond_opt, br_decl, br_n));
                    }
                    return Some((name.clone(), params.clone(), branches));
                }
            }
        }
        None
    }

    pub fn build_micro_param_map(
        &mut self,
        params: &[Param],
        args: &[Expr],
    ) -> HashMap<String, Expr> {
        let mut map = HashMap::new();
        for (p, arg) in params.iter().zip(args.iter()) {
            let param_ty = p.type_node.as_str();
            let arg_ty = self.get_expr_type(arg);
            let mapped = if param_ty != "unknown"
                && param_ty != "void"
                && arg_ty != "unknown"
                && param_ty != arg_ty
            {
                Expr::Cast {
                    expr: Box::new(arg.clone()),
                    target_type: p.type_node.clone(),
                }
            } else {
                arg.clone()
            };
            map.insert(p.name.clone(), mapped);
        }
        map
    }

    pub fn evaluate_ast(&mut self, stmts: &mut Vec<Stmt>) -> Result<(), String> {
        let mut new_stmts = Vec::with_capacity(stmts.len());
        for mut stmt in stmts.drain(..) {
            match &mut stmt {
                Stmt::CompileValidation { body: _, args } => {
                    if self.in_function_body {
                        // Belongs to the containing function definition.
                        // Do NOT execute or strip at declaration time; callers will evaluate it on each call.
                        new_stmts.push(stmt);
                        continue;
                    }
                    for a in args.iter_mut() {
                        self.fold_expr(a)?;
                    }
                    let all_literals = true;
                    for a in args.iter() {
                        let is_lit = if let Expr::Identifier(id) = a {
                            self.env.get_ref(id).map_or_else(|| is_literal(a), is_literal)
                        } else {
                            is_literal(a)
                        };
                        if !is_lit {
                            return Err(format!(
                                "Compile Error: Argument '{}' in @compile validation cannot be evaluated at compile time. Dynamic runtime values are rejected.",
                                self.format_expr_for_print(a)
                            ));
                        }
                    }
                    if all_literals {
                        self.eval_stmt(&stmt)?;
                    }
                    // Compile-time validation block succeeded! Strip from top-level AST so C++ codegen never sees it.
                    continue;
                }
                Stmt::Declaration(decl) => {
                    if let Decl::MacroDecl {
                        name, params, body, ..
                    } = decl
                    {
                        let clean_m = name.trim_start_matches('$');
                        self.macros
                            .entry(name.clone())
                            .or_default()
                            .push((params.clone(), body.clone()));
                        if clean_m != name {
                            self.macros
                                .entry(clean_m.to_string())
                                .or_default()
                                .push((params.clone(), body.clone()));
                        }
                        // Macros are pure compile-time AST inlined templates.
                        // Strip their declaration from the AST so C++ codegen never generates code for them!
                        continue;
                    }
                    if let Some((orig_name, fn_params, branches)) =
                        self.try_split_mixed_branches(decl)
                    {
                        let branch_pairs: Vec<(Option<Expr>, String)> = branches
                            .iter()
                            .map(|(c, _, n)| (c.clone(), n.clone()))
                            .collect();
                        self.split_functions
                            .insert(orig_name, (fn_params, branch_pairs));
                        for (_, mut br_decl, _) in branches {
                            self.evaluate_decl(&mut br_decl)?;
                            new_stmts.push(Stmt::Declaration(br_decl));
                        }
                    } else {
                        self.evaluate_decl(decl)?;
                        new_stmts.push(stmt);
                    }
                }
                Stmt::ReassignStmt { target, value, op } => {
                    self.fold_expr(target)?;
                    self.fold_expr(value)?;
                    if let Expr::Identifier(id) = target {
                        let new_val = if op == "=" {
                            if is_literal(value) {
                                Some(value.clone())
                            } else {
                                None
                            }
                        } else if let Some(cur) = self.env.get_ref(id) {
                            if is_literal(value) && is_literal(cur) {
                                apply_reassign_op(cur, op, value)
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        if let Some(nv) = new_val {
                            self.env.define(id.clone(), nv);
                        } else {
                            self.env.remove(id);
                        }
                    }
                    new_stmts.push(stmt);
                }
                Stmt::ReturnStmt(opt_expr) => {
                    if let Some(e) = opt_expr {
                        self.fold_expr(e)?;
                    }
                    new_stmts.push(stmt);
                }
                Stmt::IfStmt {
                    condition,
                    then_block,
                    else_block,
                } => {
                    self.fold_expr(condition)?;
                    if let Expr::LiteralBool(b) = *condition {
                        if b {
                            self.evaluate_ast(then_block)?;
                            new_stmts.append(then_block);
                        } else if let Some(eb) = else_block {
                            self.evaluate_ast(eb)?;
                            new_stmts.append(eb);
                        }
                    } else {
                        // Dynamic condition!
                        if has_compile_operations(then_block)
                            || else_block
                                .as_ref()
                                .is_some_and(|eb| has_compile_operations(eb))
                        {
                            return Err("Compile Error: Dynamic conditions cannot contain @compile blocks or @compile::throw".to_string());
                        }
                        self.evaluate_ast(then_block)?;
                        if let Some(eb) = else_block {
                            self.evaluate_ast(eb)?;
                        }
                        new_stmts.push(stmt);
                    }
                }
                Stmt::WhileStmt {
                    condition, body, ..
                } => {
                    self.fold_expr(condition)?;
                    self.evaluate_either_block(body)?;
                    new_stmts.push(stmt);
                }
                Stmt::LoopStmt { count, body } => {
                    if let Some(c) = count {
                        self.fold_expr(c)?;
                    }
                    self.evaluate_either_block(body)?;
                    new_stmts.push(stmt);
                }
                Stmt::ForStmt {
                    init,
                    condition,
                    increment,
                    body,
                } => {
                    let mut prev_vars = Vec::new();
                    if let Some(i) = init {
                        if let Stmt::Declaration(Decl::VarDecl {
                                name,
                                type_node,
                                value,
                                ..
                            }) = &mut **i {
                            self.fold_expr(value)?;
                            let mut ty_name = self.resolve_type_str(&type_node.as_str());
                            if (ty_name == "unknown" || ty_name == "auto")
                                && !matches!(value, Expr::LiteralVoid)
                            {
                                let inferred = self.get_expr_type(value);
                                if inferred != "unknown" {
                                    ty_name = inferred;
                                }
                            }
                            let prev = self.var_types.get(name).cloned();
                            self.var_types.insert(name.clone(), ty_name);
                            prev_vars.push((name.clone(), prev));
                        }
                    }
                    if let Some(c) = condition {
                        self.fold_expr(c)?;
                    }
                    if let Some(inc) = increment {
                        match &mut **inc {
                            Stmt::ReassignStmt { value, .. } => {
                                self.fold_expr(value)?;
                            }
                            Stmt::ExpressionStmt(e) => {
                                self.fold_expr(e)?;
                            }
                            _ => {}
                        }
                    }
                    self.evaluate_either_block(body)?;
                    for (name, prev) in prev_vars {
                        match prev {
                            Some(t) => {
                                self.var_types.insert(name, t);
                            }
                            None => {
                                self.var_types.remove(&name);
                            }
                        }
                    }
                    new_stmts.push(stmt);
                }
                Stmt::ForIn {
                    item_decl,
                    iterable,
                    body,
                } => {
                    self.fold_expr(iterable)?;
                    let iterable_type = self.get_expr_type(iterable);
                    let elem_type = self.infer_iterable_element_type(&iterable_type, iterable);

                    let var_name = match &**item_decl {
                        Stmt::Declaration(Decl::VarDecl {
                            name, type_node, ..
                        }) => {
                            let ty_str = type_node.as_str();
                            let final_ty =
                                if ty_str != "auto" && ty_str != "unknown" && !ty_str.is_empty() {
                                    self.resolve_type_str(&ty_str)
                                } else {
                                    elem_type.clone()
                                };
                            Some((name.clone(), final_ty))
                        }
                        Stmt::ExpressionStmt(Expr::Identifier(name)) => {
                            Some((name.clone(), elem_type.clone()))
                        }
                        _ => None,
                    };

                    let prev_var = if let Some((ref name, ref ty)) = var_name {
                        let prev = self.var_types.get(name).cloned();
                        self.var_types.insert(name.clone(), ty.clone());
                        Some((name.clone(), prev))
                    } else {
                        None
                    };

                    self.evaluate_ast(body)?;

                    if let Some((name, prev)) = prev_var {
                        match prev {
                            Some(t) => {
                                self.var_types.insert(name, t);
                            }
                            None => {
                                self.var_types.remove(&name);
                            }
                        }
                    }

                    new_stmts.push(stmt);
                }
                Stmt::CaseStmt { option, set, body } => {
                    self.fold_expr(option)?;
                    self.fold_expr(set)?;
                    let mut inserted_vars = Vec::new();
                    if let Some(ref switch_type) = self.current_switch_type.clone() {
                        if let Expr::Call { callee, args, .. } = option {
                            let variant_name = match &**callee {
                                Expr::Identifier(id) => id.clone(),
                                Expr::NamespaceAccess { property, .. } => {
                                    if let Expr::Identifier(p) = &**property {
                                        p.clone()
                                    } else {
                                        "".to_string()
                                    }
                                }
                                _ => "".to_string(),
                            };
                            if args.len() == 1 {
                                if let Expr::Identifier(var_name) = &args[0] {
                                    if switch_type.contains('<') && switch_type.ends_with('>') {
                                        let start = switch_type.find('<').unwrap() + 1;
                                        let end = switch_type.rfind('>').unwrap();
                                        let generic_part = &switch_type[start..end];
                                        let gen_types: Vec<&str> = generic_part.split(',').map(|s| s.trim()).collect();
                                        let inner_t = if gen_types.len() == 1 {
                                            gen_types[0].to_string()
                                        } else if variant_name == "Err" && gen_types.len() > 1 {
                                            gen_types[1].to_string()
                                        } else {
                                            gen_types[0].to_string()
                                        };
                                        self.var_types.insert(var_name.clone(), inner_t);
                                        inserted_vars.push(var_name.clone());
                                    }
                                }
                            }
                        }
                    }
                    self.evaluate_ast(body)?;
                    for v in inserted_vars {
                        self.var_types.remove(&v);
                    }
                    new_stmts.push(stmt);
                }
                Stmt::SwitchStmt {
                    condition, cases, ..
                } => {
                    self.fold_expr(condition)?;
                    let cond_type = self.get_expr_type(condition);
                    let prev_switch_type = self.current_switch_type.take();
                    self.current_switch_type = if cond_type != "unknown" {
                        Some(cond_type)
                    } else {
                        None
                    };
                    self.evaluate_ast(cases)?;
                    self.current_switch_type = prev_switch_type;
                    new_stmts.push(stmt);
                }
                Stmt::TryCatchStmt {
                    try_block,
                    catches,
                } => {
                    self.evaluate_ast(try_block)?;
                    for c in catches {
                        self.evaluate_ast(&mut c.body)?;
                    }
                    new_stmts.push(stmt);
                }
                Stmt::Block(inner) | Stmt::ThisBlock(inner) => {
                    self.env.push_scope();
                    self.evaluate_ast(inner)?;
                    self.env.pop_scope();
                    new_stmts.push(stmt);
                }
                Stmt::CallStmt(e) | Stmt::ExpressionStmt(e) => {
                    let call_info = match e {
                        Expr::MacroCall { callee, args } => {
                            let m_name = match &**callee {
                                Expr::Identifier(id) => Some(id.clone()),
                                Expr::NamespaceAccess {
                                    namespace,
                                    property,
                                } => {
                                    if let Expr::Identifier(p) = &**property {
                                        Some(format!("{}::{}", namespace, p))
                                    } else {
                                        None
                                    }
                                }
                                _ => None,
                            };
                            m_name.map(|name| (name, args.clone()))
                        }
                        Expr::Call { callee, args, .. } => {
                            let m_name = match &**callee {
                                Expr::Identifier(id) => Some(id.clone()),
                                Expr::NamespaceAccess {
                                    namespace,
                                    property,
                                } => {
                                    if let Expr::Identifier(p) = &**property {
                                        Some(format!("{}::{}", namespace, p))
                                    } else {
                                        None
                                    }
                                }
                                _ => None,
                            };
                            m_name.map(|name| (name, args.clone()))
                        }
                        _ => None,
                    };

                    let inline_info = if let Some((ref name, ref call_args)) = call_info {
                        let candidates = vec![
                            name.clone(),
                            name.trim_start_matches('$').to_string(),
                            format!("${}", name),
                            name.split("::").last().unwrap_or(name).to_string(),
                            name.split("::").last().unwrap_or(name).trim_start_matches('$').to_string(),
                        ];
                        let mut found = None;
                        for cand in &candidates {
                            if let Some(overloads) = self.macros.get(cand) {
                                if let Some((params, body)) = overloads.iter().find(|(p, _)| p.len() == call_args.len()).or_else(|| overloads.first()) {
                                    found = Some((params.clone(), call_args.clone(), body.clone()));
                                    break;
                                }
                            }
                            if let Some((params, body)) = self.micros.get(cand) {
                                found = Some((params.clone(), call_args.clone(), body.clone()));
                                break;
                            }
                        }
                        found
                    } else {
                        None
                    };

                    if let Some((params, mut call_args, body)) = inline_info {
                        for a in call_args.iter_mut() {
                            self.fold_expr(a)?;
                        }
                        let param_map = self.build_micro_param_map(&params, &call_args);
                        let mut expanded = Vec::new();
                        for s in &body {
                            if let Stmt::ReturnStmt(Some(Expr::BlockExpr { statements, final_expr })) = s {
                                for st in statements {
                                    expanded.push(substitute_stmt(st, &param_map));
                                }
                                if let Some(fe) = final_expr {
                                    expanded.push(Stmt::ExpressionStmt(substitute_expr(fe, &param_map)));
                                }
                            } else {
                                expanded.push(substitute_stmt(s, &param_map));
                            }
                        }
                        self.evaluate_ast(&mut expanded)?;
                        new_stmts.extend(expanded);
                        continue;
                    }

                    self.fold_expr(e)?;
                    if matches!(e, Expr::LiteralVoid) {
                        continue;
                    }
                    new_stmts.push(stmt);
                }
                Stmt::DoWhileStmt { body, condition } => {
                    self.fold_expr(condition)?;
                    self.evaluate_either_block(body)?;
                    new_stmts.push(stmt);
                }
                Stmt::ForInStmt {
                    item,
                    iterable,
                    body,
                } => {
                    self.fold_expr(iterable)?;
                    let iterable_type = self.get_expr_type(iterable);
                    let elem_type = self.infer_iterable_element_type(&iterable_type, iterable);

                    let var_name = match &**item {
                        Stmt::Declaration(Decl::VarDecl {
                            name, type_node, ..
                        }) => {
                            let ty_str = type_node.as_str();
                            let final_ty =
                                if ty_str != "auto" && ty_str != "unknown" && !ty_str.is_empty() {
                                    self.resolve_type_str(&ty_str)
                                } else {
                                    elem_type.clone()
                                };
                            Some((name.clone(), final_ty))
                        }
                        Stmt::ExpressionStmt(Expr::Identifier(name)) => {
                            Some((name.clone(), elem_type.clone()))
                        }
                        _ => None,
                    };

                    let prev_var = if let Some((ref name, ref ty)) = var_name {
                        let prev = self.var_types.get(name).cloned();
                        self.var_types.insert(name.clone(), ty.clone());
                        Some((name.clone(), prev))
                    } else {
                        None
                    };

                    self.evaluate_either_block(body)?;

                    if let Some((name, prev)) = prev_var {
                        match prev {
                            Some(t) => {
                                self.var_types.insert(name, t);
                            }
                            None => {
                                self.var_types.remove(&name);
                            }
                        }
                    }

                    new_stmts.push(stmt);
                }
                _ => {
                    new_stmts.push(stmt);
                }
            }
        }
        *stmts = new_stmts;
        Ok(())
    }

    pub fn prune_static_branches(
        &mut self,
        stmts: &mut Vec<Stmt>,
        params: &[Param],
        generics: &[BaseType],
    ) -> Result<(), String> {
        let mut new_stmts = Vec::with_capacity(stmts.len());
        for mut stmt in stmts.drain(..) {
            match &mut stmt {
                Stmt::IfStmt {
                    condition,
                    then_block,
                    else_block,
                } => {
                    if !expr_references_params(condition, params, generics) {
                        let mut cond_clone = condition.clone();
                        if self.fold_expr(&mut cond_clone).is_ok() {
                            if let Expr::LiteralBool(b) = cond_clone {
                                if b {
                                    self.prune_static_branches(then_block, params, generics)?;
                                    new_stmts.append(then_block);
                                } else if let Some(eb) = else_block {
                                    self.prune_static_branches(eb, params, generics)?;
                                    new_stmts.append(eb);
                                }
                                continue;
                            }
                        }
                    }
                    self.prune_static_branches(then_block, params, generics)?;
                    if let Some(eb) = else_block {
                        self.prune_static_branches(eb, params, generics)?;
                    }
                    new_stmts.push(stmt);
                }
                Stmt::WhileStmt { body, .. } => {
                    if let EitherBlock::Inline(b) = body {
                        self.prune_static_branches(b, params, generics)?;
                    }
                    new_stmts.push(stmt);
                }
                Stmt::LoopStmt { body, .. } => {
                    if let EitherBlock::Inline(b) = body {
                        self.prune_static_branches(b, params, generics)?;
                    }
                    new_stmts.push(stmt);
                }
                _ => {
                    new_stmts.push(stmt);
                }
            }
        }
        *stmts = new_stmts;
        Ok(())
    }

    pub fn evaluate_decl(&mut self, decl: &mut Decl) -> Result<(), String> {
        match decl {
            Decl::VarDecl {
                name,
                value,
                type_node,
                ..
            } => {
                self.fold_expr(value)?;
                let mut ty_name = self.resolve_type_str(&type_node.as_str());
                if (ty_name == "unknown" || ty_name == "auto")
                    && !matches!(value, Expr::LiteralVoid)
                {
                    let inferred = self.get_expr_type(value);
                    if inferred != "unknown" {
                        ty_name = inferred;
                    }
                }
                self.var_types.insert(name.clone(), ty_name.clone());
                if is_literal(value) {
                    self.env.define(name.clone(), value.clone());
                } else if ty_name == "type"
                    || ty_name.starts_with("type<")
                    || type_node.as_str() == "type"
                {
                    if let Expr::LiteralString(ref s) = value {
                        if s.starts_with("type<") {
                            self.env.define(name.clone(), value.clone());
                        }
                    }
                }
            }
            Decl::DestructureDecl {
                type_node,
                assignments,
                ..
            } => {
                for (name, val) in assignments {
                    self.fold_expr(val)?;
                    let mut ty_name = self.resolve_type_str(&type_node.as_str());
                    if (ty_name == "unknown" || ty_name == "auto")
                        && !matches!(val, Expr::LiteralVoid)
                    {
                        let inferred = self.get_expr_type(val);
                        if inferred != "unknown" {
                            ty_name = inferred;
                        }
                    }
                    self.var_types.insert(name.clone(), ty_name.clone());
                    if is_literal(val) {
                        self.env.define(name.clone(), val.clone());
                    }
                }
            }
            Decl::ObjectDestructureDecl { fields, rhs, .. } => {
                self.fold_expr(rhs)?;
                for (ty, name) in fields {
                    let ty_name = self.resolve_type_str(&ty.as_str());
                    self.var_types.insert(name.clone(), ty_name);
                }
            }
            Decl::ArrayDecl {
                name,
                type_node,
                value,
                ..
            } => {
                self.fold_expr(value)?;
                self.var_types
                    .insert(name.clone(), format!("array<{}>", type_node.as_str()));
                if is_literal(value) {
                    self.env.define(name.clone(), value.clone());
                }
            }
            Decl::FnDecl {
                name,
                generics,
                params,
                return_type,
                body,
                ..
            } => {
                if let BaseType::TypeExpr(_) = return_type {
                    if let Some(sigs) = self.fn_overloads.get(name) {
                        if let Some(sig) = sigs.first() {
                            if !matches!(sig.return_type, BaseType::TypeExpr(_)) {
                                *return_type = sig.return_type.clone();
                            }
                        }
                    }
                }
                let prev_in_func = self.in_function_body;
                self.in_function_body = true;
                if generics.is_empty() {
                    self.evaluate_ast(body)?;
                } else {
                    self.prune_static_branches(body, params, generics)?;
                }
                self.in_function_body = prev_in_func;
                self.functions.insert(
                    name.clone(),
                    (generics.clone(), params.clone(), body.clone()),
                );
            }
            Decl::ClassDecl {
                name,
                public_block,
                private_block,
                handle_block,
                static_block,
                ..
            } => {
                let prev_t = self.current_type.take();
                self.current_type = Some(name.clone());
                for d in public_block {
                    self.evaluate_decl(d)?;
                }
                for d in private_block {
                    self.evaluate_decl(d)?;
                }
                for d in handle_block {
                    self.evaluate_decl(d)?;
                }
                for d in static_block {
                    self.evaluate_decl(d)?;
                }
                self.current_type = prev_t;
            }
            Decl::StructDecl {
                name,
                public_block,
                private_block,
                handle_block,
                static_block,
                ..
            } => {
                let prev_t = self.current_type.take();
                self.current_type = Some(name.clone());
                for d in public_block {
                    self.evaluate_decl(d)?;
                }
                for d in private_block {
                    self.evaluate_decl(d)?;
                }
                for d in handle_block {
                    self.evaluate_decl(d)?;
                }
                for d in static_block {
                    self.evaluate_decl(d)?;
                }
                self.current_type = prev_t;
            }
            Decl::ImplDecl {
                target,
                methods,
                handle_block,
                ..
            } => {
                let prev_t = self.current_type.take();
                self.current_type = Some(target.clone());
                let clean_target = self.extract_clean_blueprint_name(target);
                for m in methods {
                    if let Decl::FnDecl {
                        name,
                        generics,
                        params,
                        body,
                        ..
                    } = &*m
                    {
                        self.functions.insert(
                            name.clone(),
                            (generics.clone(), params.clone(), body.clone()),
                        );
                        self.functions.insert(
                            format!("{}::{}", clean_target, name),
                            (generics.clone(), params.clone(), body.clone()),
                        );
                    }
                    self.evaluate_decl(m)?;
                }
                for h in handle_block {
                    if let Decl::FnDecl {
                        name,
                        generics,
                        params,
                        body,
                        ..
                    } = &*h
                    {
                        self.functions.insert(
                            name.clone(),
                            (generics.clone(), params.clone(), body.clone()),
                        );
                        self.functions.insert(
                            format!("{}::{}", clean_target, name),
                            (generics.clone(), params.clone(), body.clone()),
                        );
                    }
                    self.evaluate_decl(h)?;
                }
                self.current_type = prev_t;
            }
            Decl::NamespaceDecl { name, decls, .. } => {
                for d in decls {
                    self.evaluate_decl(d)?;
                    match d {
                        Decl::FnDecl {
                            name: fn_name,
                            generics,
                            params,
                            body,
                            ..
                        } => {
                            self.functions.insert(
                                format!("{}::{}", name, fn_name),
                                (generics.clone(), params.clone(), body.clone()),
                            );
                        }
                        Decl::MacroDecl {
                            name: m_name,
                            params,
                            body,
                            ..
                        } => {
                            let clean_m = m_name.trim_start_matches('$');
                            self.macros
                                .entry(format!("{}::{}", name, m_name))
                                .or_default()
                                .push((params.clone(), body.clone()));
                            self.macros
                                .entry(format!("{}::{}", name, clean_m))
                                .or_default()
                                .push((params.clone(), body.clone()));
                            self.macros
                                .entry(m_name.clone())
                                .or_default()
                                .push((params.clone(), body.clone()));
                            self.macros
                                .entry(clean_m.to_string())
                                .or_default()
                                .push((params.clone(), body.clone()));
                        }
                        _ => {}
                    }
                }
            }
            Decl::CompileDecl { name, decls } => {
                for d in decls {
                    self.evaluate_decl(d)?;
                    match d {
                        Decl::FnDecl {
                            name: fn_name,
                            generics,
                            params,
                            body,
                            ..
                        } => {
                            self.functions.insert(
                                format!("{}::{}", name, fn_name),
                                (generics.clone(), params.clone(), body.clone()),
                            );
                            self.functions.insert(
                                fn_name.clone(),
                                (generics.clone(), params.clone(), body.clone()),
                            );
                        }
                        Decl::MacroDecl {
                            name: m_name,
                            params,
                            body,
                            ..
                        } => {
                            let clean_m = m_name.trim_start_matches('$');
                            self.macros
                                .entry(format!("{}::{}", name, m_name))
                                .or_default()
                                .push((params.clone(), body.clone()));
                            self.macros
                                .entry(format!("{}::{}", name, clean_m))
                                .or_default()
                                .push((params.clone(), body.clone()));
                            self.macros
                                .entry(m_name.clone())
                                .or_default()
                                .push((params.clone(), body.clone()));
                            self.macros
                                .entry(clean_m.to_string())
                                .or_default()
                                .push((params.clone(), body.clone()));
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub fn evaluate_either_block(&mut self, block: &mut EitherBlock) -> Result<(), String> {
        match block {
            EitherBlock::Inline(stmts) => self.evaluate_ast(stmts),
            EitherBlock::External(expr) => self.fold_expr(expr),
        }
    }


}
