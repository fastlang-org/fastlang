use crate::frontend::parser::ast::*;
use crate::middle_end::interpreter::intrinsics::CompilerIntrinsics;
use crate::middle_end::interpreter::Interpreter;
use crate::middle_end::interpreter::env::InterpreterEnv;
use crate::middle_end::interpreter::eval::{
    expr_contains_this, is_known_type_name, is_literal, strip_type_wrapper,
};

impl Interpreter {
    pub fn eval_expr(&mut self, expr: &Expr) -> Result<Expr, String> {
        match expr {
            Expr::LiteralInt(_)
            | Expr::LiteralUInt(_)
            | Expr::LiteralFloat(_)
            | Expr::LiteralString(_)
            | Expr::LiteralBool(_)
            | Expr::LiteralChar(_)
            | Expr::LiteralVoid
            | Expr::LiteralUndefined => Ok(expr.clone()),
            Expr::Identifier(name) => self.env.get(name),
            Expr::Cast { expr, target_type } => {
                let val = self.eval_expr(expr)?;
                let target_name = match target_type {
                    BaseType::GenericParam(g_name) => g_name.clone(),
                    BaseType::Blueprint { name, .. } => name.clone(),
                    BaseType::New(n) => n.clone(),
                    _ => target_type.as_str(),
                };
                let resolved_target = if let Some(Expr::LiteralString(s)) = self.env.get_ref(&target_name)
                {
                    let clean = if s.starts_with("type<") && s.ends_with('>') {
                        &s[5..s.len() - 1]
                    } else {
                        s.as_str()
                    };
                    BaseType::from_str(clean)
                } else {
                    target_type.clone()
                };
                self.fastlang_cast(&val, &resolved_target)
            }
            Expr::UnaryOp { operator, operand } => {
                let val = self.eval_expr(operand)?;
                match (operator.as_str(), val) {
                    ("!", Expr::LiteralBool(b)) => Ok(Expr::LiteralBool(!b)),
                    ("-", Expr::LiteralInt(i)) => Ok(Expr::LiteralInt(-i)),
                    ("-", Expr::LiteralFloat(f)) => Ok(Expr::LiteralFloat(-f)),
                    _ => Ok(Expr::LiteralUndefined),
                }
            }
            Expr::IfExpr {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond_val = self.eval_expr(condition)?;
                let is_true = match cond_val {
                    Expr::LiteralBool(b) => b,
                    Expr::LiteralInt(i) => i != 0,
                    _ => false,
                };
                if is_true {
                    self.eval_expr(then_branch)
                } else {
                    self.eval_expr(else_branch)
                }
            }
            Expr::BlockExpr {
                statements,
                final_expr,
            } => {
                self.env.push_scope();
                for stmt in statements {
                    let _ = self.eval_stmt(stmt)?;
                }
                let res = if let Some(f) = final_expr {
                    self.eval_expr(f)
                } else {
                    Ok(Expr::LiteralVoid)
                };
                self.env.pop_scope();
                res
            }
            Expr::PropertyAccess { object, property } => {
                let obj_val = self.eval_expr(object).unwrap_or_else(|_| *object.clone());
                let type_name = match &obj_val {
                    Expr::LiteralString(s) => strip_type_wrapper(s).to_string(),
                    Expr::Identifier(id) => {
                        if let Some(Expr::LiteralString(s)) = self.env.get_ref(id) {
                            strip_type_wrapper(s).to_string()
                        } else {
                            id.clone()
                        }
                    }
                    _ => "".to_string(),
                };
                let is_type_reflection = matches!(obj_val, Expr::LiteralString(ref s) if s.starts_with("type<"))
                    || (!type_name.is_empty() && is_known_type_name(&type_name));
                if !is_type_reflection {
                    return Err(
                        "PropertyAccess on non-type cannot be evaluated at compile time"
                            .to_string(),
                    );
                }
                let ft = self.get_fast_type(&type_name);
                match property.as_str() {
                    "is_primitive" => Ok(Expr::LiteralBool(ft.is_primitive())),
                    "is_pointer" => Ok(Expr::LiteralBool(ft.is_pointer())),
                    "is_array" => Ok(Expr::LiteralBool(ft.is_array())),
                    "printable" | "is_printable" => Ok(Expr::LiteralBool(ft.printable())),
                    "throwable" | "is_throwable" => Ok(Expr::LiteralBool(ft.throwable())),
                    "shareable" | "is_shareable" => Ok(Expr::LiteralBool(ft.shareable())),
                    "copyable" | "is_copyable" => Ok(Expr::LiteralBool(ft.copyable())),
                    "castable" | "is_castable" => Ok(Expr::LiteralBool(ft.castable())),
                    "is_constant" | "is_const" => Ok(Expr::LiteralBool(ft.is_constant())),
                    "is_runtime" => Ok(Expr::LiteralBool(ft.is_runtime())),
                    "is_comptime" => Ok(Expr::LiteralBool(ft.is_comptime())),
                    "is_undefined" => Ok(Expr::LiteralBool(ft.is_undefined())),
                    "on_heap" => Ok(Expr::LiteralBool(ft.on_heap())),
                    "on_stack" => Ok(Expr::LiteralBool(ft.on_stack())),
                    "as_str" => {
                        if matches!(obj_val, Expr::LiteralString(ref s) if s.starts_with("type<")) {
                            Ok(Expr::LiteralString(ft.as_str()))
                        } else {
                            Err("as_str only supported on type reflection in comptime eval"
                                .to_string())
                        }
                    }
                    "size" => Ok(Expr::LiteralInt(ft.size() as i128)),
                    "default" => {
                        if let Some(def) = self.default_for_type(&type_name) {
                            Ok(def)
                        } else {
                            Ok(Expr::Default(Some(BaseType::from_str(&type_name))))
                        }
                    }
                    _ => {
                        if let Some(method_name) = property.strip_prefix("has_") {
                            Ok(Expr::LiteralBool(
                                ft.has_method(method_name)
                                    || ft.has_handle(method_name)
                                    || (method_name == "as_str"
                                        && (ft.printable()
                                            || ft.has_method("as_str")
                                            || ft.has_handle("display"))),
                            ))
                        } else {
                            Ok(Expr::LiteralUndefined)
                        }
                    }
                }
            }
            Expr::BinaryOp {
                left,
                operator,
                right,
            } => {
                if operator == "==" || operator == "!=" {
                    let left_ty = self.extract_type_string(left);
                    let right_ty = self.extract_type_string(right);
                    if let (Some(lt), Some(rt)) = (left_ty, right_ty) {
                        let clean_lt = strip_type_wrapper(&lt).to_string();
                        let clean_rt = strip_type_wrapper(&rt).to_string();
                        let eq = clean_lt == clean_rt;
                        let res = if operator == "==" { eq } else { !eq };
                        return Ok(Expr::LiteralBool(res));
                    }
                }

                let l = self.eval_expr(left)?;
                let r = self.eval_expr(right)?;
                match (l, operator.as_str(), r) {
                    (Expr::LiteralInt(a), "+", Expr::LiteralInt(b)) => Ok(Expr::LiteralInt(a + b)),
                    (Expr::LiteralInt(a), "-", Expr::LiteralInt(b)) => Ok(Expr::LiteralInt(a - b)),
                    (Expr::LiteralInt(a), "*", Expr::LiteralInt(b)) => Ok(Expr::LiteralInt(a * b)),
                    (Expr::LiteralInt(a), "/", Expr::LiteralInt(b)) if b != 0 => {
                        Ok(Expr::LiteralInt(a / b))
                    }
                    (Expr::LiteralFloat(a), "+", Expr::LiteralFloat(b)) => {
                        Ok(Expr::LiteralFloat(a + b))
                    }
                    (Expr::LiteralFloat(a), "-", Expr::LiteralFloat(b)) => {
                        Ok(Expr::LiteralFloat(a - b))
                    }
                    (Expr::LiteralFloat(a), "*", Expr::LiteralFloat(b)) => {
                        Ok(Expr::LiteralFloat(a * b))
                    }
                    (Expr::LiteralFloat(a), "/", Expr::LiteralFloat(b)) if b != 0.0 => {
                        Ok(Expr::LiteralFloat(a / b))
                    }
                    (Expr::LiteralInt(a), "+", Expr::LiteralFloat(b)) => {
                        Ok(Expr::LiteralFloat(a as f64 + b))
                    }
                    (Expr::LiteralFloat(a), "+", Expr::LiteralInt(b)) => {
                        Ok(Expr::LiteralFloat(a + b as f64))
                    }
                    (Expr::LiteralString(mut a), "+", Expr::LiteralString(b)) => {
                        a.push_str(&b);
                        Ok(Expr::LiteralString(a))
                    }
                    (Expr::LiteralString(mut a), "+", Expr::LiteralChar(c)) => {
                        a.push(c);
                        Ok(Expr::LiteralString(a))
                    }
                    (Expr::LiteralString(mut a), "+", Expr::LiteralUChar(c)) => {
                        if let Some(ch) = std::char::from_u32(c) {
                            a.push(ch);
                        }
                        Ok(Expr::LiteralString(a))
                    }
                    (Expr::LiteralChar(c), "+", Expr::LiteralString(b)) => {
                        let mut s = c.to_string();
                        s.push_str(&b);
                        Ok(Expr::LiteralString(s))
                    }
                    (Expr::LiteralUChar(c), "+", Expr::LiteralString(b)) => {
                        let mut s = String::new();
                        if let Some(ch) = std::char::from_u32(c) {
                            s.push(ch);
                        }
                        s.push_str(&b);
                        Ok(Expr::LiteralString(s))
                    }
                    (Expr::LiteralBool(a), "&&", Expr::LiteralBool(b)) => {
                        Ok(Expr::LiteralBool(a && b))
                    }
                    (Expr::LiteralBool(a), "||", Expr::LiteralBool(b)) => {
                        Ok(Expr::LiteralBool(a || b))
                    }
                    (Expr::LiteralInt(a), "==", Expr::LiteralInt(b)) => {
                        Ok(Expr::LiteralBool(a == b))
                    }
                    (Expr::LiteralInt(a), "!=", Expr::LiteralInt(b)) => {
                        Ok(Expr::LiteralBool(a != b))
                    }
                    (Expr::LiteralInt(a), "<", Expr::LiteralInt(b)) => Ok(Expr::LiteralBool(a < b)),
                    (Expr::LiteralInt(a), "<=", Expr::LiteralInt(b)) => {
                        Ok(Expr::LiteralBool(a <= b))
                    }
                    (Expr::LiteralInt(a), ">", Expr::LiteralInt(b)) => Ok(Expr::LiteralBool(a > b)),
                    (Expr::LiteralInt(a), ">=", Expr::LiteralInt(b)) => {
                        Ok(Expr::LiteralBool(a >= b))
                    }
                    (Expr::LiteralFloat(a), "==", Expr::LiteralFloat(b)) => {
                        Ok(Expr::LiteralBool(a == b))
                    }
                    (Expr::LiteralFloat(a), "!=", Expr::LiteralFloat(b)) => {
                        Ok(Expr::LiteralBool(a != b))
                    }
                    (Expr::LiteralFloat(a), "<", Expr::LiteralFloat(b)) => {
                        Ok(Expr::LiteralBool(a < b))
                    }
                    (Expr::LiteralFloat(a), "<=", Expr::LiteralFloat(b)) => {
                        Ok(Expr::LiteralBool(a <= b))
                    }
                    (Expr::LiteralFloat(a), ">", Expr::LiteralFloat(b)) => {
                        Ok(Expr::LiteralBool(a > b))
                    }
                    (Expr::LiteralFloat(a), ">=", Expr::LiteralFloat(b)) => {
                        Ok(Expr::LiteralBool(a >= b))
                    }
                    (Expr::LiteralInt(a), "<", Expr::LiteralFloat(b)) => {
                        Ok(Expr::LiteralBool((a as f64) < b))
                    }
                    (Expr::LiteralInt(a), "<=", Expr::LiteralFloat(b)) => {
                        Ok(Expr::LiteralBool((a as f64) <= b))
                    }
                    (Expr::LiteralInt(a), ">", Expr::LiteralFloat(b)) => {
                        Ok(Expr::LiteralBool((a as f64) > b))
                    }
                    (Expr::LiteralInt(a), ">=", Expr::LiteralFloat(b)) => {
                        Ok(Expr::LiteralBool((a as f64) >= b))
                    }
                    (Expr::LiteralFloat(a), "<", Expr::LiteralInt(b)) => {
                        Ok(Expr::LiteralBool(a < (b as f64)))
                    }
                    (Expr::LiteralFloat(a), "<=", Expr::LiteralInt(b)) => {
                        Ok(Expr::LiteralBool(a <= (b as f64)))
                    }
                    (Expr::LiteralFloat(a), ">", Expr::LiteralInt(b)) => {
                        Ok(Expr::LiteralBool(a > (b as f64)))
                    }
                    (Expr::LiteralFloat(a), ">=", Expr::LiteralInt(b)) => {
                        Ok(Expr::LiteralBool(a >= (b as f64)))
                    }
                    (Expr::LiteralChar(a), "==", Expr::LiteralChar(b)) => {
                        Ok(Expr::LiteralBool(a == b))
                    }
                    (Expr::LiteralChar(a), "!=", Expr::LiteralChar(b)) => {
                        Ok(Expr::LiteralBool(a != b))
                    }
                    (Expr::LiteralChar(a), "<", Expr::LiteralChar(b)) => {
                        Ok(Expr::LiteralBool(a < b))
                    }
                    (Expr::LiteralChar(a), "<=", Expr::LiteralChar(b)) => {
                        Ok(Expr::LiteralBool(a <= b))
                    }
                    (Expr::LiteralChar(a), ">", Expr::LiteralChar(b)) => {
                        Ok(Expr::LiteralBool(a > b))
                    }
                    (Expr::LiteralChar(a), ">=", Expr::LiteralChar(b)) => {
                        Ok(Expr::LiteralBool(a >= b))
                    }
                    (Expr::LiteralUChar(a), "==", Expr::LiteralUChar(b)) => {
                        Ok(Expr::LiteralBool(a == b))
                    }
                    (Expr::LiteralUChar(a), "!=", Expr::LiteralUChar(b)) => {
                        Ok(Expr::LiteralBool(a != b))
                    }
                    (Expr::LiteralUChar(a), "<", Expr::LiteralUChar(b)) => {
                        Ok(Expr::LiteralBool(a < b))
                    }
                    (Expr::LiteralUChar(a), "<=", Expr::LiteralUChar(b)) => {
                        Ok(Expr::LiteralBool(a <= b))
                    }
                    (Expr::LiteralUChar(a), ">", Expr::LiteralUChar(b)) => {
                        Ok(Expr::LiteralBool(a > b))
                    }
                    (Expr::LiteralUChar(a), ">=", Expr::LiteralUChar(b)) => {
                        Ok(Expr::LiteralBool(a >= b))
                    }
                    (l_val, op, r_val) => Ok(Expr::BinaryOp {
                        left: Box::new(l_val),
                        operator: op.to_string(),
                        right: Box::new(r_val),
                    }),
                }
            }
            Expr::New { target, .. } => {
                // For 'new Error("msg")', the target is often Instantiate
                self.eval_expr(target)
            }
            Expr::Instantiate { args, .. } => {
                // Just bubble up the first argument if it's a string, or undefined
                if let Some(first) = args.first() {
                    self.eval_expr(first)
                } else {
                    Ok(Expr::LiteralUndefined)
                }
            }
            Expr::Call {
                callee,
                generics,
                args,
            } => {
                if let Expr::PropertyAccess { object, property } = &**callee {
                    let obj_val = self.eval_expr(object).unwrap_or_else(|_| *object.clone());
                    let type_name = match &obj_val {
                        Expr::LiteralString(s) => strip_type_wrapper(s).to_string(),
                        Expr::Identifier(id) => {
                            if let Ok(Expr::LiteralString(s)) = self.env.get(id) {
                                strip_type_wrapper(&s).to_string()
                            } else {
                                id.clone()
                            }
                        }
                        _ => "".to_string(),
                    };
                    let is_type_reflection = matches!(obj_val, Expr::LiteralString(ref s) if s.starts_with("type<"))
                        || (!type_name.is_empty() && is_known_type_name(&type_name));
                    if is_type_reflection {
                        let ft = self.get_fast_type(&type_name);
                        match property.as_str() {
                            "is_primitive" => return Ok(Expr::LiteralBool(ft.is_primitive())),
                            "is_pointer" => return Ok(Expr::LiteralBool(ft.is_pointer())),
                            "is_array" => return Ok(Expr::LiteralBool(ft.is_array())),
                            "printable" | "is_printable" => {
                                return Ok(Expr::LiteralBool(ft.printable()))
                            }
                            "throwable" | "is_throwable" => {
                                return Ok(Expr::LiteralBool(ft.throwable()))
                            }
                            "shareable" | "is_shareable" => {
                                return Ok(Expr::LiteralBool(ft.shareable()))
                            }
                            "copyable" | "is_copyable" => {
                                return Ok(Expr::LiteralBool(ft.copyable()))
                            }
                            "castable" | "is_castable" => {
                                return Ok(Expr::LiteralBool(ft.castable()))
                            }
                            "is_constant" | "is_const" => {
                                return Ok(Expr::LiteralBool(ft.is_constant()))
                            }
                            "is_runtime" => return Ok(Expr::LiteralBool(ft.is_runtime())),
                            "is_comptime" => return Ok(Expr::LiteralBool(ft.is_comptime())),
                            "is_undefined" => return Ok(Expr::LiteralBool(ft.is_undefined())),
                            "on_heap" => return Ok(Expr::LiteralBool(ft.on_heap())),
                            "on_stack" => return Ok(Expr::LiteralBool(ft.on_stack())),
                            "has_handle" => {
                                if let Some(arg) = args.first() {
                                    let evaled =
                                        self.eval_expr(arg).unwrap_or_else(|_| arg.clone());
                                    let s1 = self.extract_type_or_name(&evaled);
                                    let s2 = args.get(1).map(|a| {
                                        let v = self.eval_expr(a).unwrap_or_else(|_| a.clone());
                                        self.extract_type_or_name(&v)
                                    });
                                    return Ok(Expr::LiteralBool(
                                        ft.has_handle_query(&s1, s2.as_deref()),
                                    ));
                                }
                                return Ok(Expr::LiteralBool(false));
                            }
                            "has_method" => {
                                if let Some(arg) = args.first() {
                                    let evaled =
                                        self.eval_expr(arg).unwrap_or_else(|_| arg.clone());
                                    let s1 = self.extract_type_or_name(&evaled);
                                    let s2 = args.get(1).map(|a| {
                                        let v = self.eval_expr(a).unwrap_or_else(|_| a.clone());
                                        self.extract_type_or_name(&v)
                                    });
                                    return Ok(Expr::LiteralBool(
                                        ft.has_method_query(&s1, s2.as_deref()),
                                    ));
                                }
                                return Ok(Expr::LiteralBool(false));
                            }
                            "has_field" => {
                                if let Some(arg) = args.first() {
                                    let evaled =
                                        self.eval_expr(arg).unwrap_or_else(|_| arg.clone());
                                    let s1 = self.extract_type_or_name(&evaled);
                                    let s2 = args.get(1).map(|a| {
                                        let v = self.eval_expr(a).unwrap_or_else(|_| a.clone());
                                        self.extract_type_or_name(&v)
                                    });
                                    return Ok(Expr::LiteralBool(
                                        ft.has_field_query(&s1, s2.as_deref()),
                                    ));
                                }
                                return Ok(Expr::LiteralBool(false));
                            }
                            "as_str" => {
                                if matches!(obj_val, Expr::LiteralString(ref s) if s.starts_with("type<"))
                                {
                                    return Ok(Expr::LiteralString(ft.as_str()));
                                } else if is_literal(&obj_val) {
                                    return Ok(Expr::LiteralString(
                                        self.format_expr_for_print(&obj_val),
                                    ));
                                } else {
                                    return Err(
                                        "as_str only supported on literal or type in comptime eval"
                                            .to_string(),
                                    );
                                }
                            }
                            "size" => {
                                if matches!(obj_val, Expr::LiteralString(ref s) if s.starts_with("type<"))
                                {
                                    return Ok(Expr::LiteralInt(ft.size() as i128));
                                } else {
                                    return Err("size method only supported on type reflection at compile time".to_string());
                                }
                            }
                            "default" => {
                                if let Some(def) = self.default_for_type(&type_name) {
                                    return Ok(def);
                                } else {
                                    return Ok(Expr::Default(Some(BaseType::from_str(&type_name))));
                                }
                            }
                            "castable_to" => {
                                if let Some(target_expr) = args.first() {
                                    let evaled = self
                                        .eval_expr(target_expr)
                                        .unwrap_or_else(|_| target_expr.clone());
                                    let target_name = match evaled {
                                        Expr::LiteralString(s) => {
                                            strip_type_wrapper(&s).to_string()
                                        }
                                        Expr::Identifier(id) => {
                                            if let Ok(Expr::LiteralString(s)) = self.env.get(&id) {
                                                strip_type_wrapper(&s).to_string()
                                            } else {
                                                id
                                            }
                                        }
                                        _ => "".to_string(),
                                    };
                                    let target_ft = self.get_fast_type(&target_name);
                                    return Ok(Expr::LiteralBool(ft.castable_to(&target_ft)));
                                }
                            }
                            _ => {
                                if let Some(method_name) = property.strip_prefix("has_") {
                                    return Ok(Expr::LiteralBool(
                                        ft.has_method(method_name)
                                            || ft.has_handle(method_name)
                                            || (method_name == "as_str"
                                                && (ft.printable()
                                                    || ft.has_method("as_str")
                                                    || ft.has_handle("display"))),
                                    ));
                                }
                            }
                        }
                    }
                }

                let target_fn_name = match &**callee {
                    Expr::NamespaceAccess {
                        namespace,
                        property,
                    } if namespace == "@compile" => {
                        if let Expr::Identifier(p) = &**property {
                            format!("@compile::{}", p)
                        } else {
                            "".to_string()
                        }
                    }
                    Expr::Identifier(id) => id.clone(),
                    _ => "".to_string(),
                };

                if target_fn_name == "@compile::typeof" || target_fn_name == "typeof" {
                    if let Some(arg) = args.first() {
                        let t = self.fastlang_typeof(arg);
                        if t != "unknown" {
                            return Ok(Expr::LiteralString(format!("type<{}>", t)));
                        } else {
                            return Ok(Expr::LiteralString("type<unknown>".to_string()));
                        }
                    }
                    return Ok(expr.clone());
                }

                if target_fn_name == "@compile::sizeof" || target_fn_name == "sizeof" {
                    if let Some(arg) = args.first() {
                        let sz = self.fastlang_sizeof(arg);
                        if sz > 0 {
                            return Ok(Expr::LiteralInt(sz as i128));
                        }
                    }
                    return Ok(expr.clone());
                }
                if target_fn_name == "@compile::print" {
                    let mut parts = Vec::new();
                    for arg in args {
                        let val = self.eval_expr(arg).unwrap_or_else(|_| arg.clone());
                        if let Expr::ArrayLiteral(elems) = val {
                            for elem in elems {
                                parts.push(self.format_expr_for_print(&elem));
                            }
                        } else {
                            parts.push(self.format_expr_for_print(&val));
                        }
                    }
                    println!("{}", parts.join(" "));
                    return Ok(Expr::LiteralVoid);
                }

                if target_fn_name == "@compile::rand" || target_fn_name == "rand" {
                    let target_type = generics
                        .first()
                        .cloned()
                        .unwrap_or(BaseType::Int(crate::frontend::parser::ast::Size::S32));
                    if !CompilerIntrinsics::is_numeric_type(&target_type) {
                        return self.fastlang_throw(&format!(
                            "@compile::rand only supports numeric types, found '{}'",
                            target_type.as_str()
                        ));
                    }
                    let evaled_args: Vec<Expr> = args
                        .iter()
                        .map(|a| self.eval_expr(a).unwrap_or_else(|_| a.clone()))
                        .collect();
                    return self.fastlang_rand(&target_type, &evaled_args);
                }

                if target_fn_name == "@compile::access" || target_fn_name == "access" {
                    if args.len() < 2 {
                        return self.fastlang_throw(
                            "@compile::access requires at least 2 arguments: (target, prop)",
                        );
                    }
                    let target_expr = &args[0];
                    let prop_expr = &args[1];

                    if expr_contains_this(target_expr) {
                        return Ok(expr.clone());
                    }

                    let (prop_name, expected_ty) = self.extract_prop_name_and_type(prop_expr);
                    if prop_name.is_empty() {
                        return self.fastlang_throw(
                            "@compile::access requires a valid member name in member specification",
                        );
                    }

                    let target_type = self.get_expr_type(target_expr);
                    let clean_target = self.extract_clean_blueprint_name(&target_type);

                    if let Some(ref env_rc) = self.current_env.clone() {
                        let env = env_rc.borrow();
                        if let Some(bp) = env.lookup_blueprint(&clean_target) {
                            let has_field = bp.fields.get(&prop_name);
                            let has_method = bp.methods.get(&prop_name);
                            let has_handle = bp.handle_signatures.get(&prop_name);
                            if has_field.is_none() && has_method.is_none() && has_handle.is_none() {
                                return self.fastlang_throw(&format!(
                                    "Member '{}' not found on type '{}'",
                                    prop_name, clean_target
                                ));
                            }
                            if let Some(ref expected) = expected_ty {
                                if let Some(field_ty) = has_field {
                                    let clean_actual =
                                        self.extract_clean_blueprint_name(&field_ty.as_str());
                                    let clean_expected =
                                        self.extract_clean_blueprint_name(expected);
                                    if clean_actual != clean_expected
                                        && field_ty.as_str() != *expected
                                    {
                                        return self.fastlang_throw(&format!(
                                            "Member '{}' on type '{}' has type '{}', but expected '{}'",
                                            prop_name, clean_target, field_ty.as_str(), expected
                                        ));
                                    }
                                }
                            }
                        }
                    }

                    return Ok(Expr::PropertyAccess {
                        object: Box::new(target_expr.clone()),
                        property: prop_name,
                    });
                }

                if target_fn_name.starts_with("@compile::")
                    && !self.functions.contains_key(&target_fn_name)
                {
                    let clean = target_fn_name.trim_start_matches("@compile::");
                    if !CompilerIntrinsics::is_compile_member(clean) {
                        eprintln!(
                            "STACKTRACE:\n{:?}",
                            std::backtrace::Backtrace::force_capture()
                        );
                        return self.fastlang_throw(&format!(
                            "Unknown compile intrinsic '{}'",
                            target_fn_name
                        ));
                    }
                }
                if let Some((params, body)) = self.micros.get(&target_fn_name).cloned() {
                    let mut eval_args = Vec::new();
                    for a in args {
                        eval_args.push(self.eval_expr(a).unwrap_or_else(|_| a.clone()));
                    }
                    let mut micro_interp = Interpreter {
                        env: InterpreterEnv::with_parent(self.env.clone()),
                        functions: self.functions.clone(),
                        micros: self.micros.clone(),
                        var_types: self.var_types.clone(),
                        current_env: self.current_env.clone(),
                        fn_overloads: self.fn_overloads.clone(),
                        split_functions: self.split_functions.clone(),
                        in_function_body: true,
                        current_switch_type: None,
                        current_type: self.current_type.clone(),
                        target_backend: self.target_backend.clone(),
                    };
                    for (i, param) in params.iter().enumerate() {
                        if let Some(arg_val) = eval_args.get(i) {
                            micro_interp.env.define(param.name.clone(), arg_val.clone());
                            let arg_t = self.get_expr_type(arg_val);
                            micro_interp.var_types.insert(param.name.clone(), arg_t);
                        }
                    }
                    if let Some(ret_val) = micro_interp.eval_method(&body, &params, &eval_args)? {
                        return Ok(ret_val);
                    } else {
                        return Ok(Expr::LiteralVoid);
                    }
                }

                let is_compilable = target_fn_name.starts_with("@compile::") || {
                    if let Some((_, _, body)) = self.functions.get(&target_fn_name) {
                        crate::middle_end::semantic::analyzer::detect_execution_mode(body)
                            == ExecutionMode::FullyCompilable
                    } else {
                        false
                    }
                };

                if is_compilable {
                    if let Some((fn_generics, params, body)) =
                        self.functions.get(&target_fn_name).cloned()
                    {
                        let mut eval_args = Vec::new();
                        for a in args {
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

                        let mut func_interp = Interpreter {
                            env: InterpreterEnv::with_parent(self.env.clone()),
                            functions: self.functions.clone(),
                            micros: self.micros.clone(),
                            var_types: self.var_types.clone(),
                            current_env: self.current_env.clone(),
                            fn_overloads: self.fn_overloads.clone(),
                            split_functions: self.split_functions.clone(),
                            in_function_body: true,
                            current_switch_type: None,
                            current_type: self.current_type.clone(),
                            target_backend: self.target_backend.clone(),
                        };
                        for (g_name, g_type) in &generic_map {
                            func_interp.env.define(
                                g_name.clone(),
                                Expr::LiteralString(format!("type<{}>", g_type.as_str())),
                            );
                        }
                        for (i, param) in params.iter().enumerate() {
                            if param.is_variadic {
                                let rest: Vec<Expr> = eval_args[i..].to_vec();
                                func_interp
                                    .env
                                    .define(param.name.clone(), Expr::ArrayLiteral(rest));
                                break;
                            }
                            let arg_val = if let Some(val) = eval_args.get(i) {
                                Some(val.clone())
                            } else if let Some(ref def) = param.default_value {
                                let folded =
                                    func_interp.eval_expr(def).unwrap_or_else(|_| def.clone());
                                Some(folded)
                            } else {
                                None
                            };
                            if let Some(val) = arg_val {
                                func_interp.env.define(param.name.clone(), val.clone());
                                let arg_t = func_interp.get_expr_type(&val);
                                func_interp.var_types.insert(param.name.clone(), arg_t);
                            }
                        }

                        if let Some(ret_val) =
                            func_interp.eval_method(&body, &params, &eval_args)?
                        {
                            return Ok(ret_val);
                        } else {
                            return Ok(Expr::LiteralVoid);
                        }
                    }
                }

                Ok(expr.clone())
            }
            _ => Ok(expr.clone()),
        }
    }

}

