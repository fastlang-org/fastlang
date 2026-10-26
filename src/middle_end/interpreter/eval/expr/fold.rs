use crate::frontend::parser::ast;
use crate::frontend::parser::ast::*;
use crate::middle_end::interpreter::env::InterpreterEnv;
use crate::middle_end::interpreter::eval::is_literal;
use crate::middle_end::interpreter::eval::strip_type_wrapper;
use crate::middle_end::interpreter::eval::substitute::substitute_expr;
use crate::middle_end::interpreter::intrinsics::CompilerIntrinsics;
use crate::middle_end::interpreter::Interpreter;
use crate::middle_end::semantic::analyzer::resolve_call_generics;

impl Interpreter {
    pub fn fold_expr(&mut self, expr: &mut Expr) -> Result<(), String> {
        match expr {
            Expr::UnaryOp { operator, operand } => {
                self.fold_expr(operand)?;
                match (operator.as_str(), &**operand) {
                    ("!", Expr::LiteralBool(b)) => *expr = Expr::LiteralBool(!b),
                    ("-", Expr::LiteralInt(i)) => *expr = Expr::LiteralInt(-i),
                    ("-", Expr::LiteralFloat(f)) => *expr = Expr::LiteralFloat(-f),
                    _ => {}
                }
            }
            Expr::BinaryOp {
                left,
                operator,
                right,
            } => {
                self.fold_expr(left)?;
                self.fold_expr(right)?;

                if operator == "->" || operator == "=>" {
                    let handle_name = if operator == "->" {
                        "arrow"
                    } else {
                        "fat_arrow"
                    };
                    if let Expr::Identifier(prop_name) = &**right {
                        if let Some(expanded) =
                            self.try_expand_access_handle(left, prop_name, handle_name)?
                        {
                            *expr = expanded;
                            return self.fold_expr(expr);
                        }
                    } else if let Expr::Call {
                        callee,
                        generics,
                        args,
                    } = &**right
                    {
                        if let Expr::Identifier(prop_name) = &**callee {
                            if let Some(expanded) =
                                self.try_expand_access_handle(left, prop_name, handle_name)?
                            {
                                *expr = Expr::Call {
                                    callee: Box::new(expanded),
                                    generics: generics.clone(),
                                    args: args.clone(),
                                };
                                return self.fold_expr(expr);
                            }
                        }
                    }
                }

                // Namespace / Enum comparison, e.g. Backend::Cpp == Backend::Cpp
                if operator == "==" || operator == "!=" {
                    let extract_ns = |e: &Expr| -> Option<(String, String)> {
                        match e {
                            Expr::NamespaceAccess {
                                namespace,
                                property,
                            } => {
                                if let Expr::Identifier(p) = &**property {
                                    Some((namespace.clone(), p.clone()))
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        }
                    };
                    if let (Some((l_ns, l_prop)), Some((r_ns, r_prop))) =
                        (extract_ns(left), extract_ns(right))
                    {
                        if l_ns == r_ns {
                            let eq = l_prop == r_prop;
                            let res = if operator == "==" { eq } else { !eq };
                            *expr = Expr::LiteralBool(res);
                            return Ok(());
                        }
                    }
                }

                if operator == "&&" {
                    if let (Expr::LiteralBool(l), Expr::LiteralBool(r)) = (&**left, &**right) {
                        *expr = Expr::LiteralBool(*l && *r);
                        return Ok(());
                    }
                }
                if operator == "||" {
                    if let (Expr::LiteralBool(l), Expr::LiteralBool(r)) = (&**left, &**right) {
                        *expr = Expr::LiteralBool(*l || *r);
                        return Ok(());
                    }
                }

                // Type comparisons, e.g. typeof(id) == int32 or t == int32
                if operator == "==" || operator == "!=" {
                    let left_ty = self.extract_type_string(left);
                    let right_ty = self.extract_type_string(right);
                    if let (Some(lt), Some(rt)) = (left_ty, right_ty) {
                        let clean_lt = strip_type_wrapper(&lt).to_string();
                        let clean_rt = strip_type_wrapper(&rt).to_string();
                        let eq = clean_lt == clean_rt;
                        let res = if operator == "==" { eq } else { !eq };
                        *expr = Expr::LiteralBool(res);
                        return Ok(());
                    }
                }

                if is_literal(left) && is_literal(right) {
                    if let (Expr::LiteralString(s), "+", Expr::LiteralChar(c)) =
                        (&**left, operator.as_str(), &**right)
                    {
                        *expr = Expr::LiteralString(format!("{}{}", s, c));
                        return Ok(());
                    }
                    if let (Expr::LiteralChar(c), "+", Expr::LiteralString(s)) =
                        (&**left, operator.as_str(), &**right)
                    {
                        *expr = Expr::LiteralString(format!("{}{}", c, s));
                        return Ok(());
                    }
                    if let Ok(res) = self.eval_expr(expr) {
                        if is_literal(&res) {
                            *expr = res;
                        }
                    }
                }
            }
            Expr::HandleCall {
                object,
                handle_name,
                args,
            } => {
                self.fold_expr(object)?;
                for a in args.iter_mut() {
                    self.fold_expr(a)?;
                }
                if args.is_empty() {
                    if let Some(expanded) =
                        self.try_expand_access_handle(object, handle_name, "handle_access")?
                    {
                        *expr = expanded;
                        return self.fold_expr(expr);
                    }
                }
            }
            Expr::NamespaceAccess {
                namespace,
                property,
            } => {
                if namespace == "@compile" {
                    if let Expr::Identifier(prop_name) = &**property {
                        if prop_name == "backend" {
                            let variant = if self.target_backend == "cranelift" {
                                "Cranelift"
                            } else {
                                "Cpp"
                            };
                            *expr = Expr::NamespaceAccess {
                                namespace: "Backend".to_string(),
                                property: Box::new(Expr::Identifier(variant.to_string())),
                            };
                            return Ok(());
                        } else if prop_name == "system" {
                            let variant = if cfg!(target_os = "windows") {
                                "Windows"
                            } else if cfg!(target_os = "macos") {
                                "MacOS"
                            } else {
                                "Linux"
                            };
                            *expr = Expr::NamespaceAccess {
                                namespace: "System".to_string(),
                                property: Box::new(Expr::Identifier(variant.to_string())),
                            };
                            return Ok(());
                        } else if prop_name == "time" {
                            *expr = super::builtins::create_comptime_time_expr();
                            return Ok(());
                        }
                    }
                }
                self.fold_expr(property)?;
                if let Expr::Identifier(prop_name) = &**property {
                    let obj_expr = Expr::Identifier(namespace.clone());
                    if let Some(expanded) =
                        self.try_expand_access_handle(&obj_expr, prop_name, "namespace_access")?
                    {
                        *expr = expanded;
                        return self.fold_expr(expr);
                    }
                }
            }
            Expr::Cast {
                expr: inner,
                target_type,
            } => {
                self.fold_expr(inner)?;

                // Execute @compile validation block from cast handle if defined (e.g. array::cast)
                let inner_type = self.get_expr_type(inner);
                let (base_name, elem_type) = if inner_type.ends_with("[]") {
                    (
                        "array".to_string(),
                        inner_type[..inner_type.len() - 2].trim().to_string(),
                    )
                } else if inner_type.starts_with("array<") && inner_type.ends_with('>') {
                    (
                        "array".to_string(),
                        inner_type["array<".len()..inner_type.len() - 1]
                            .trim()
                            .to_string(),
                    )
                } else {
                    (
                        self.extract_clean_blueprint_name(&inner_type),
                        inner_type.clone(),
                    )
                };

                if elem_type != "unknown"
                    && elem_type != "T"
                    && !matches!(target_type, BaseType::GenericParam(_))
                {
                    let cast_fn_key = format!("{}::cast", base_name);
                    let found_fn = self.functions.get(&cast_fn_key).cloned().or_else(|| {
                        if base_name == "array" {
                            self.functions.get("cast").cloned()
                        } else {
                            None
                        }
                    });

                    if let Some((_fn_generics, _params, body)) = found_fn {
                        for stmt in &body {
                            if let Stmt::CompileValidation { body: val_body, .. } = stmt {
                                let mut val_interp = Interpreter {
                                    env: InterpreterEnv::with_parent(self.env.clone()),
                                    functions: self.functions.clone(),
                                    micros: self.micros.clone(),
                                    macros: self.macros.clone(),
                                    var_types: self.var_types.clone(),
                                    current_env: self.current_env.clone(),
                                    fn_overloads: self.fn_overloads.clone(),
                                    split_functions: self.split_functions.clone(),
                                    in_function_body: true,
                                    in_compile_block: true,
                                    current_switch_type: None,
                                    current_type: self.current_type.clone(),
                                    target_backend: self.target_backend.clone(),
                                };

                                if base_name == "array" {
                                    val_interp.env.define(
                                        "T".to_string(),
                                        Expr::LiteralString(format!("type<{}>", elem_type)),
                                    );
                                    val_interp
                                        .var_types
                                        .insert("T".to_string(), "type".to_string());
                                }
                                val_interp.env.define(
                                    "Target".to_string(),
                                    Expr::LiteralString(format!("type<{}>", target_type.as_str())),
                                );
                                val_interp
                                    .var_types
                                    .insert("Target".to_string(), "type".to_string());
                                val_interp.env.define("this".to_string(), (**inner).clone());

                                for s in val_body {
                                    val_interp.eval_stmt(s)?;
                                }
                            }
                        }
                    }
                }

                let resolved = if let Expr::Identifier(id) = &**inner {
                    self.env.get(id).unwrap_or_else(|_| *inner.clone())
                } else {
                    *inner.clone()
                };
                if is_literal(&resolved) {
                    if let Ok(res) = self.fastlang_cast(&resolved, target_type) {
                        *expr = res;
                    }
                }
            }
            Expr::PropertyAccess { object, property } => {
                self.fold_expr(object)?;
                if let Some(expanded) =
                    self.try_expand_access_handle(object, property, "property_access")?
                {
                    *expr = expanded;
                    return self.fold_expr(expr);
                }
                let obj_type_name = self.extract_type_string(object);
                if let Some(t_name) = obj_type_name {
                    let ft = self.get_fast_type(&t_name);
                    match property.as_str() {
                        "primitive" => *expr = Expr::LiteralBool(ft.is_primitive()),
                        "pointer" => *expr = Expr::LiteralBool(ft.is_pointer()),
                        "array" => *expr = Expr::LiteralBool(ft.is_array()),
                        "printable" => *expr = Expr::LiteralBool(ft.printable()),
                        "throwable" => *expr = Expr::LiteralBool(ft.throwable()),
                        "shareable" => *expr = Expr::LiteralBool(ft.shareable()),
                        "copyable" => *expr = Expr::LiteralBool(ft.copyable()),
                        "castable" => *expr = Expr::LiteralBool(ft.castable()),
                        "constant" => *expr = Expr::LiteralBool(ft.is_constant()),
                        "runtime" => *expr = Expr::LiteralBool(ft.is_runtime()),
                        "comptime" => *expr = Expr::LiteralBool(ft.is_comptime()),
                        "undefined" => *expr = Expr::LiteralBool(ft.is_undefined()),
                        "on_heap" => *expr = Expr::LiteralBool(ft.on_heap()),
                        "on_stack" => *expr = Expr::LiteralBool(ft.on_stack()),
                        "size" => *expr = Expr::LiteralInt(ft.size() as i128),
                        "as_str" => *expr = Expr::LiteralString(ft.as_str()),
                        "default" => {
                            if let Some(def) = self.default_for_type(&t_name) {
                                *expr = def;
                            } else {
                                *expr = Expr::Default(Some(BaseType::from_str(&t_name)));
                            }
                        }
                        _ => {
                            if let Some(method_name) = property.strip_prefix("has_") {
                                *expr = Expr::LiteralBool(
                                    ft.has_method(method_name)
                                        || ft.has_handle(method_name)
                                        || (method_name == "as_str"
                                            && (ft.printable()
                                                || ft.has_method("as_str")
                                                || ft.has_handle("display"))),
                                );
                            }
                        }
                    }
                } else if let Ok(res) = self.eval_expr(expr) {
                    if is_literal(&res) {
                        *expr = res;
                    }
                }
            }
            Expr::MacroCall { callee, args } => {
                for a in args.iter_mut() {
                    self.fold_expr(a)?;
                }

                let macro_name = match &**callee {
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

                if let Some(ref name) = macro_name {
                    let candidates = vec![
                        name.clone(),
                        name.trim_start_matches('$').to_string(),
                        format!("${}", name),
                        name.split("::").last().unwrap_or(name).to_string(),
                        name.split("::").last().unwrap_or(name).trim_start_matches('$').to_string(),
                    ];
                    let mut macro_match = None;
                    for cand in &candidates {
                        if let Some(overloads) = self.macros.get(cand) {
                            if let Some((params, body)) = overloads.iter().find(|(p, _)| p.len() == args.len()) {
                                macro_match = Some((params.clone(), body.clone()));
                                break;
                            }
                        }
                    }
                    if let Some((params, body)) = macro_match {
                        let ret_expr = body.iter().find_map(|s| match s {
                            Stmt::ReturnStmt(Some(Expr::BlockExpr { statements, final_expr })) => {
                                if let Some(fe) = final_expr {
                                    Some((**fe).clone())
                                } else {
                                    statements.iter().find_map(|st| match st {
                                        Stmt::ReturnStmt(Some(e))
                                        | Stmt::ExpressionStmt(e)
                                        | Stmt::CallStmt(e) => Some(e.clone()),
                                        _ => None,
                                    })
                                }
                            }
                            Stmt::ReturnStmt(Some(e)) => Some(e.clone()),
                            Stmt::ExpressionStmt(e) | Stmt::CallStmt(e) => Some(e.clone()),
                            _ => None,
                        });

                        if let Some(r_expr) = ret_expr {
                            let param_map = self.build_micro_param_map(&params, args);
                            let mut substituted = substitute_expr(&r_expr, &param_map);
                            self.fold_expr(&mut substituted)?;
                            *expr = substituted;
                            return Ok(());
                        } else {
                            *expr = Expr::LiteralVoid;
                            return Ok(());
                        }
                    }
                }
            }
            Expr::Call {
                callee,
                generics,
                args,
            } => {
                for a in args.iter_mut() {
                    self.fold_expr(a)?;
                }

                if let Expr::NamespaceAccess {
                    namespace,
                    property,
                } = &**callee
                {
                    if namespace == "@compile" {
                        if let Expr::Identifier(prop_name) = &**property {
                            if prop_name == "backend" {
                                let variant = if self.target_backend == "cranelift" {
                                    "Cranelift"
                                } else {
                                    "Cpp"
                                };
                                *expr = Expr::NamespaceAccess {
                                    namespace: "Backend".to_string(),
                                    property: Box::new(Expr::Identifier(variant.to_string())),
                                };
                                return Ok(());
                            } else if prop_name == "system" {
                                let variant = if cfg!(target_os = "windows") {
                                    "Windows"
                                } else if cfg!(target_os = "macos") {
                                    "MacOS"
                                } else {
                                    "Linux"
                                };
                                *expr = Expr::NamespaceAccess {
                                    namespace: "System".to_string(),
                                    property: Box::new(Expr::Identifier(variant.to_string())),
                                };
                                return Ok(());
                            } else if prop_name == "time" {
                                *expr = super::builtins::create_comptime_time_expr();
                                return Ok(());
                            }
                        }
                    }
                }

                // Expand micro calls in expression contexts
                let micro_info = {
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
                    if let Some(ref name) = m_name {
                        self.micros.get(name).cloned().or_else(|| {
                            if let Some(short) = name.split("::").last() {
                                self.micros.get(short).cloned()
                            } else {
                                None
                            }
                        })
                    } else {
                        None
                    }
                };

                if let Some((params, body)) = micro_info {
                    let ret_expr = body.iter().find_map(|s| match s {
                        Stmt::ReturnStmt(Some(e)) => Some(e.clone()),
                        Stmt::ExpressionStmt(e) | Stmt::CallStmt(e) => Some(e.clone()),
                        _ => None,
                    });
                    if let Some(r_expr) = ret_expr {
                        let param_map = self.build_micro_param_map(&params, args);
                        let mut substituted = substitute_expr(&r_expr, &param_map);
                        self.fold_expr(&mut substituted)?;
                        *expr = substituted;
                        return Ok(());
                    } else {
                        *expr = Expr::LiteralVoid;
                        return Ok(());
                    }
                }

                // Expand macro calls in expression contexts if called via Expr::Call
                let macro_info = {
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
                    if let Some(ref name) = m_name {
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
                                if let Some((params, body)) = overloads.iter().find(|(p, _)| p.len() == args.len()) {
                                    found = Some((params.clone(), body.clone()));
                                    break;
                                }
                            }
                        }
                        found
                    } else {
                        None
                    }
                };

                if let Some((params, body)) = macro_info {
                    let ret_expr = body.iter().find_map(|s| match s {
                        Stmt::ReturnStmt(Some(Expr::BlockExpr { statements, final_expr })) => {
                            if let Some(fe) = final_expr {
                                Some((**fe).clone())
                            } else {
                                statements.iter().find_map(|st| match st {
                                    Stmt::ReturnStmt(Some(e))
                                    | Stmt::ExpressionStmt(e)
                                    | Stmt::CallStmt(e) => Some(e.clone()),
                                    _ => None,
                                })
                            }
                        }
                        Stmt::ReturnStmt(Some(e)) => Some(e.clone()),
                        Stmt::ExpressionStmt(e) | Stmt::CallStmt(e) => Some(e.clone()),
                        _ => None,
                    });
                    if let Some(r_expr) = ret_expr {
                        let param_map = self.build_micro_param_map(&params, args);
                        let mut substituted = substitute_expr(&r_expr, &param_map);
                        self.fold_expr(&mut substituted)?;
                        *expr = substituted;
                        return Ok(());
                    } else {
                        *expr = Expr::LiteralVoid;
                        return Ok(());
                    }
                }

                // Property method calls on types, e.g. typeof(id).printable(), typeof(id).castable_to(float64)
                if let Expr::PropertyAccess { object, property } = &mut **callee {
                    self.fold_expr(object)?;
                    let obj_type_name = self.extract_type_string(object);
                    if let Some(t_name) = obj_type_name {
                        let ft = self.get_fast_type(&t_name);
                        match property.as_str() {
                            "printable" => {
                                *expr = Expr::LiteralBool(ft.printable());
                                return Ok(());
                            }
                            "throwable" => {
                                *expr = Expr::LiteralBool(ft.throwable());
                                return Ok(());
                            }
                            "shareable" => {
                                *expr = Expr::LiteralBool(ft.shareable());
                                return Ok(());
                            }
                            "copyable" => {
                                *expr = Expr::LiteralBool(ft.copyable());
                                return Ok(());
                            }
                            "castable" => {
                                *expr = Expr::LiteralBool(ft.castable());
                                return Ok(());
                            }
                            "castable_to" => {
                                if let Some(target_arg) = args.first() {
                                    let target_name = self
                                        .extract_type_string(target_arg)
                                        .unwrap_or_else(|| self.get_expr_type(target_arg));
                                    let target_ft = self.get_fast_type(&target_name);
                                    *expr = Expr::LiteralBool(ft.castable_to(&target_ft));
                                    return Ok(());
                                }
                            }
                            "primitive" => {
                                *expr = Expr::LiteralBool(ft.is_primitive());
                                return Ok(());
                            }
                            "pointer" => {
                                *expr = Expr::LiteralBool(ft.is_pointer());
                                return Ok(());
                            }
                            "array" => {
                                *expr = Expr::LiteralBool(ft.is_array());
                                return Ok(());
                            }
                            "constant" => {
                                *expr = Expr::LiteralBool(ft.is_constant());
                                return Ok(());
                            }
                            "runtime" => {
                                *expr = Expr::LiteralBool(ft.is_runtime());
                                return Ok(());
                            }
                            "comptime" => {
                                *expr = Expr::LiteralBool(ft.is_comptime());
                                return Ok(());
                            }
                            "undefined" => {
                                *expr = Expr::LiteralBool(ft.is_undefined());
                                return Ok(());
                            }
                            "on_heap" => {
                                *expr = Expr::LiteralBool(ft.on_heap());
                                return Ok(());
                            }
                            "on_stack" => {
                                *expr = Expr::LiteralBool(ft.on_stack());
                                return Ok(());
                            }
                            "as_str" => {
                                *expr = Expr::LiteralString(ft.as_str());
                                return Ok(());
                            }
                            "has_handle" => {
                                if let Some(target_arg) = args.first() {
                                    let val = self
                                        .eval_expr(target_arg)
                                        .unwrap_or_else(|_| target_arg.clone());
                                    let s1 = self.extract_type_or_name(&val);
                                    let s2 = args.get(1).map(|a| {
                                        let v = self.eval_expr(a).unwrap_or_else(|_| a.clone());
                                        self.extract_type_or_name(&v)
                                    });
                                    *expr =
                                        Expr::LiteralBool(ft.has_handle_query(&s1, s2.as_deref()));
                                    return Ok(());
                                }
                                *expr = Expr::LiteralBool(false);
                                return Ok(());
                            }
                            "has_method" => {
                                if let Some(target_arg) = args.first() {
                                    let val = self
                                        .eval_expr(target_arg)
                                        .unwrap_or_else(|_| target_arg.clone());
                                    let s1 = self.extract_type_or_name(&val);
                                    let s2 = args.get(1).map(|a| {
                                        let v = self.eval_expr(a).unwrap_or_else(|_| a.clone());
                                        self.extract_type_or_name(&v)
                                    });
                                    *expr =
                                        Expr::LiteralBool(ft.has_method_query(&s1, s2.as_deref()));
                                    return Ok(());
                                }
                                *expr = Expr::LiteralBool(false);
                                return Ok(());
                            }
                            "has_field" => {
                                if let Some(target_arg) = args.first() {
                                    let val = self
                                        .eval_expr(target_arg)
                                        .unwrap_or_else(|_| target_arg.clone());
                                    let s1 = self.extract_type_or_name(&val);
                                    let s2 = args.get(1).map(|a| {
                                        let v = self.eval_expr(a).unwrap_or_else(|_| a.clone());
                                        self.extract_type_or_name(&v)
                                    });
                                    *expr =
                                        Expr::LiteralBool(ft.has_field_query(&s1, s2.as_deref()));
                                    return Ok(());
                                }
                                *expr = Expr::LiteralBool(false);
                                return Ok(());
                            }
                            "size" => {
                                *expr = Expr::LiteralInt(ft.size() as i128);
                                return Ok(());
                            }
                            "default" => {
                                if let Some(def) = self.default_for_type(&t_name) {
                                    *expr = def;
                                } else {
                                    *expr = Expr::Default(Some(BaseType::from_str(&t_name)));
                                }
                                return Ok(());
                            }
                            _ => {
                                if let Some(method_name) = property.strip_prefix("has_") {
                                    *expr = Expr::LiteralBool(
                                        ft.has_method(method_name)
                                            || ft.has_handle(method_name)
                                            || (method_name == "as_str"
                                                && (ft.printable()
                                                    || ft.has_method("as_str")
                                                    || ft.has_handle("display"))),
                                    );
                                    return Ok(());
                                }
                            }
                        }
                    }
                }
                self.fold_expr(callee)?;
                let folded_opt = {
                    let should_try_fold = match &**callee {
                        Expr::PropertyAccess { property, .. } => {
                            matches!(property.as_str(), "select" | "insert" | "get_content")
                        }
                        Expr::Identifier(ref fn_name) => {
                            matches!(fn_name.as_str(), "create_ident")
                                || fn_name.starts_with("@compile::")
                        }
                        Expr::NamespaceAccess { namespace, .. } => {
                            namespace == "@compile"
                        }
                        _ => false,
                    };
                    if should_try_fold {
                        let temp_expr = Expr::Call {
                            callee: callee.clone(),
                            generics: generics.clone(),
                            args: args.clone(),
                        };
                        match self.eval_expr(&temp_expr) {
                            Ok(res) => Some(res),
                            Err(_) => None,
                        }
                    } else {
                        None
                    }
                };
                if let Some(folded) = folded_opt {
                    *expr = folded;
                    return Ok(());
                }

                let fn_name = match &**callee {
                    Expr::NamespaceAccess {
                        namespace,
                        property,
                    } if namespace == "@compile" => {
                        if let Expr::Identifier(p) = &**property {
                            p.as_str()
                        } else {
                            ""
                        }
                    }
                    Expr::Identifier(id) => id.as_str(),
                    _ => "",
                };

                if fn_name == "typeof" {
                    if let Some(arg) = args.first() {
                        let t = self.get_expr_type(arg);
                        if t != "unknown" {
                            *expr = Expr::LiteralString(format!("type<{}>", t));
                            return Ok(());
                        }
                    }
                }

                if fn_name == "sizeof" {
                    if let Some(arg) = args.first() {
                        let t = self.get_expr_type(arg);
                        if t != "unknown" {
                            let sz = BaseType::from_str(&t).size();
                            *expr = Expr::LiteralInt(sz as i128);
                            return Ok(());
                        }
                    }
                }

                if fn_name == "rand" {
                    let target_type = generics
                        .first()
                        .cloned()
                        .unwrap_or(BaseType::Int(ast::Size::S32));
                    if CompilerIntrinsics::is_numeric_type(&target_type) {
                        let evaled_args: Vec<Expr> = args
                            .iter()
                            .map(|a| self.eval_expr(a).unwrap_or_else(|_| a.clone()))
                            .collect();
                        if let Ok(res) = self.fastlang_rand(&target_type, &evaled_args) {
                            *expr = res;
                            return Ok(());
                        }
                    }
                }

                if fn_name == "cast" {
                    if let (Some(target_type), Some(arg)) = (generics.first(), args.first()) {
                        let resolved = if let Expr::Identifier(id) = arg {
                            self.env.get(id).unwrap_or_else(|_| arg.clone())
                        } else {
                            arg.clone()
                        };
                        let target_str = target_type.as_str();
                        let is_generic =
                            matches!(target_type, BaseType::GenericParam(_) | BaseType::Unknown)
                                || (!BaseType::from_str(&target_str).is_primitive()
                                    && self.env.get(&target_str).is_err());
                        if !is_generic && is_literal(&resolved) {
                            if let Ok(res) = self.fastlang_cast(&resolved, target_type) {
                                *expr = res;
                                return Ok(());
                            }
                        }
                    }
                }

                // Resolve generic arguments for generic functions only if generics were provided
                if !generics.is_empty() {
                    if let Expr::Identifier(fn_name) = &**callee {
                        let fn_info = self.lookup_fn(fn_name);
                        if let Some((fn_generics, params)) = fn_info {
                            if !fn_generics.is_empty() {
                                let arg_types: Vec<String> =
                                    args.iter().map(|a| self.get_expr_type(a)).collect();
                                let generic_map = resolve_call_generics(
                                    &fn_generics,
                                    &params,
                                    generics,
                                    &arg_types,
                                );
                                let mut full_generics = Vec::new();
                                for g in &fn_generics {
                                    let g_name = match g {
                                        BaseType::GenericParam(n) => n.clone(),
                                        BaseType::New(n) => n.clone(),
                                        _ => g.as_str(),
                                    };
                                    if let Some(concrete) = generic_map.get(&g_name) {
                                        full_generics.push(concrete.clone());
                                    } else {
                                        full_generics.push(g.clone());
                                    }
                                }
                                let all_concrete = full_generics.iter().all(|g| {
                                    !matches!(g, BaseType::GenericParam(_) | BaseType::Unknown)
                                        && !g.as_str().starts_with("...")
                                });
                                if all_concrete {
                                    *generics = full_generics;
                                }
                            }
                        }
                    }
                }

                // Check if this is a split function with compile-time branches
                let split_call_info = if let Expr::Identifier(id) = &**callee {
                    self.split_functions.get(id).cloned()
                } else {
                    None
                };

                if let Some((params, branches)) = split_call_info {
                    let mut eval_args = Vec::new();
                    for a in args.iter() {
                        eval_args.push(self.eval_expr(a).unwrap_or_else(|_| a.clone()));
                    }
                    let mut cond_interp = Interpreter {
                        env: InterpreterEnv::with_parent(self.env.clone()),
                        functions: self.functions.clone(),
                        micros: self.micros.clone(),
                        macros: self.macros.clone(),
                        var_types: self.var_types.clone(),
                        current_env: self.current_env.clone(),
                        fn_overloads: self.fn_overloads.clone(),
                        split_functions: self.split_functions.clone(),
                        in_function_body: true,
                        in_compile_block: self.in_compile_block,
                        current_switch_type: None,
                        current_type: self.current_type.clone(),
                        target_backend: self.target_backend.clone(),
                    };
                    for (i, p) in params.iter().enumerate() {
                        if let Some(arg_val) = eval_args.get(i) {
                            cond_interp.env.define(p.name.clone(), arg_val.clone());
                            let arg_t = self.get_expr_type(arg_val);
                            cond_interp.var_types.insert(p.name.clone(), arg_t);
                        }
                    }
                    let mut selected_fn: Option<String> = None;
                    for (cond_opt, br_fn) in branches {
                        if let Some(cond) = cond_opt {
                            if let Ok(Expr::LiteralBool(true)) = cond_interp.eval_expr(&cond) {
                                selected_fn = Some(br_fn);
                                break;
                            }
                        } else {
                            selected_fn = Some(br_fn);
                            break;
                        }
                    }
                    if let Some(chosen_fn) = selected_fn {
                        **callee = Expr::Identifier(chosen_fn.clone());
                        let is_chosen_compilable = chosen_fn.starts_with("@compile::") || {
                            if let Some((_, _, body)) = self.functions.get(&chosen_fn) {
                                crate::middle_end::semantic::analyzer::detect_execution_mode(body)
                                    == ExecutionMode::FullyCompilable
                            } else {
                                false
                            }
                        };
                        if is_chosen_compilable {
                            let _ = self.eval_expr(expr)?;
                        }
                        return Ok(());
                    }
                }

                // Check if called function has a compile-time validation block (@compile { ... }(args))
                let fn_candidates = match &**callee {
                    Expr::Identifier(id) => vec![id.clone()],
                    Expr::NamespaceAccess {
                        namespace,
                        property,
                    } => {
                        if let Expr::Identifier(p) = &**property {
                            vec![format!("{}::{}", namespace, p), p.clone()]
                        } else {
                            vec![]
                        }
                    }
                    Expr::PropertyAccess { object, property } => {
                        let obj_type = self.get_expr_type(object);
                        let clean_obj = self.extract_clean_blueprint_name(&obj_type);
                        vec![format!("{}::{}", clean_obj, property), property.clone()]
                    }
                    _ => vec![],
                };
                let mut found_fn = None;
                for name in &fn_candidates {
                    if let Some(info) = self.functions.get(name).cloned() {
                        found_fn = Some(info);
                        break;
                    }
                }
                if let Some((fn_generics, params, body)) = found_fn {
                    let is_fn_fully_compilable =
                        crate::middle_end::semantic::analyzer::detect_execution_mode(&body)
                            == ExecutionMode::FullyCompilable;
                    if !is_fn_fully_compilable {
                        for stmt in &body {
                            if let Stmt::CompileValidation {
                                body: val_body,
                                args: _,
                            } = stmt
                            {
                                let mut eval_args = Vec::new();
                                for a in args.iter() {
                                    eval_args.push(self.eval_expr(a).unwrap_or_else(|_| a.clone()));
                                }
                                let arg_types: Vec<String> =
                                    eval_args.iter().map(|a| self.fastlang_typeof(a)).collect();
                                let generic_map =
                                    crate::middle_end::semantic::analyzer::resolve_call_generics(
                                        &fn_generics,
                                        &params,
                                        generics,
                                        &arg_types,
                                    );
                                let mut val_interp = Interpreter {
                                    env: InterpreterEnv::with_parent(self.env.clone()),
                                    functions: self.functions.clone(),
                                    micros: self.micros.clone(),
                                    macros: self.macros.clone(),
                                    var_types: self.var_types.clone(),
                                    current_env: self.current_env.clone(),
                                    fn_overloads: self.fn_overloads.clone(),
                                    split_functions: self.split_functions.clone(),
                                    in_function_body: true,
                                    in_compile_block: true,
                                    current_switch_type: None,
                                    current_type: self.current_type.clone(),
                                    target_backend: self.target_backend.clone(),
                                };
                                for (g_name, g_type) in &generic_map {
                                    val_interp.env.define(
                                        g_name.clone(),
                                        Expr::LiteralString(format!("type<{}>", g_type.as_str())),
                                    );
                                }
                                for (i, p) in params.iter().enumerate() {
                                    if p.is_variadic {
                                        let rest_args = if i < eval_args.len() {
                                            eval_args[i..].to_vec()
                                        } else {
                                            Vec::new()
                                        };
                                        val_interp
                                            .env
                                            .define(p.name.clone(), Expr::ArrayLiteral(rest_args));
                                        val_interp
                                            .var_types
                                            .insert(p.name.clone(), "array".to_string());
                                        break;
                                    }
                                    let arg_val = if let Some(val) = eval_args.get(i) {
                                        Some(val.clone())
                                    } else if let Some(ref def) = p.default_value {
                                        let folded = val_interp
                                            .eval_expr(def)
                                            .unwrap_or_else(|_| def.clone());
                                        Some(folded)
                                    } else {
                                        None
                                    };
                                    if let Some(val) = arg_val {
                                        val_interp.env.define(p.name.clone(), val.clone());
                                        let arg_t = val_interp.get_expr_type(&val);
                                        val_interp.var_types.insert(p.name.clone(), arg_t);
                                    }
                                }
                                for s in val_body {
                                    val_interp.eval_stmt(s)?;
                                }
                            }
                        }
                    }
                }

                // Evaluate via interpreter if compile-time function
                let is_compilable = match &**callee {
                    Expr::NamespaceAccess {
                        namespace,
                        property,
                    } if namespace == "@compile" => {
                        if let Expr::Identifier(p) = &**property {
                            if p == "print" {
                                !self.in_function_body
                            } else {
                                true
                            }
                        } else {
                            true
                        }
                    }
                    Expr::Identifier(id) => {
                        if id == "@compile::print" {
                            !self.in_function_body
                        } else if id.starts_with("@compile::") {
                            true
                        } else if let Some((_, _, body)) = self.functions.get(id) {
                            crate::middle_end::semantic::analyzer::detect_execution_mode(body)
                                == ExecutionMode::FullyCompilable
                        } else {
                            false
                        }
                    }
                    _ => false,
                };

                let is_explicit_compile = match &**callee {
                    Expr::NamespaceAccess { namespace, .. } => namespace == "@compile",
                    Expr::Identifier(id) => id.starts_with("@compile::"),
                    _ => false,
                };

                if is_compilable {
                    match self.eval_expr(expr) {
                        Ok(res) => {
                            if is_literal(&res)
                                || matches!(
                                    &res,
                                    Expr::UnaryOp { .. }
                                        | Expr::PropertyAccess { .. }
                                        | Expr::LiteralVoid
                                )
                            {
                                *expr = res;
                            }
                        }
                        Err(e) => {
                            if is_explicit_compile || e.contains("Compile Error") {
                                return Err(e);
                            }
                        }
                    }
                }
            }
            Expr::QuestionMark(inner) => {
                self.fold_expr(inner)?;
            }
            Expr::IfExpr {
                condition,
                then_branch,
                else_branch,
            } => {
                self.fold_expr(condition)?;
                if let Expr::LiteralBool(b) = **condition {
                    if b {
                        self.fold_expr(then_branch)?;
                        *expr = (**then_branch).clone();
                    } else {
                        self.fold_expr(else_branch)?;
                        *expr = (**else_branch).clone();
                    }
                    return Ok(());
                } else {
                    self.fold_expr(then_branch)?;
                    self.fold_expr(else_branch)?;
                }
            }
            Expr::Lambda { body, .. } => {
                self.evaluate_ast(body)?;
            }
            Expr::BlockExpr {
                statements,
                final_expr,
            } => {
                self.evaluate_ast(statements)?;
                if let Some(fe) = final_expr {
                    self.fold_expr(fe)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}
