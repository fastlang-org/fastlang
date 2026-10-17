use crate::frontend::parser::ast::*;
use crate::middle_end::interpreter::Interpreter;
use crate::middle_end::interpreter::eval::{
    is_known_type_name, is_typeof_callee, strip_type_wrapper, substitute_this_in_expr,
};
use crate::middle_end::semantic::environment::SymbolKind;
use colored::Colorize;

impl Interpreter {
    pub fn fastlang_typeof(&mut self, expr: &Expr) -> String {
        self.get_expr_type(expr)
    }

    pub fn fastlang_sizeof(&mut self, expr: &Expr) -> u32 {
        let t = self.get_expr_type(expr);
        BaseType::from_str(&t).size()
    }

    pub fn fastlang_throw(&mut self, msg: &str) -> Result<Expr, String> {
        Err(format!("{}: {}", "Compile Error".red().bold(), msg))
    }

    pub fn format_expr_for_print(&self, expr: &Expr) -> String {
        match expr {
            Expr::LiteralString(s) => s.clone(),
            Expr::LiteralInt(i) => i.to_string(),
            Expr::LiteralUInt(u) => u.to_string(),
            Expr::LiteralFloat(f) => f.to_string(),
            Expr::LiteralBool(b) => b.to_string(),
            Expr::LiteralChar(c) => c.to_string(),
            Expr::LiteralVoid => "()".to_string(),
            Expr::ArrayLiteral(elems) => {
                let inner: Vec<String> = elems
                    .iter()
                    .map(|e| self.format_expr_for_print(e))
                    .collect();
                format!("[{}]", inner.join(", "))
            }
            Expr::Identifier(id) => {
                if let Some(val) = self.env.get_ref(id) {
                    self.format_expr_for_print(val)
                } else {
                    id.clone()
                }
            }
            _ => format!("{:?}", expr),
        }
    }

    pub fn expr_to_i128(&self, expr: &Expr) -> Result<i128, String> {
        match expr {
            Expr::LiteralInt(i) => Ok(*i),
            Expr::LiteralUInt(u) => Ok(*u as i128),
            Expr::LiteralFloat(f) => Ok(*f as i128),
            Expr::UnaryOp { operator, operand } if operator == "-" => {
                let inner = self.expr_to_i128(operand)?;
                Ok(-inner)
            }
            Expr::Identifier(id) => {
                if let Some(v) = self.env.get_ref(id) {
                    self.expr_to_i128(v)
                } else {
                    Err(format!(
                        "Identifier '{}' is not a constant integer in rand()",
                        id
                    ))
                }
            }
            _ => Err(format!("Non-constant expression '{:?}' in rand()", expr)),
        }
    }

    pub fn expr_to_f64(&self, expr: &Expr) -> Result<f64, String> {
        match expr {
            Expr::LiteralFloat(f) => Ok(*f),
            Expr::LiteralInt(i) => Ok(*i as f64),
            Expr::LiteralUInt(u) => Ok(*u as f64),
            Expr::UnaryOp { operator, operand } if operator == "-" => {
                let inner = self.expr_to_f64(operand)?;
                Ok(-inner)
            }
            Expr::Identifier(id) => {
                if let Some(v) = self.env.get_ref(id) {
                    self.expr_to_f64(v)
                } else {
                    Err(format!(
                        "Identifier '{}' is not a constant float in rand()",
                        id
                    ))
                }
            }
            _ => Err(format!("Non-constant expression '{:?}' in rand()", expr)),
        }
    }

    pub fn fastlang_rand(&mut self, target_type: &BaseType, args: &[Expr]) -> Result<Expr, String> {
        use std::time::{SystemTime, UNIX_EPOCH};
        static SEED_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(123456789);
        let counter = SEED_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut z = nanos.wrapping_add(counter.wrapping_mul(0x9e3779b97f4a7c15));
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        let seed = z ^ (z >> 31);

        let is_float = matches!(target_type, BaseType::Float(_));

        if is_float {
            let (min, max, step) = match args.len() {
                0 => (0.0, 1.0, None),
                1 => {
                    let max = self.expr_to_f64(&args[0])?;
                    (0.0, max, None)
                }
                2 => {
                    let min = self.expr_to_f64(&args[0])?;
                    let max = self.expr_to_f64(&args[1])?;
                    (min, max, None)
                }
                3 => {
                    let min = self.expr_to_f64(&args[0])?;
                    let max = self.expr_to_f64(&args[1])?;
                    let step = self.expr_to_f64(&args[2])?;
                    if step <= 0.0 {
                        return self.fastlang_throw("rand step must be greater than 0");
                    }
                    (min, max, Some(step))
                }
                _ => return self.fastlang_throw("rand takes at most 3 arguments (min, max, step)"),
            };

            let (min, max) = if min > max { (max, min) } else { (min, max) };
            let norm = (seed as f64) / (u64::MAX as f64);
            let val = if let Some(s) = step {
                let num_steps = ((max - min) / s).floor() as i64;
                let step_idx = (norm * ((num_steps + 1) as f64)).floor() as i64;
                min + (step_idx as f64) * s
            } else {
                min + norm * (max - min)
            };
            Ok(Expr::LiteralFloat(val))
        } else {
            let (min, max, step) = match args.len() {
                0 => (0i128, 100i128, None),
                1 => {
                    let max = self.expr_to_i128(&args[0])?;
                    (0i128, max, None)
                }
                2 => {
                    let min = self.expr_to_i128(&args[0])?;
                    let max = self.expr_to_i128(&args[1])?;
                    (min, max, None)
                }
                3 => {
                    let min = self.expr_to_i128(&args[0])?;
                    let max = self.expr_to_i128(&args[1])?;
                    let step = self.expr_to_i128(&args[2])?;
                    if step <= 0 {
                        return self.fastlang_throw("rand step must be greater than 0");
                    }
                    (min, max, Some(step))
                }
                _ => return self.fastlang_throw("rand takes at most 3 arguments (min, max, step)"),
            };

            let (min, max) = if min > max { (max, min) } else { (min, max) };
            let r = (seed as i128).abs();
            let val = if let Some(s) = step {
                let num_steps = (max - min) / s;
                let step_idx = if num_steps > 0 {
                    r % (num_steps + 1)
                } else {
                    0
                };
                min + step_idx * s
            } else {
                let range = max - min + 1;
                if range > 0 {
                    min + (r % range)
                } else {
                    min
                }
            };

            match target_type {
                BaseType::UInt(_) | BaseType::USize => Ok(Expr::LiteralUInt(val as u128)),
                _ => Ok(Expr::LiteralInt(val)),
            }
        }
    }

    pub fn extract_string_from_expr(&mut self, expr: &Expr) -> String {
        let evaled = self.eval_expr(expr).unwrap_or_else(|_| expr.clone());
        match &evaled {
            Expr::LiteralString(s) => s.clone(),
            Expr::Identifier(id) => {
                if let Ok(val) = self.env.get(id) {
                    self.extract_string_from_expr(&val)
                } else {
                    id.clone()
                }
            }
            Expr::New { target, .. } => self.extract_string_from_expr(target),
            Expr::Instantiate { args, .. } => {
                if let Some(first) = args.first() {
                    self.extract_string_from_expr(first)
                } else {
                    "Unknown Error".to_string()
                }
            }
            Expr::BinaryOp { .. } => {
                if let Ok(Expr::LiteralString(s)) = self.eval_expr(&evaled) {
                    return s;
                }
                "Compile-time Error".to_string()
            }
            _ => self.format_expr_for_print(&evaled),
        }
    }

    pub fn extract_clean_blueprint_name(&self, type_str: &str) -> String {
        let mut clean = type_str.trim();
        loop {
            let mut changed = false;
            for prefix in &["blueprint::", "struct::", "class::", "enum::", "fast_std::"] {
                if clean.starts_with(prefix) {
                    clean = &clean[prefix.len()..];
                    changed = true;
                }
            }
            for prefix in &["type<"] {
                if clean.starts_with(prefix) && clean.ends_with('>') {
                    clean = &clean[prefix.len()..clean.len() - 1];
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let base = clean.split('<').next().unwrap_or(clean);
        base.trim_end_matches('*')
            .trim_end_matches('&')
            .trim()
            .to_string()
    }


    pub fn extract_prop_name(&mut self, expr: &Expr) -> String {
        self.extract_prop_name_and_type(expr).0
    }

    pub fn extract_prop_name_and_type(&mut self, expr: &Expr) -> (String, Option<String>) {
        match expr {
            Expr::LiteralString(s) => (s.clone(), None),
            Expr::Identifier(id) => {
                if let Ok(val) = self.env.get(id) {
                    match &val {
                        Expr::LiteralString(s) => (s.clone(), None),
                        Expr::ObjectLiteral(_) | Expr::Instantiate { .. } => {
                            self.extract_prop_name_and_type(&val)
                        }
                        _ => (id.clone(), None),
                    }
                } else {
                    (id.clone(), None)
                }
            }
            Expr::ObjectLiteral(stmts) => {
                let mut name_opt = None;
                let mut type_opt = None;
                for stmt in stmts {
                    match stmt {
                        Stmt::Declaration(Decl::VarDecl { name, value, .. }) => {
                            if name == "name" {
                                name_opt = Some(self.extract_string_from_expr(value));
                            } else if name == "ty" || name == "type" {
                                type_opt = Some(self.extract_type_or_name(value));
                            }
                        }
                        Stmt::ReassignStmt {
                            target: Expr::Identifier(field_name),
                            value,
                            ..
                        } => {
                            if field_name == "name" {
                                name_opt = Some(self.extract_string_from_expr(value));
                            } else if field_name == "ty" || field_name == "type" {
                                type_opt = Some(self.extract_type_or_name(value));
                            }
                        }
                        Stmt::ExpressionStmt(Expr::BinaryOp {
                            left,
                            operator,
                            right,
                        }) if operator == "=" => {
                            if let Expr::Identifier(field_name) = &**left {
                                if field_name == "name" {
                                    name_opt = Some(self.extract_string_from_expr(right));
                                } else if field_name == "ty" || field_name == "type" {
                                    type_opt = Some(self.extract_type_or_name(right));
                                }
                            }
                        }
                        _ => {}
                    }
                }
                (name_opt.unwrap_or_default(), type_opt)
            }
            Expr::Instantiate { args, .. } => {
                for arg in args {
                    let (n, t) = self.extract_prop_name_and_type(arg);
                    if !n.is_empty() {
                        return (n, t);
                    }
                }
                ("".to_string(), None)
            }
            Expr::PropertyAccess { property, .. } => (property.clone(), None),
            _ => (self.extract_string_from_expr(expr), None),
        }
    }

    pub fn fastlang_cast(&mut self, val: &Expr, target_type: &BaseType) -> Result<Expr, String> {
        let resolved = match val {
            Expr::Identifier(id) => {
                if let Ok(v) = self.env.get(id) {
                    v
                } else {
                    val.clone()
                }
            }
            _ => val.clone(),
        };

        if matches!(resolved, Expr::LiteralUndefined) {
            eprintln!("Warning: Compile-time cast on undefined value. Consider using runtime 'as' operator instead.");
            return Ok(Expr::LiteralUndefined);
        }

        match target_type {
            BaseType::Int(_) | BaseType::ISize => match resolved {
                Expr::LiteralInt(i) => Ok(Expr::LiteralInt(i)),
                Expr::LiteralUInt(u) => Ok(Expr::LiteralInt(u as i128)),
                Expr::LiteralFloat(f) => Ok(Expr::LiteralInt(f as i128)),
                Expr::LiteralBool(b) => Ok(Expr::LiteralInt(if b { 1 } else { 0 })),
                Expr::LiteralChar(c) => Ok(Expr::LiteralInt(c as u32 as i128)),
                Expr::LiteralUChar(c) => Ok(Expr::LiteralInt(c as i128)),
                Expr::LiteralString(ref s) => s
                    .trim()
                    .parse::<i128>()
                    .map(Expr::LiteralInt)
                    .map_err(|_| format!("Compile Error: Cannot cast string \"{}\" to int", s)),
                _ => Err(format!("Compile Error: Cannot cast {:?} to int", resolved)),
            },
            BaseType::UInt(_) | BaseType::USize => match resolved {
                Expr::LiteralUInt(u) => Ok(Expr::LiteralUInt(u)),
                Expr::LiteralInt(i) => Ok(Expr::LiteralUInt(i as u128)),
                Expr::LiteralFloat(f) => Ok(Expr::LiteralUInt(f as u128)),
                Expr::LiteralBool(b) => Ok(Expr::LiteralUInt(if b { 1 } else { 0 })),
                Expr::LiteralChar(c) => Ok(Expr::LiteralUInt(c as u32 as u128)),
                Expr::LiteralUChar(c) => Ok(Expr::LiteralUInt(c as u128)),
                _ => Err(format!("Compile Error: Cannot cast {:?} to uint", resolved)),
            },
            BaseType::Float(_) => match resolved {
                Expr::LiteralFloat(f) => Ok(Expr::LiteralFloat(f)),
                Expr::LiteralInt(i) => Ok(Expr::LiteralFloat(i as f64)),
                Expr::LiteralUInt(u) => Ok(Expr::LiteralFloat(u as f64)),
                Expr::LiteralBool(b) => Ok(Expr::LiteralFloat(if b { 1.0 } else { 0.0 })),
                Expr::LiteralChar(c) => Ok(Expr::LiteralFloat(c as u32 as f64)),
                Expr::LiteralUChar(c) => Ok(Expr::LiteralFloat(c as f64)),
                _ => Err(format!(
                    "Compile Error: Cannot cast {:?} to float",
                    resolved
                )),
            },
            BaseType::Bool => match resolved {
                Expr::LiteralBool(b) => Ok(Expr::LiteralBool(b)),
                Expr::LiteralInt(i) => Ok(Expr::LiteralBool(i != 0)),
                Expr::LiteralUInt(u) => Ok(Expr::LiteralBool(u != 0)),
                Expr::LiteralFloat(f) => Ok(Expr::LiteralBool(f != 0.0)),
                Expr::LiteralChar(c) => Ok(Expr::LiteralBool(c != '\0')),
                Expr::LiteralUChar(c) => Ok(Expr::LiteralBool(c != 0)),
                _ => Err(format!("Compile Error: Cannot cast {:?} to bool", resolved)),
            },
            BaseType::Char => match resolved {
                Expr::LiteralChar(c) => Ok(Expr::LiteralChar(c)),
                Expr::LiteralUChar(c) => {
                    let ch = char::from_u32(c).unwrap_or('\0');
                    Ok(Expr::LiteralChar(ch))
                }
                Expr::LiteralInt(i) => {
                    let ch = char::from_u32(i as u32).unwrap_or('\0');
                    Ok(Expr::LiteralChar(ch))
                }
                Expr::LiteralUInt(u) => {
                    let ch = char::from_u32(u as u32).unwrap_or('\0');
                    Ok(Expr::LiteralChar(ch))
                }
                _ => Err(format!("Compile Error: Cannot cast {:?} to char", resolved)),
            },
            BaseType::UChar => match resolved {
                Expr::LiteralUChar(c) => Ok(Expr::LiteralUChar(c)),
                Expr::LiteralChar(c) => Ok(Expr::LiteralUChar(c as u32)),
                Expr::LiteralInt(i) => Ok(Expr::LiteralUChar(i as u32)),
                Expr::LiteralUInt(u) => Ok(Expr::LiteralUChar(u as u32)),
                _ => Err(format!(
                    "Compile Error: Cannot cast {:?} to uchar",
                    resolved
                )),
            },
            BaseType::Str => match resolved {
                Expr::LiteralString(s) => Ok(Expr::LiteralString(s)),
                Expr::LiteralUString(s) => Ok(Expr::LiteralString(s)),
                _ => Err(format!("Compile Error: Cannot cast {:?} to str", resolved)),
            },
            BaseType::UStr => match resolved {
                Expr::LiteralUString(s) => Ok(Expr::LiteralUString(s)),
                Expr::LiteralString(s) => Ok(Expr::LiteralUString(s)),
                _ => Err(format!("Compile Error: Cannot cast {:?} to ustr", resolved)),
            },
            _ => Err(format!(
                "Compile Error: Compile-time cast to '{}' is not supported",
                target_type.as_str()
            )),
        }
    }


    pub fn infer_iterable_element_type(&mut self, iterable_type: &str, iterable: &Expr) -> String {
        let clean_iter = self.extract_clean_blueprint_name(iterable_type);
        if iterable_type.starts_with("array<") && iterable_type.ends_with('>') {
            let inner = &iterable_type[6..iterable_type.len() - 1];
            return self.resolve_type_str(inner.trim());
        }
        if let Some(inner) = iterable_type.strip_suffix("[]") {
            return self.resolve_type_str(inner.trim());
        }
        if iterable_type == "array" {
            if let Expr::ArrayLiteral(elements) = iterable {
                if let Some(first) = elements.first() {
                    let t = self.get_expr_type(first);
                    if t != "unknown" {
                        return t;
                    }
                }
            }
        }
        if let Some(ref env_rc) = self.current_env.clone() {
            let env = env_rc.borrow();
            if let Some(bp) = env.lookup_blueprint(&clean_iter) {
                // Check for iter() method or handle
                if let Some(iter_sig) = bp
                    .methods
                    .get("iter")
                    .or_else(|| bp.handle_signatures.get("iter"))
                {
                    let cursor_type = iter_sig.return_type.as_str();
                    let substituted_cursor = self.substitute_generics_in_type_str(
                        &cursor_type,
                        &bp.generics,
                        iterable_type,
                    );
                    let clean_cursor = self.extract_clean_blueprint_name(&substituted_cursor);
                    if let Some(cursor_bp) = env.lookup_blueprint(&clean_cursor) {
                        if let Some(next_sig) = cursor_bp
                            .methods
                            .get("next")
                            .or_else(|| cursor_bp.handle_signatures.get("next"))
                        {
                            let ret = next_sig.return_type.as_str();
                            let sub_ret = self.substitute_generics_in_type_str(
                                &ret,
                                &cursor_bp.generics,
                                &substituted_cursor,
                            );
                            return self.resolve_type_str(&sub_ret);
                        }
                    }
                }
                // Check for direct next() method or handle
                if let Some(next_sig) = bp
                    .methods
                    .get("next")
                    .or_else(|| bp.handle_signatures.get("next"))
                {
                    let ret = next_sig.return_type.as_str();
                    let sub_ret =
                        self.substitute_generics_in_type_str(&ret, &bp.generics, iterable_type);
                    return self.resolve_type_str(&sub_ret);
                }
            }
        }
        // Fallback if iterable_type has generic parameter (e.g. List<int32>)
        if iterable_type.contains('<') && iterable_type.ends_with('>') {
            let start = iterable_type.find('<').unwrap() + 1;
            let end = iterable_type.rfind('>').unwrap();
            let inner = iterable_type[start..end]
                .split(',')
                .next()
                .unwrap_or("")
                .trim();
            if !inner.is_empty() {
                return self.resolve_type_str(inner);
            }
        }
        "unknown".to_string()
    }

    pub fn extract_type_string(&mut self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::LiteralString(s) if s.starts_with("type<") && s.ends_with('>') => {
                Some(strip_type_wrapper(s).to_string())
            }
            Expr::Call { callee, args, .. } if is_typeof_callee(callee) => {
                args.first().map(|a| self.get_expr_type(a))
            }
            Expr::Identifier(id) => {
                if let Ok(Expr::LiteralString(s)) = self.env.get(id) {
                    if s.starts_with("type<") && s.ends_with('>') {
                        return Some(strip_type_wrapper(&s).to_string());
                    }
                }
                if let Some(t) = self.var_types.get(id) {
                    if t == "type" || t.starts_with("type<") {
                        if let Ok(Expr::LiteralString(s)) = self.env.get(id) {
                            let clean = strip_type_wrapper(s.as_str());
                            if clean != s {
                                return Some(clean.to_string());
                            }
                        }
                        return Some(t.clone());
                    }
                    None
                } else if is_known_type_name(id)
                    || id.ends_with("[]")
                    || (id.contains('<') && id.ends_with('>'))
                    || (self.current_env.is_some() && {
                        let base = id.split('<').next().unwrap_or(id);
                        self.current_env
                            .as_ref()
                            .unwrap()
                            .borrow()
                            .lookup_blueprint(base)
                            .is_some()
                    })
                {
                    Some(self.resolve_type_str(id))
                } else {
                    let resolved = self.resolve_type_str(id);
                    if resolved != *id {
                        Some(resolved)
                    } else {
                        None
                    }
                }
            }
            _ => None,
        }
    }

    pub fn get_expr_type(&mut self, expr: &Expr) -> String {
        let raw = match expr {
            Expr::LiteralInt(_) => "int32".to_string(),
            Expr::LiteralUInt(_) => "uint32".to_string(),
            Expr::LiteralFloat(_) => "float64".to_string(),
            Expr::LiteralString(s) if s.starts_with("type<") && s.ends_with('>') => {
                strip_type_wrapper(s).to_string()
            }
            Expr::LiteralString(_) => "str".to_string(),
            Expr::LiteralUString(_) => "ustr".to_string(),
            Expr::LiteralBool(_) => "bool".to_string(),
            Expr::LiteralChar(_) => "char".to_string(),
            Expr::LiteralUChar(_) => "uchar".to_string(),
            Expr::LiteralVoid => "void".to_string(),
            Expr::This => {
                if let Some(ref t) = self.current_type {
                    t.clone()
                } else {
                    "unknown".to_string()
                }
            }
            Expr::Identifier(id) => {
                let ty = if is_known_type_name(id) {
                    id.clone()
                } else if let Some(t) = self.var_types.get(id) {
                    t.clone()
                } else if let Ok(val) = self.env.get(id) {
                    let val_ty = match &val {
                        Expr::LiteralString(s) if s.starts_with("type<") && s.ends_with('>') => {
                            strip_type_wrapper(s).to_string()
                        }
                        Expr::Identifier(other_id) if other_id == id => "unknown".to_string(),
                        _ => self.get_expr_type(&val),
                    };
                    if val_ty != "unknown" {
                        val_ty
                    } else if let Some(ref env_rc) = self.current_env {
                        if env_rc.borrow().lookup_blueprint(id).is_some() {
                            id.clone()
                        } else if let Some(info) = env_rc.borrow().lookup(id) {
                            if let SymbolKind::Variable { type_node, .. } = &info.kind {
                                type_node.as_str()
                            } else {
                                "unknown".to_string()
                            }
                        } else {
                            "unknown".to_string()
                        }
                    } else {
                        "unknown".to_string()
                    }
                } else if let Some(ref env_rc) = self.current_env {
                    if env_rc.borrow().lookup_blueprint(id).is_some() {
                        id.clone()
                    } else if let Some(info) = env_rc.borrow().lookup(id) {
                        if let SymbolKind::Variable { type_node, .. } = &info.kind {
                            type_node.as_str()
                        } else {
                            "unknown".to_string()
                        }
                    } else {
                        "unknown".to_string()
                    }
                } else {
                    "unknown".to_string()
                };
                self.resolve_type_str(&ty)
            }
            Expr::UnaryOp { operator, operand } if operator == "&" => {
                format!("{}*", self.get_expr_type(operand))
            }
            Expr::PropertyAccess { object, property } => {
                let obj_type = self.get_expr_type(object);
                let clean_obj = self.extract_clean_blueprint_name(&obj_type);
                if let Some(ref env_rc) = self.current_env.clone() {
                    let env = env_rc.borrow();
                    if let Some(bp) = env.lookup_blueprint(&clean_obj) {
                        if let Some(ft) = bp.fields.get(property) {
                            let ft_str = ft.as_str();
                            let substituted = self.substitute_generics_in_type_str(
                                &ft_str,
                                &bp.generics,
                                &obj_type,
                            );
                            return self.resolve_type_str(&substituted);
                        }
                        if let Some(sig) = bp
                            .methods
                            .get(property)
                            .or_else(|| bp.handle_signatures.get(property))
                        {
                            let ret_str = sig.return_type.as_str();
                            let substituted = self.substitute_generics_in_type_str(
                                &ret_str,
                                &bp.generics,
                                &obj_type,
                            );
                            return self.resolve_type_str(&substituted);
                        }
                    }
                }
                "unknown".to_string()
            }
            Expr::ArrayLiteral(elements) => {
                if let Some(first) = elements.first() {
                    let elem_t = self.get_expr_type(first);
                    if elem_t != "unknown" {
                        format!("array<{}>", elem_t)
                    } else {
                        "array".to_string()
                    }
                } else {
                    "array".to_string()
                }
            }
            Expr::Instantiate { target, .. } => match &**target {
                Expr::Identifier(id) => id.clone(),
                _ => self.get_expr_type(target),
            },
            Expr::New { type_node, .. } => type_node.as_str(),
            Expr::Cast { target_type, .. } => target_type.as_str(),
            Expr::IfExpr {
                then_branch,
                else_branch,
                ..
            } => {
                let t = self.get_expr_type(then_branch);
                if t != "unknown" {
                    t
                } else {
                    self.get_expr_type(else_branch)
                }
            }
            Expr::Call { callee, .. } => {
                let fn_name = match &**callee {
                    Expr::Identifier(id) => id.as_str(),
                    Expr::PropertyAccess { property, .. } => property.as_str(),
                    Expr::NamespaceAccess { property, .. } => {
                        if let Expr::Identifier(p) = &**property {
                            p.as_str()
                        } else {
                            ""
                        }
                    }
                    _ => "",
                };
                if let Some(sigs) = self.fn_overloads.get(fn_name) {
                    if let Some(sig) = sigs.first() {
                        return self.resolve_type_str(&sig.return_type.as_str());
                    }
                }
                if let Some(ref env_rc) = self.current_env {
                    if let Some(info) = env_rc.borrow().lookup(fn_name) {
                        if let SymbolKind::Function { return_type, .. } = &info.kind {
                            return self.resolve_type_str(&return_type.as_str());
                        }
                    }
                }
                "unknown".to_string()
            }
            _ => "unknown".to_string(),
        };
        self.resolve_type_str(&raw)
    }


    pub fn try_expand_access_handle(
        &mut self,
        object: &Expr,
        prop_name: &str,
        handle_name: &str,
    ) -> Result<Option<Expr>, String> {
        let obj_type = self.get_expr_type(object);
        if obj_type == "unknown" {
            return Ok(None);
        }
        let clean_obj = self.extract_clean_blueprint_name(&obj_type);
        let env_rc = match self.current_env.clone() {
            Some(e) => e,
            None => return Ok(None),
        };
        let bp = match env_rc.borrow().lookup_blueprint(&clean_obj) {
            Some(b) => b,
            None => return Ok(None),
        };
        if handle_name == "property_access" {
            if bp.fields.contains_key(prop_name)
                || bp.methods.contains_key(prop_name)
                || bp.handle_signatures.contains_key(prop_name)
            {
                return Ok(None);
            }
        } else if handle_name == "handle_access"
            && (bp.handle_signatures.contains_key(prop_name) || bp.methods.contains_key(prop_name)) {
                return Ok(None);
            }

        let sig = match bp.handle_signatures.get(handle_name) {
            Some(s) => s,
            None => return Ok(None),
        };

        let param_ty = self.extract_clean_blueprint_name(&sig.params[0].type_node.as_str());
        let ret_ty = sig.return_type.as_str();
        let is_access_sig = sig.params.len() == 1
            && (param_ty == "MemberType" || param_ty == "unknown")
            && ret_ty.ends_with('*');

        if !is_access_sig {
            return Ok(None);
        }

        let fn_key = format!("{}::{}", clean_obj, handle_name);
        let body = match self.functions.get(&fn_key) {
            Some((_, _, b)) => b.clone(),
            None => match self.functions.get(handle_name) {
                Some((_, _, b)) => b.clone(),
                None => return Ok(None),
            },
        };

        for stmt in &body {
            if let Stmt::ReturnStmt(Some(ret_expr)) = stmt {
                let target_opt: Option<&Expr> = match ret_expr {
                    Expr::Call { callee, args, .. } => {
                        let is_compile_access = match &**callee {
                            Expr::NamespaceAccess {
                                namespace,
                                property,
                            } => {
                                namespace == "@compile"
                                    && matches!(&**property, Expr::Identifier(p) if p == "access")
                            }
                            Expr::Identifier(id) => id == "@compile::access" || id == "access",
                            _ => false,
                        };
                        if is_compile_access && args.len() >= 2 {
                            Some(&args[0])
                        } else {
                            None
                        }
                    }
                    Expr::UnaryOp { operator, operand } if operator == "&" => {
                        if let Expr::PropertyAccess { object, .. } = &**operand {
                            Some(&**object)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                if let Some(target) = target_opt {
                    let substituted_target = substitute_this_in_expr(target, object);
                    let target_type = self.get_expr_type(&substituted_target);
                    let clean_target = self.extract_clean_blueprint_name(&target_type);
                    if let Some(target_bp) = env_rc.borrow().lookup_blueprint(&clean_target) {
                        let has_member = target_bp.fields.contains_key(prop_name)
                            || target_bp.methods.contains_key(prop_name)
                            || target_bp.handle_signatures.contains_key(prop_name);
                        if !has_member {
                            return self
                                .fastlang_throw(&format!(
                                    "Member '{}' not found on type '{}'",
                                    prop_name, clean_target
                                ))
                                .map(|_| None);
                        }
                    }
                    return Ok(Some(Expr::PropertyAccess {
                        object: Box::new(substituted_target),
                        property: prop_name.to_string(),
                    }));
                }
            }
        }

        Ok(None)
    }

}

