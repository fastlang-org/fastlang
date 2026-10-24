use crate::frontend::parser::ast::*;

pub(crate) fn type_to_cpp(t: &BaseType) -> String {
    match t {
        BaseType::Int(Size::S8) => "int8_t".to_string(),
        BaseType::Int(Size::S16) => "int16_t".to_string(),
        BaseType::Int(Size::S32) => "int32_t".to_string(),
        BaseType::Int(Size::S64) => "int64_t".to_string(),
        BaseType::Int(Size::S128) => "__int128".to_string(),
        BaseType::UInt(Size::S8) => "uint8_t".to_string(),
        BaseType::UInt(Size::S16) => "uint16_t".to_string(),
        BaseType::UInt(Size::S32) => "uint32_t".to_string(),
        BaseType::UInt(Size::S64) => "uint64_t".to_string(),
        BaseType::UInt(Size::S128) => "unsigned __int128".to_string(),
        BaseType::USize => "size_t".to_string(),
        BaseType::ISize => "ptrdiff_t".to_string(),
        BaseType::Float(Size::S32) => "float".to_string(),
        BaseType::Float(Size::S64) => "double".to_string(),
        BaseType::Float(Size::S128) => "long double".to_string(),
        BaseType::Char => "char".to_string(),
        BaseType::UChar => "char32_t".to_string(),
        BaseType::Str => "char*".to_string(),
        BaseType::UStr => "char32_t*".to_string(),
        BaseType::Bool => "bool".to_string(),
        BaseType::Void => "void".to_string(),
        BaseType::Block { name, .. } => {
            if !name.is_empty() {
                format!("__block_type_{}", name)
            } else {
                "void".to_string()
            }
        }
        BaseType::Machine { name, .. } => {
            if !name.is_empty() {
                format!("__machine_type_{}", name)
            } else {
                "fastlang_fn_ptr<void>".to_string()
            }
        }
        BaseType::Class { name, generics, .. } | BaseType::Struct { name, generics, .. } => {
            if generics.is_empty() {
                name.clone()
            } else {
                let gen_strs: Vec<String> = generics.iter().map(type_to_cpp).collect();
                format!("{}<{}>", name, gen_strs.join(", "))
            }
        }
        BaseType::Enum { name, generics, .. } | BaseType::Blueprint { name, generics, .. } => {
            if name == "array" {
                if generics.is_empty() {
                    "fast_std::array<void>".to_string()
                } else {
                    let gen_strs: Vec<String> = generics.iter().map(type_to_cpp).collect();
                    format!("fast_std::array<{}>", gen_strs.join(", "))
                }
            } else if generics.is_empty() {
                name.clone()
            } else {
                let gen_strs: Vec<String> = generics.iter().map(type_to_cpp).collect();
                format!("{}<{}>", name, gen_strs.join(", "))
            }
        }
        BaseType::GenericParam(name) => name.trim_start_matches('.').to_string(),
        BaseType::Generic(inner_vec) => {
            let strs: Vec<String> = inner_vec.iter().map(type_to_cpp).collect();
            strs.join(", ")
        }

        BaseType::RawPointer(inner) => format!("{}*", type_to_cpp(inner)),
        BaseType::Address(inner) => format!("const {}&", type_to_cpp(inner)),
        BaseType::Type(_) => "type".to_string(),
        BaseType::Flag => "bool".to_string(),
        BaseType::Array { base_type, .. } => format!("fast_std::array<{}>", type_to_cpp(base_type)),
        BaseType::Fn {
            return_type,
            params,
            ..
        }
        | BaseType::Micro {
            return_type,
            params,
            ..
        }
        | BaseType::Lambda {
            return_type,
            params,
            ..
        }
        | BaseType::Method {
            return_type,
            params,
            ..
        } => {
            if return_type.as_ref() == &BaseType::Unknown {
                "auto".to_string()
            } else {
                let ret = type_to_cpp(return_type);
                let p_types: Vec<String> = params.iter().map(type_to_cpp).collect();
                if p_types.is_empty() {
                    format!("fastlang_fn_ptr<{}>", ret)
                } else {
                    format!("fastlang_fn_ptr<{}, {}>", ret, p_types.join(", "))
                }
            }
        }
        BaseType::Function {
            return_type,
            params,
            ..
        } => {
            if return_type.as_ref() == &BaseType::Unknown {
                "auto".to_string()
            } else {
                let ret = type_to_cpp(return_type);
                let p_types: Vec<String> = params.iter().map(type_to_cpp).collect();
                if p_types.is_empty() {
                    format!("fastlang_fn_ptr<{}>", ret)
                } else {
                    format!("fastlang_fn_ptr<{}, {}>", ret, p_types.join(", "))
                }
            }
        }
        BaseType::Identifier => "const char*".to_string(),
        BaseType::Expr => "auto".to_string(),
        BaseType::Object => "auto&&".to_string(),
        BaseType::Number => "auto".to_string(),
        BaseType::TypeExpr(_) => "auto".to_string(),
        BaseType::Unknown | BaseType::Auto => "auto".to_string(),
        _ => t.as_str(),
    }
}

pub(crate) fn is_raw_pointer(type_node: &BaseType, cpp_type: &str) -> bool {
    matches!(type_node, BaseType::RawPointer(_))
        || (cpp_type.ends_with('*')
            && cpp_type != "char*"
            && cpp_type != "const char*"
            && cpp_type != "char32_t*"
            && cpp_type != "const char32_t*"
            && !matches!(type_node, BaseType::Str | BaseType::UStr))
}

pub(crate) fn extract_generic_type_args(type_str: &str) -> Vec<String> {
    let start = match type_str.find('<') {
        Some(idx) => idx + 1,
        None => return Vec::new(),
    };
    let end = match type_str.rfind('>') {
        Some(idx) => idx,
        None => return Vec::new(),
    };
    if start >= end {
        return Vec::new();
    }
    let inner = &type_str[start..end];
    let mut args = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    for ch in inner.chars() {
        match ch {
            '<' => {
                depth += 1;
                current.push(ch);
            }
            '>' => {
                if depth > 0 {
                    depth -= 1;
                }
                current.push(ch);
            }
            ',' if depth == 0 => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    args.push(trimmed);
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        args.push(trimmed);
    }
    args
}
pub(crate) fn expr_uses_flag(expr: &Expr, flag: &Flag) -> bool {
    let flag_str = flag.as_str();
    match expr {
        Expr::Identifier(id) => Flag::from_str(id) == *flag || id == &flag_str,
        Expr::PropertyAccess { property, .. } => Flag::from_str(property) == *flag || property == &flag_str,
        Expr::BinaryOp { left, right, .. } => {
            expr_uses_flag(left, flag) || expr_uses_flag(right, flag)
        }
        Expr::UnaryOp { operand, .. } => expr_uses_flag(operand, flag),
        Expr::Call { callee, args, .. } => {
            expr_uses_flag(callee, flag) || args.iter().any(|a| expr_uses_flag(a, flag))
        }
        _ => false,
    }
}

pub(crate) fn stmts_use_flag(stmts: &[Stmt], flag: &Flag) -> bool {
    for s in stmts {
        match s {
            Stmt::ReassignStmt { target, value, .. } => {
                if expr_uses_flag(target, flag) || expr_uses_flag(value, flag) {
                    return true;
                }
            }
            Stmt::ExpressionStmt(expr) => {
                if expr_uses_flag(expr, flag) {
                    return true;
                }
            }
            Stmt::YieldStmt(_) if *flag == Flag::Yielded || *flag == Flag::HasYield => {
                return true;
            }
            Stmt::LeaveStmt if *flag == Flag::Leaved || *flag == Flag::HasLeave => {
                return true;
            }
            Stmt::IfStmt {
                condition,
                then_block,
                else_block,
                ..
            } => {
                if expr_uses_flag(condition, flag) || stmts_use_flag(then_block, flag) {
                    return true;
                }
                if let Some(eb) = else_block {
                    if stmts_use_flag(eb, flag) {
                        return true;
                    }
                }
            }
            Stmt::WhileStmt { condition, body } | Stmt::DoWhileStmt { condition, body } => {
                if expr_uses_flag(condition, flag) {
                    return true;
                }
                if let EitherBlock::Inline(b) = body {
                    if stmts_use_flag(b, flag) {
                        return true;
                    }
                }
            }
            Stmt::LoopStmt { count, body } => {
                if let Some(c) = count {
                    if expr_uses_flag(c, flag) {
                        return true;
                    }
                }
                if let EitherBlock::Inline(b) = body {
                    if stmts_use_flag(b, flag) {
                        return true;
                    }
                }
            }
            Stmt::ForStmt { body, .. } | Stmt::ForInStmt { body, .. } => {
                if let EitherBlock::Inline(b) = body {
                    if stmts_use_flag(b, flag) {
                        return true;
                    }
                }
            }
            Stmt::SwitchStmt { cases, .. } => {
                if stmts_use_flag(cases, flag) {
                    return true;
                }
            }
            Stmt::CaseStmt { body, .. } => {
                if stmts_use_flag(body, flag) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

pub(crate) fn decls_use_flag(decls: &[Decl], flag: &Flag) -> bool {
    for d in decls {
        match d {
            Decl::FnDecl { body, .. } => {
                if stmts_use_flag(body, flag) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

pub(crate) fn collect_block_vars(stmts: &[Stmt], vars: &mut Vec<(String, BaseType)>) {
    for s in stmts {
        match s {
            Stmt::Declaration(Decl::VarDecl {
                name, type_node, ..
            }) => {
                if !vars.iter().any(|(n, _)| n == name) {
                    vars.push((name.clone(), type_node.clone()));
                }
            }
            Stmt::ForStmt { init, body, .. } => {
                if let Some(i) = init {
                    if let Stmt::Declaration(Decl::VarDecl {
                        name, type_node, ..
                    }) = &**i
                    {
                        if !vars.iter().any(|(n, _)| n == name) {
                            vars.push((name.clone(), type_node.clone()));
                        }
                    }
                }
                if let EitherBlock::Inline(inner) = body {
                    collect_block_vars(inner, vars);
                }
            }
            Stmt::WhileStmt { body, .. }
            | Stmt::DoWhileStmt { body, .. }
            | Stmt::LoopStmt { body, .. } => {
                if let EitherBlock::Inline(inner) = body {
                    collect_block_vars(inner, vars);
                }
            }
            Stmt::IfStmt {
                then_block,
                else_block,
                ..
            } => {
                collect_block_vars(then_block, vars);
                if let Some(eb) = else_block {
                    collect_block_vars(eb, vars);
                }
            }
            Stmt::Block(inner) | Stmt::ThisBlock(inner) => {
                collect_block_vars(inner, vars);
            }
            Stmt::SwitchStmt { cases, .. } => {
                for c in cases {
                    if let Stmt::CaseStmt { body, .. } = c {
                        collect_block_vars(body, vars);
                    }
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn cpp_safe_name(name: &str) -> String {
    let unescaped = name.strip_prefix('$').unwrap_or(name);
    if let Some(stripped) = unescaped.strip_prefix('@') {
        return format!("_fl_at_{}", stripped);
    }
    match unescaped {
        "throw" => "_throw".to_string(),
        "default" => "fastlang_handle_default".to_string(),
        "delete" => "_delete".to_string(),
        "new" => "_new".to_string(),
        "switch" => "_switch".to_string(),
        "case" => "_case".to_string(),
        "catch" => "_catch".to_string(),
        "try" => "_try".to_string(),
        "class" => "_class".to_string(),
        "struct" => "_struct".to_string(),
        "union" => "_union".to_string(),
        "enum" => "_enum".to_string(),
        "template" => "_template".to_string(),
        "typename" => "_typename".to_string(),
        "public" => "_public".to_string(),
        "private" => "_private".to_string(),
        "protected" => "_protected".to_string(),
        "virtual" => "_virtual".to_string(),
        "static" => "_static".to_string(),
        "const" => "_const".to_string(),
        "auto" => "_auto".to_string(),
        "void" => "_void".to_string(),
        "int" => "_int".to_string(),
        "float" => "_float".to_string(),
        "double" => "_double".to_string(),
        "char" => "_char".to_string(),
        "bool" => "_bool".to_string(),
        "while" => "_while".to_string(),
        "for" => "_for".to_string(),
        "do" => "_do".to_string(),
        "if" => "_if".to_string(),
        "else" => "_else".to_string(),
        "return" => "_return".to_string(),
        "break" => "_break".to_string(),
        "continue" => "_continue".to_string(),
        "goto" => "_goto".to_string(),
        "sizeof" => "_sizeof".to_string(),
        "typedef" => "_typedef".to_string(),
        other => {
            if name.starts_with('$') {
                format!("_f_{}", other)
            } else {
                other.to_string()
            }
        }
    }
}
