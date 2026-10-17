pub mod expr;
pub mod reflection;
pub mod stmt;
pub mod substitute;

#[allow(unused_imports)]
pub use expr::*;
#[allow(unused_imports)]
pub use reflection::*;
#[allow(unused_imports)]
pub use stmt::*;
pub use substitute::*;

use crate::frontend::parser::ast::*;
use crate::middle_end::interpreter::env::InterpreterEnv;
use crate::middle_end::semantic::environment::{Environment, FnSignature, SymbolKind};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub fn is_literal(expr: &Expr) -> bool {
    match expr {
        Expr::LiteralInt(_)
        | Expr::LiteralUInt(_)
        | Expr::LiteralFloat(_)
        | Expr::LiteralBool(_)
        | Expr::LiteralChar(_)
        | Expr::LiteralUChar(_)
        | Expr::LiteralString(_)
        | Expr::LiteralVoid => true,
        Expr::ArrayLiteral(elems) => elems.iter().all(is_literal),
        _ => false,
    }
}

pub fn apply_reassign_op(cur: &Expr, op: &str, rhs: &Expr) -> Option<Expr> {
    match (cur, op, rhs) {
        (Expr::LiteralInt(a), "+=", Expr::LiteralInt(b)) => Some(Expr::LiteralInt(a + b)),
        (Expr::LiteralInt(a), "-=", Expr::LiteralInt(b)) => Some(Expr::LiteralInt(a - b)),
        (Expr::LiteralInt(a), "*=", Expr::LiteralInt(b)) => Some(Expr::LiteralInt(a * b)),
        (Expr::LiteralInt(a), "/=", Expr::LiteralInt(b)) if *b != 0 => {
            Some(Expr::LiteralInt(a / b))
        }
        (Expr::LiteralFloat(a), "+=", Expr::LiteralFloat(b)) => Some(Expr::LiteralFloat(a + b)),
        (Expr::LiteralFloat(a), "-=", Expr::LiteralFloat(b)) => Some(Expr::LiteralFloat(a - b)),
        (Expr::LiteralFloat(a), "*=", Expr::LiteralFloat(b)) => Some(Expr::LiteralFloat(a * b)),
        (Expr::LiteralFloat(a), "/=", Expr::LiteralFloat(b)) if *b != 0.0 => {
            Some(Expr::LiteralFloat(a / b))
        }
        (Expr::LiteralInt(a), "+=", Expr::LiteralFloat(b)) => {
            Some(Expr::LiteralFloat(*a as f64 + b))
        }
        (Expr::LiteralFloat(a), "+=", Expr::LiteralInt(b)) => {
            Some(Expr::LiteralFloat(a + *b as f64))
        }
        (Expr::LiteralString(a), "+=", Expr::LiteralString(b)) => {
            Some(Expr::LiteralString(format!("{}{}", a, b)))
        }
        (Expr::LiteralString(a), "+=", Expr::LiteralChar(b)) => {
            Some(Expr::LiteralString(format!("{}{}", a, b)))
        }
        (Expr::LiteralString(a), "+=", Expr::LiteralUChar(b)) => {
            let ch = std::char::from_u32(*b).unwrap_or('\0');
            Some(Expr::LiteralString(format!("{}{}", a, ch)))
        }
        _ => None,
    }
}

pub fn is_known_type_name(s: &str) -> bool {
    let clean = s.trim_end_matches("[]").trim();
    let base = clean.split('<').next().unwrap_or(clean);
    matches!(
        base,
        "int8"
            | "int16"
            | "int32"
            | "int"
            | "int64"
            | "int128"
            | "uint8"
            | "uint16"
            | "uint32"
            | "uint"
            | "uint64"
            | "uint128"
            | "usize"
            | "isize"
            | "float32"
            | "float"
            | "float64"
            | "float128"
            | "char"
            | "uchar"
            | "str"
            | "ustr"
            | "bool"
            | "void"
            | "type"
            | "array"
    )
}

#[inline]
pub fn strip_generic_wrapper<'a>(s: &'a str, prefix: &str) -> &'a str {
    if let Some(rest) = s.strip_prefix(prefix) {
        if let Some(inner) = rest.strip_suffix('>') {
            return inner;
        }
    }
    s
}

#[inline]
pub fn strip_type_wrapper(s: &str) -> &str {
    let mut res = strip_generic_wrapper(s, "type<");
    for prefix in &["blueprint::", "struct::", "class::", "enum::"] {
        if let Some(rest) = res.strip_prefix(prefix) {
            res = rest;
            break;
        }
    }
    res
}

pub fn is_typeof_callee(callee: &Expr) -> bool {
    match callee {
        Expr::Identifier(id) => id == "typeof",
        Expr::NamespaceAccess {
            namespace,
            property,
        } if namespace == "@compile" => match &**property {
            Expr::Identifier(id) => id == "typeof",
            _ => false,
        },
        _ => false,
    }
}

pub fn has_compile_operations(stmts: &[Stmt]) -> bool {
    fn check_expr(e: &Expr) -> bool {
        match e {
            Expr::NamespaceAccess { namespace, .. } if namespace == "@compile" => true,
            Expr::Call { callee, args, .. } => {
                if check_expr(callee) {
                    return true;
                }
                args.iter().any(check_expr)
            }
            Expr::BinaryOp { left, right, .. } => check_expr(left) || check_expr(right),
            Expr::UnaryOp { operand, .. } => check_expr(operand),
            Expr::Cast { expr, .. } => check_expr(expr),
            Expr::PropertyAccess { object, .. } => check_expr(object),
            Expr::ArrayLiteral(elements) => elements.iter().any(check_expr),
            _ => false,
        }
    }

    fn check_stmt(s: &Stmt) -> bool {
        match s {
            Stmt::CompileValidation { .. } => true,
            Stmt::Declaration(Decl::CompileDecl { .. }) => true,
            Stmt::CallStmt(e) | Stmt::ExpressionStmt(e) => check_expr(e),
            Stmt::ReturnStmt(Some(e)) => check_expr(e),
            Stmt::ReassignStmt { value, .. } => check_expr(value),
            Stmt::IfStmt {
                condition,
                then_block,
                else_block,
            } => {
                check_expr(condition)
                    || then_block.iter().any(check_stmt)
                    || else_block
                        .as_ref()
                        .is_some_and(|eb| eb.iter().any(check_stmt))
            }
            Stmt::WhileStmt {
                condition, body, ..
            } => {
                check_expr(condition)
                    || match body {
                        EitherBlock::Inline(stmts) => stmts.iter().any(check_stmt),
                        EitherBlock::External(e) => check_expr(e),
                    }
            }
            Stmt::LoopStmt { count, body } => {
                count.as_ref().is_some_and(check_expr)
                    || match body {
                        EitherBlock::Inline(stmts) => stmts.iter().any(check_stmt),
                        EitherBlock::External(e) => check_expr(e),
                    }
            }
            Stmt::ForStmt { body, .. } => match body {
                EitherBlock::Inline(stmts) => stmts.iter().any(check_stmt),
                EitherBlock::External(e) => check_expr(e),
            },
            Stmt::Block(stmts) | Stmt::ThisBlock(stmts) => stmts.iter().any(check_stmt),
            _ => false,
        }
    }

    stmts.iter().any(check_stmt)
}

pub(crate) fn substitute_this_in_expr(expr: &Expr, replacement: &Expr) -> Expr {
    match expr {
        Expr::This => replacement.clone(),
        Expr::PropertyAccess { object, property } => Expr::PropertyAccess {
            object: Box::new(substitute_this_in_expr(object, replacement)),
            property: property.clone(),
        },
        Expr::UnaryOp { operator, operand } => Expr::UnaryOp {
            operator: operator.clone(),
            operand: Box::new(substitute_this_in_expr(operand, replacement)),
        },
        Expr::BinaryOp {
            left,
            operator,
            right,
        } => Expr::BinaryOp {
            left: Box::new(substitute_this_in_expr(left, replacement)),
            operator: operator.clone(),
            right: Box::new(substitute_this_in_expr(right, replacement)),
        },
        _ => expr.clone(),
    }
}

pub(crate) fn expr_contains_this(expr: &Expr) -> bool {
    match expr {
        Expr::This => true,
        Expr::PropertyAccess { object, .. } => expr_contains_this(object),
        Expr::UnaryOp { operand, .. } => expr_contains_this(operand),
        Expr::BinaryOp { left, right, .. } => expr_contains_this(left) || expr_contains_this(right),
        Expr::Call { callee, args, .. } => {
            expr_contains_this(callee) || args.iter().any(expr_contains_this)
        }
        _ => false,
    }
}

#[allow(clippy::type_complexity)]
pub struct Interpreter {
    pub env: InterpreterEnv,
    pub functions: HashMap<String, (Vec<BaseType>, Vec<Param>, Vec<Stmt>)>,
    pub micros: HashMap<String, (Vec<Param>, Vec<Stmt>)>,
    pub var_types: HashMap<String, String>,
    pub current_env: Option<Rc<RefCell<Environment>>>,
    pub fn_overloads: HashMap<String, Vec<FnSignature>>,
    pub split_functions: HashMap<String, (Vec<Param>, Vec<(Option<Expr>, String)>)>,
    pub in_function_body: bool,
    pub current_switch_type: Option<String>,
    pub current_type: Option<String>,
    pub target_backend: String,
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

impl Interpreter {
    pub fn new() -> Self {
        Self {
            env: InterpreterEnv::new(),
            functions: HashMap::new(),
            micros: HashMap::new(),
            var_types: HashMap::new(),
            current_env: None,
            fn_overloads: HashMap::new(),
            split_functions: HashMap::new(),
            in_function_body: false,
            current_switch_type: None,
            current_type: None,
            target_backend: "cpp".to_string(),
        }
    }

    pub fn load_from_program(
        &mut self,
        main_ast: &[Stmt],
        modules: &[crate::loader::LoadedModule],
    ) {
        let mut all_stmts = Vec::new();
        for m in modules {
            all_stmts.extend(m.ast.clone());
        }
        all_stmts.extend(main_ast.to_vec());

        for s in all_stmts {
            if let Stmt::Declaration(decl) = s {
                match decl {
                    Decl::FnDecl {
                        name,
                        generics,
                        params,
                        body,
                        ..
                    } => {
                        self.functions.insert(name, (generics, params, body));
                    }
                    Decl::MicroDecl {
                        name, params, body, ..
                    } => {
                        self.micros.insert(name, (params, body));
                    }
                    Decl::ImplDecl {
                        target,
                        methods,
                        handle_block,
                        ..
                    } => {
                        let clean_target = self.extract_clean_blueprint_name(&target);
                        for m in methods {
                            if let Decl::FnDecl {
                                name,
                                generics,
                                params,
                                body,
                                ..
                            } = m
                            {
                                self.functions.insert(
                                    name.clone(),
                                    (generics.clone(), params.clone(), body.clone()),
                                );
                                self.functions.insert(
                                    format!("{}::{}", clean_target, name),
                                    (generics, params, body),
                                );
                            }
                        }
                        for h in handle_block {
                            if let Decl::FnDecl {
                                name,
                                generics,
                                params,
                                body,
                                ..
                            } = h
                            {
                                self.functions.insert(
                                    name.clone(),
                                    (generics.clone(), params.clone(), body.clone()),
                                );
                                self.functions.insert(
                                    format!("{}::{}", clean_target, name),
                                    (generics, params, body),
                                );
                            }
                        }
                    }
                    Decl::NamespaceDecl { name, decls, .. } => {
                        for d in decls {
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
                                Decl::MicroDecl {
                                    name: m_name,
                                    params,
                                    body,
                                    ..
                                } => {
                                    self.micros.insert(
                                        format!("{}::{}", name, m_name),
                                        (params.clone(), body.clone()),
                                    );
                                }
                                _ => {}
                            }
                        }
                    }
                    Decl::CompileDecl { name, decls } => {
                        for d in decls {
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
                                    self.functions.insert(
                                        format!("{}::{}", name, m_name),
                                        (vec![], params.clone(), body.clone()),
                                    );
                                    self.functions.insert(
                                        format!("{}::{}", name, clean_m),
                                        (vec![], params.clone(), body.clone()),
                                    );
                                    self.functions.insert(
                                        m_name.clone(),
                                        (vec![], params.clone(), body.clone()),
                                    );
                                    self.functions.insert(
                                        clean_m.to_string(),
                                        (vec![], params.clone(), body.clone()),
                                    );
                                }
                                Decl::MicroDecl {
                                    name: m_name,
                                    params,
                                    body,
                                    generics: _,
                                    ..
                                } => {
                                    self.micros.insert(
                                        format!("{}::{}", name, m_name),
                                        (params.clone(), body.clone()),
                                    );
                                    self.micros.insert(m_name.clone(), (params, body));
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn load_functions_from_env(&mut self, env: &Environment) {
        for (name, info) in &env.symbols {
            if let SymbolKind::Function {
                generics,
                params,
                body: Some(body),
                ..
            } = &info.kind
            {
                self.functions
                    .entry(name.clone())
                    .or_insert_with(|| (generics.clone(), params.clone(), body.clone()));
            } else if let SymbolKind::Macro { params, body, .. } = &info.kind {
                self.functions
                    .entry(name.clone())
                    .or_insert_with(|| (vec![], params.clone(), body.clone()));
            } else if let SymbolKind::Micro { params, body, .. } = &info.kind {
                self.micros
                    .entry(name.clone())
                    .or_insert_with(|| (params.clone(), body.clone()));
            }
        }
        if let Some(ref parent) = env.parent {
            self.load_functions_from_env(&parent.borrow());
        }
    }


    pub fn lookup_fn(&self, name: &str) -> Option<(Vec<BaseType>, Vec<Param>)> {
        if let Some(sigs) = self.fn_overloads.get(name) {
            if let Some(sig) = sigs.iter().find(|s| !s.generics.is_empty()) {
                return Some((sig.generics.clone(), sig.params.clone()));
            }
            if let Some(sig) = sigs.first() {
                return Some((sig.generics.clone(), sig.params.clone()));
            }
        }
        if let Some(ref env_rc) = self.current_env {
            if let Some(info) = env_rc.borrow().lookup(name) {
                if let SymbolKind::Function {
                    generics, params, ..
                } = &info.kind
                {
                    return Some((generics.clone(), params.clone()));
                }
            }
        }
        if let Some((generics, params, _)) = self.functions.get(name) {
            return Some((generics.clone(), params.clone()));
        }
        None
    }
}
