use super::*;
use std::collections::HashMap;
use crate::frontend::parser::ast::*;

pub fn is_compile_condition(expr: &Expr) -> bool {
    match expr {
        Expr::Call { callee, args, .. } => {
            if let Expr::PropertyAccess { object, property } = &**callee {
                if property.starts_with("has_")
                    || property.starts_with("is_")
                    || property == "printable"
                    || property == "throwable"
                    || property == "shareable"
                    || property == "copyable"
                    || property == "castable"
                    || property == "castable_to"
                {
                    return true;
                }
                if is_compile_condition(object) {
                    return true;
                }
            }
            if let Expr::Identifier(id) = &**callee {
                if id == "typeof" || id == "sizeof" {
                    return true;
                }
                if id.starts_with("@compile::") {
                    if id == "@compile::backend" || id == "@compile::system" {
                        return false;
                    }
                    return true;
                }
            }
            if let Expr::NamespaceAccess {
                namespace,
                property,
            } = &**callee
            {
                if namespace == "@compile" {
                    if let Expr::Identifier(p) = &**property {
                        if p == "backend" || p == "system" {
                            return false;
                        }
                    }
                    return true;
                }
            }
            args.iter().any(is_compile_condition)
        }
        Expr::PropertyAccess { object, property } => {
            if property.starts_with("has_")
                || property.starts_with("is_")
                || property == "printable"
                || property == "throwable"
                || property == "shareable"
                || property == "copyable"
                || property == "castable"
            {
                return true;
            }
            is_compile_condition(object)
        }
        Expr::UnaryOp { operand, .. } => is_compile_condition(operand),
        Expr::BinaryOp { left, right, .. } => {
            is_compile_condition(left) || is_compile_condition(right)
        }
        _ => false,
    }
}

pub fn substitute_expr(expr: &Expr, param_map: &HashMap<String, Expr>) -> Expr {
    match expr {
        Expr::Identifier(id) => {
            if let Some(replacement) = param_map.get(id) {
                replacement.clone()
            } else {
                Expr::Identifier(id.clone())
            }
        }
        Expr::LiteralInt(i) => Expr::LiteralInt(*i),
        Expr::LiteralUInt(u) => Expr::LiteralUInt(*u),
        Expr::LiteralFloat(f) => Expr::LiteralFloat(*f),
        Expr::LiteralString(s) => Expr::LiteralString(s.clone()),
        Expr::LiteralUString(s) => Expr::LiteralUString(s.clone()),
        Expr::LiteralBool(b) => Expr::LiteralBool(*b),
        Expr::LiteralChar(c) => Expr::LiteralChar(*c),
        Expr::LiteralUChar(c) => Expr::LiteralUChar(*c),
        Expr::LiteralVoid => Expr::LiteralVoid,
        Expr::LiteralUndefined => Expr::LiteralUndefined,
        Expr::Super => Expr::Super,
        Expr::This => Expr::This,
        Expr::Global => Expr::Global,
        Expr::Default(d) => Expr::Default(d.clone()),
        Expr::ArrayLiteral(elems) => Expr::ArrayLiteral(
            elems
                .iter()
                .map(|e| substitute_expr(e, param_map))
                .collect(),
        ),
        Expr::Spread(e) => Expr::Spread(Box::new(substitute_expr(e, param_map))),
        Expr::ObjectLiteral(stmts) => Expr::ObjectLiteral(
            stmts
                .iter()
                .map(|s| substitute_stmt(s, param_map))
                .collect(),
        ),
        Expr::Instantiate { target, args } => Expr::Instantiate {
            target: Box::new(substitute_expr(target, param_map)),
            args: args.iter().map(|a| substitute_expr(a, param_map)).collect(),
        },
        Expr::ArrayAllocate {
            type_node,
            size,
            length,
        } => Expr::ArrayAllocate {
            type_node: type_node.clone(),
            size: Box::new(substitute_expr(size, param_map)),
            length: length
                .as_ref()
                .map(|l| Box::new(substitute_expr(l, param_map))),
        },
        Expr::New { type_node, target } => Expr::New {
            type_node: type_node.clone(),
            target: Box::new(substitute_expr(target, param_map)),
        },
        Expr::UnaryOp { operator, operand } => Expr::UnaryOp {
            operator: operator.clone(),
            operand: Box::new(substitute_expr(operand, param_map)),
        },
        Expr::Cast {
            expr: inner,
            target_type,
        } => Expr::Cast {
            expr: Box::new(substitute_expr(inner, param_map)),
            target_type: target_type.clone(),
        },
        Expr::IndexAccess { object, indices } => Expr::IndexAccess {
            object: Box::new(substitute_expr(object, param_map)),
            indices: indices
                .iter()
                .map(|i| substitute_expr(i, param_map))
                .collect(),
        },
        Expr::Call {
            callee,
            generics,
            args,
        } => Expr::Call {
            callee: Box::new(substitute_expr(callee, param_map)),
            generics: generics.clone(),
            args: args.iter().map(|a| substitute_expr(a, param_map)).collect(),
        },
        Expr::MacroCall { callee, args } => Expr::MacroCall {
            callee: Box::new(substitute_expr(callee, param_map)),
            args: args.iter().map(|a| substitute_expr(a, param_map)).collect(),
        },
        Expr::PropertyAccess { object, property } => Expr::PropertyAccess {
            object: Box::new(substitute_expr(object, param_map)),
            property: property.clone(),
        },
        Expr::HandleCall {
            object,
            handle_name,
            args,
        } => Expr::HandleCall {
            object: Box::new(substitute_expr(object, param_map)),
            handle_name: handle_name.clone(),
            args: args.iter().map(|a| substitute_expr(a, param_map)).collect(),
        },
        Expr::NamespaceAccess {
            namespace,
            property,
        } => Expr::NamespaceAccess {
            namespace: namespace.clone(),
            property: Box::new(substitute_expr(property, param_map)),
        },
        Expr::BinaryOp {
            left,
            operator,
            right,
        } => Expr::BinaryOp {
            left: Box::new(substitute_expr(left, param_map)),
            operator: operator.clone(),
            right: Box::new(substitute_expr(right, param_map)),
        },
        Expr::PrefixUpdate { operator, right } => Expr::PrefixUpdate {
            operator: operator.clone(),
            right: Box::new(substitute_expr(right, param_map)),
        },
        Expr::PostfixUpdate { left, operator } => Expr::PostfixUpdate {
            left: Box::new(substitute_expr(left, param_map)),
            operator: operator.clone(),
        },
        Expr::Lambda {
            params,
            return_type,
            body,
        } => Expr::Lambda {
            params: params.clone(),
            return_type: return_type.clone(),
            body: body.iter().map(|s| substitute_stmt(s, param_map)).collect(),
        },
        Expr::IfExpr {
            condition,
            then_branch,
            else_branch,
        } => Expr::IfExpr {
            condition: Box::new(substitute_expr(condition, param_map)),
            then_branch: Box::new(substitute_expr(then_branch, param_map)),
            else_branch: Box::new(substitute_expr(else_branch, param_map)),
        },
        Expr::BlockExpr {
            statements,
            final_expr,
        } => Expr::BlockExpr {
            statements: statements
                .iter()
                .map(|s| substitute_stmt(s, param_map))
                .collect(),
            final_expr: final_expr
                .as_ref()
                .map(|f| Box::new(substitute_expr(f, param_map))),
        },
        Expr::QuestionMark(inner) => {
            Expr::QuestionMark(Box::new(substitute_expr(inner, param_map)))
        }
        Expr::ForeignBlock { kind, chunks } => Expr::ForeignBlock {
            kind: kind.clone(),
            chunks: chunks
                .iter()
                .map(|c| match c {
                    ForeignChunk::Static(s) => ForeignChunk::Static(s.clone()),
                    ForeignChunk::Interpolation(e) => {
                        ForeignChunk::Interpolation(Box::new(substitute_expr(e, param_map)))
                    }
                    ForeignChunk::Statements(stmts) => ForeignChunk::Statements(
                        stmts.iter().map(|s| substitute_stmt(s, param_map)).collect(),
                    ),
                })
                .collect(),
        },
        Expr::InPlaceBlock { body } => Expr::InPlaceBlock {
            body: body.iter().map(|s| substitute_stmt(s, param_map)).collect(),
        },
    }
}

pub fn substitute_either_block(
    block: &EitherBlock,
    param_map: &HashMap<String, Expr>,
) -> EitherBlock {
    match block {
        EitherBlock::Inline(stmts) => EitherBlock::Inline(
            stmts
                .iter()
                .map(|s| substitute_stmt(s, param_map))
                .collect(),
        ),
        EitherBlock::External(e) => EitherBlock::External(substitute_expr(e, param_map)),
    }
}

pub fn substitute_stmt(stmt: &Stmt, param_map: &HashMap<String, Expr>) -> Stmt {
    match stmt {
        Stmt::Declaration(decl) => Stmt::Declaration(decl.clone()),
        Stmt::Block(stmts) => Stmt::Block(
            stmts
                .iter()
                .map(|s| substitute_stmt(s, param_map))
                .collect(),
        ),
        Stmt::ThisBlock(stmts) => Stmt::ThisBlock(
            stmts
                .iter()
                .map(|s| substitute_stmt(s, param_map))
                .collect(),
        ),
        Stmt::CompileValidation { body, args } => Stmt::CompileValidation {
            body: body.iter().map(|s| substitute_stmt(s, param_map)).collect(),
            args: args.iter().map(|a| substitute_expr(a, param_map)).collect(),
        },
        Stmt::CaseStmt { option, set, body } => Stmt::CaseStmt {
            option: substitute_expr(option, param_map),
            set: substitute_expr(set, param_map),
            body: body.iter().map(|s| substitute_stmt(s, param_map)).collect(),
        },
        Stmt::SwitchStmt {
            name,
            condition,
            cases,
        } => Stmt::SwitchStmt {
            name: name.clone(),
            condition: substitute_expr(condition, param_map),
            cases: cases
                .iter()
                .map(|s| substitute_stmt(s, param_map))
                .collect(),
        },
        Stmt::ReturnStmt(opt_expr) => {
            Stmt::ReturnStmt(opt_expr.as_ref().map(|e| substitute_expr(e, param_map)))
        }
        Stmt::ForIn {
            item_decl,
            iterable,
            body,
        } => Stmt::ForIn {
            item_decl: Box::new(substitute_stmt(item_decl, param_map)),
            iterable: substitute_expr(iterable, param_map),
            body: body.iter().map(|s| substitute_stmt(s, param_map)).collect(),
        },
        Stmt::BreakStmt => Stmt::BreakStmt,
        Stmt::ContinueStmt => Stmt::ContinueStmt,
        Stmt::LeaveStmt => Stmt::LeaveStmt,
        Stmt::YieldStmt(opt_expr) => {
            Stmt::YieldStmt(opt_expr.as_ref().map(|e| substitute_expr(e, param_map)))
        }
        Stmt::CallStmt(e) => Stmt::CallStmt(substitute_expr(e, param_map)),
        Stmt::ExpressionStmt(e) => Stmt::ExpressionStmt(substitute_expr(e, param_map)),
        Stmt::ThrowStmt(e) => Stmt::ThrowStmt(substitute_expr(e, param_map)),
        Stmt::TryCatchStmt {
            try_block,
            catches,
        } => Stmt::TryCatchStmt {
            try_block: try_block
                .iter()
                .map(|s| substitute_stmt(s, param_map))
                .collect(),
            catches: catches
                .iter()
                .map(|c| crate::frontend::parser::ast::CatchClause {
                    param: c.param.clone(),
                    param_type: c.param_type.clone(),
                    body: c.body.iter().map(|s| substitute_stmt(s, param_map)).collect(),
                })
                .collect(),
        },
        Stmt::ReassignStmt { target, op, value } => Stmt::ReassignStmt {
            target: substitute_expr(target, param_map),
            op: op.clone(),
            value: substitute_expr(value, param_map),
        },
        Stmt::AddPropertyStmt { kind_name, value } => Stmt::AddPropertyStmt {
            kind_name: kind_name.clone(),
            value: substitute_expr(value, param_map),
        },
        Stmt::GotoStmt(e) => Stmt::GotoStmt(substitute_expr(e, param_map)),
        Stmt::IfStmt {
            condition,
            then_block,
            else_block,
        } => Stmt::IfStmt {
            condition: substitute_expr(condition, param_map),
            then_block: then_block
                .iter()
                .map(|s| substitute_stmt(s, param_map))
                .collect(),
            else_block: else_block
                .as_ref()
                .map(|eb| eb.iter().map(|s| substitute_stmt(s, param_map)).collect()),
        },
        Stmt::LoopStmt { count, body } => Stmt::LoopStmt {
            count: count.as_ref().map(|c| substitute_expr(c, param_map)),
            body: substitute_either_block(body, param_map),
        },
        Stmt::WhileStmt { condition, body } => Stmt::WhileStmt {
            condition: substitute_expr(condition, param_map),
            body: substitute_either_block(body, param_map),
        },
        Stmt::DoWhileStmt { body, condition } => Stmt::DoWhileStmt {
            body: substitute_either_block(body, param_map),
            condition: substitute_expr(condition, param_map),
        },
        Stmt::ForStmt {
            init,
            condition,
            increment,
            body,
        } => Stmt::ForStmt {
            init: init
                .as_ref()
                .map(|i| Box::new(substitute_stmt(i, param_map))),
            condition: condition.as_ref().map(|c| substitute_expr(c, param_map)),
            increment: increment
                .as_ref()
                .map(|inc| Box::new(substitute_stmt(inc, param_map))),
            body: substitute_either_block(body, param_map),
        },
        Stmt::ForInStmt {
            item,
            iterable,
            body,
        } => Stmt::ForInStmt {
            item: Box::new(substitute_stmt(item, param_map)),
            iterable: substitute_expr(iterable, param_map),
            body: substitute_either_block(body, param_map),
        },
        Stmt::DelStmt { target, is_array } => Stmt::DelStmt {
            target: substitute_expr(target, param_map),
            is_array: *is_array,
        },
        Stmt::UsingStmt(s) => Stmt::UsingStmt(s.clone()),
        Stmt::SetNamespace(s) => Stmt::SetNamespace(s.clone()),
        Stmt::Splice(stmts) => Stmt::Splice(
            stmts.iter().map(|s| substitute_stmt(s, param_map)).collect(),
        ),
    }
}

pub(crate) fn expr_references_params(expr: &Expr, params: &[Param], generics: &[BaseType]) -> bool {
    match expr {
        Expr::Identifier(id) => {
            params.iter().any(|p| &p.name == id)
                || generics.iter().any(|g| {
                    let s = g.as_str();
                    s == *id || format!("...{}", s) == *id
                })
        }
        Expr::Call { callee, args, .. } => {
            expr_references_params(callee, params, generics)
                || args
                    .iter()
                    .any(|a| expr_references_params(a, params, generics))
        }
        Expr::PropertyAccess { object, .. } => expr_references_params(object, params, generics),
        Expr::NamespaceAccess {
            namespace: _,
            property,
        } => expr_references_params(property, params, generics),
        Expr::UnaryOp { operand, .. } => expr_references_params(operand, params, generics),
        Expr::BinaryOp { left, right, .. } => {
            expr_references_params(left, params, generics)
                || expr_references_params(right, params, generics)
        }
        Expr::IndexAccess { object, indices } => {
            expr_references_params(object, params, generics)
                || indices
                    .iter()
                    .any(|i| expr_references_params(i, params, generics))
        }
        _ => false,
    }
}



impl Interpreter {
    pub fn substitute_generics_in_type_str(
        &self,
        type_str: &str,
        generic_params: &[String],
        concrete_type_str: &str,
    ) -> String {
        if generic_params.is_empty() || !concrete_type_str.contains('<') {
            return type_str.to_string();
        }
        let start = match concrete_type_str.find('<') {
            Some(i) => i,
            None => return type_str.to_string(),
        };
        let end = match concrete_type_str.rfind('>') {
            Some(i) => i,
            None => return type_str.to_string(),
        };
        let args_slice = &concrete_type_str[start + 1..end];
        let args: Vec<String> = args_slice
            .split(',')
            .map(|s| self.extract_clean_blueprint_name(s))
            .collect();
        let mut result = type_str.to_string();
        for (i, p) in generic_params.iter().enumerate() {
            let param_name = p.trim_start_matches("...");
            if let Some(arg) = args.get(i) {
                let patterns = [
                    (format!("<{}>", param_name), format!("<{}>", arg)),
                    (format!("<{},", param_name), format!("<{},", arg)),
                    (format!(", {}>", param_name), format!(", {}>", arg)),
                    (format!(", {},", param_name), format!(", {},", arg)),
                    (format!("{}*", param_name), format!("{}*", arg)),
                    (format!("{}&", param_name), format!("{}&", arg)),
                    (format!("{}[]", param_name), format!("{}[]", arg)),
                ];
                for (from, to) in &patterns {
                    result = result.replace(from, to);
                }
                if result == param_name {
                    result = arg.clone();
                } else if result == format!("{}*", param_name) {
                    result = format!("{}*", arg);
                } else if result == format!("{}&", param_name) {
                    result = format!("{}&", arg);
                } else if result == format!("{}[]", param_name) {
                    result = format!("{}[]", arg);
                }
            }
        }
        result
    }


}
