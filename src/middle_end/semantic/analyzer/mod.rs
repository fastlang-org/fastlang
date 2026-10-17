use crate::frontend::parser::ast::*;
use crate::frontend::parser::ast::Flag;
use crate::middle_end::semantic::environment::{
    BlueprintData, Environment, FnSignature, SymbolInfo, SymbolKind,
};
use crate::middle_end::semantic::handle_resolver::{
    build_blueprint_from_metadata, op_to_handle, resolve_handle_for_op, HandleLookupResult,
};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

pub mod decl;
pub mod expr;
pub mod overload;
pub mod stmt;
pub mod type_system;

pub use overload::{detect_execution_mode, has_compile_directive, resolve_call_generics};
pub(crate) use type_system::{extract_type_args_from_str, strip_wrapper};

// ============================================================
// SemanticAnalyzer - main orchestrator
// ============================================================
#[derive(Debug, Clone, Default)]
pub struct ScopeContext {
    pub in_class: bool,
    pub in_struct: bool,
    pub in_statement_scope: bool,
    pub in_generic_template: bool,
    pub current_type_name: Option<String>,
    pub current_context: Option<String>,
    pub current_machine: Option<String>,
}

impl ScopeContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_member_context(&self) -> bool {
        self.in_class || self.in_struct || self.current_type_name.is_some() || self.current_machine.is_some()
    }
}

#[derive(Debug, Clone, Default)]
pub struct MemoryTracker {
    pub heap_allocated_vars: HashSet<String>,
    pub deleted_vars: HashSet<String>,
}

impl MemoryTracker {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, Default)]
pub struct DiagnosticContext {
    pub current_file: String,
    pub source_code: Option<String>,
    pub debug_errors: bool,
    pub accumulated_errors: Vec<String>,
}

impl DiagnosticContext {
    pub fn new() -> Self {
        Self::default()
    }
}

pub struct SemanticAnalyzer {
    pub current_env: Rc<RefCell<Environment>>,
    pub scope: ScopeContext,
    pub memory: MemoryTracker,
    pub diagnostics: DiagnosticContext,
    pub active_flags: HashSet<Flag>,
    pub active_return_type: Option<BaseType>,
    pub global_metadata: HashMap<String, TypeMetadata>,
    pub dependency_graph: HashMap<String, HashSet<String>>,
    pub fn_overloads: HashMap<String, Vec<FnSignature>>,
    pub machine_labels: HashMap<String, HashMap<String, HashMap<String, BaseType>>>,
    pub class_hierarchy: HashMap<String, String>,
    pub target_backend: String,
}


impl SemanticAnalyzer {
    pub fn new(global_metadata: HashMap<String, TypeMetadata>) -> Self {
        let mut analyzer = SemanticAnalyzer {
            current_env: Environment::new(),
            scope: ScopeContext::new(),
            memory: MemoryTracker::new(),
            diagnostics: DiagnosticContext::new(),
            active_flags: {
                let mut s = HashSet::new();
                s.insert(Flag::HasExit);
                s
            },
            active_return_type: None,
            global_metadata,
            dependency_graph: HashMap::new(),
            target_backend: "cpp".to_string(),
            fn_overloads: HashMap::new(),
            machine_labels: HashMap::new(),
            class_hierarchy: HashMap::new(),
        };
        analyzer.import_metadata();
        analyzer
    }

    // ----------------------------------------------------------
    // Import global TypeMetadata -> BlueprintData into env
    // ----------------------------------------------------------
    pub(crate) fn import_metadata(&mut self) {
        let names: Vec<String> = self.global_metadata.keys().cloned().collect();
        for name in names {
            let meta = self.global_metadata[&name].clone();
            let bp = build_blueprint_from_metadata(&meta);
            self.current_env.borrow_mut().define_blueprint(name, bp);
        }
    }


    pub(crate) fn enter_scope(&mut self) {
        let new_env = Environment::with_parent(Rc::clone(&self.current_env));
        self.current_env = new_env;
    }

    pub fn emit_warning(
        &self,
        message: &str,
        line: Option<usize>,
        column: Option<usize>,
        hint: Option<&str>,
    ) {
        let (resolved_line, resolved_col) = match (line, column) {
            (Some(l), Some(c)) => (Some(l), Some(c)),
            _ => {
                if let Some(ref src) = self.diagnostics.source_code {
                    let (l, c) = crate::locate_semantic_error_in_source(src, message);
                    if l > 0 && c > 0 {
                        (Some(l), Some(c))
                    } else {
                        (None, None)
                    }
                } else {
                    (None, None)
                }
            }
        };

        eprintln!("\x1b[33;1mwarning:\x1b[0m {}", message);
        if let (Some(l), Some(c)) = (resolved_line, resolved_col) {
            let file_display = if self.diagnostics.current_file.is_empty() {
                "<input>"
            } else {
                &self.diagnostics.current_file
            };
            eprintln!("  --> {}:{}:{}", file_display, l, c);
            if let Some(ref src) = self.diagnostics.source_code {
                if let Some(line_str) = src.lines().nth(l.saturating_sub(1)) {
                    let pad = " ".repeat(format!("{}", l).len());
                    eprintln!("   {} |", pad);
                    eprintln!("{} | {}", l, line_str);
                    let caret_pad = " ".repeat(c.saturating_sub(1));
                    eprintln!("   {} | {}^{}", pad, caret_pad, "~".repeat(4));
                }
            }
        }
        if let Some(h) = hint {
            eprintln!("  = \x1b[36;1mhelp:\x1b[0m {}", h);
        }
    }

    pub(crate) fn leave_scope(&mut self) {
        if !self.scope.is_member_context() {
            for (name, info) in &self.current_env.borrow().symbols {
                if !info.is_used
                    && !name.starts_with('_')
                    && name != "this"
                    && name != "data"
                    && name != "__this__"
                {
                    if let SymbolKind::Variable { .. } = &info.kind {
                        let kind_str = if info.is_param {
                            "parameter"
                        } else {
                            "variable"
                        };
                        let hint = format!(
                            "if this is intentional, prefix with an underscore: '_{}'",
                            name
                        );
                        self.emit_warning(
                            &format!(
                                "{} '{}' is declared but never used in scope",
                                kind_str, name
                            ),
                            None,
                            None,
                            Some(&hint),
                        );
                    }
                }
            }
        }
        let parent = self
            .current_env
            .borrow()
            .parent
            .clone()
            .expect("Cannot leave global scope");
        self.current_env = parent;
    }


    pub fn analyze(&mut self, ast: &Vec<Stmt>) -> Result<(), String> {
        // Pass 0: Pre-register @compile members, macros, and impl metadata so declaration order in file does not matter
        for stmt in ast {
            if let Stmt::Declaration(decl) = stmt {
                if let Err(e) = self.pre_register_decl(decl) {
                    if self.diagnostics.debug_errors {
                        self.diagnostics.accumulated_errors.push(e);
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        for stmt in ast {
            if let Err(e) = self.visit_statement(stmt) {
                if self.diagnostics.debug_errors {
                    self.diagnostics.accumulated_errors.push(e);
                } else {
                    return Err(e);
                }
            }
        }

        if self.diagnostics.debug_errors && !self.diagnostics.accumulated_errors.is_empty() {
            return Err(self.diagnostics.accumulated_errors.join("\n"));
        }
        Ok(())
    }

    pub fn record_dependency(&mut self, dep: String) {
        let ctx = self
            .scope
            .current_context
            .clone()
            .unwrap_or_else(|| "main".to_string());
        self.dependency_graph
            .entry(ctx)
            .or_default()
            .insert(dep);
    }

    pub fn record_type_dependencies(&mut self, type_node: &BaseType) {
        let mut deps = Vec::new();
        type_node.collect_type_dependencies(&mut deps);
        for dep in deps {
            self.record_dependency(dep);
        }
    }

    pub fn get_reachable_symbols(&self) -> HashSet<String> {
        let mut reachable = HashSet::new();
        let mut queue = Vec::new();

        // Always start from main or top-level context
        queue.push("main".to_string());
        reachable.insert("main".to_string());

        for (name, info) in &self.current_env.borrow().symbols {
            if info.is_used
                && reachable.insert(name.clone()) {
                    queue.push(name.clone());
                }
        }

        while let Some(curr) = queue.pop() {
            // 1. If curr is a qualified method "Type::method":
            if let Some(idx) = curr.find("::") {
                let (type_part, method_part) = curr.split_at(idx);
                let method_name = &method_part[2..];
                // Check if type_part has a base class
                if let Some(base) = self.class_hierarchy.get(type_part) {
                    let base_method = format!("{}::{}", base, method_name);
                    if reachable.insert(base_method.clone()) {
                        queue.push(base_method);
                    }
                }
                // Check if any derived class of type_part is in reachable
                for (derived, base) in &self.class_hierarchy {
                    if base == type_part && reachable.contains(derived) {
                        let derived_method = format!("{}::{}", derived, method_name);
                        if reachable.insert(derived_method.clone()) {
                            queue.push(derived_method);
                        }
                    }
                }
                // Also reach bare symbol if qualified (e.g. fmt::println -> println)
                let bare_sym = method_name;
                if reachable.insert(bare_sym.to_string()) {
                    queue.push(bare_sym.to_string());
                }
            } else {
                // curr is a top-level symbol or type name
                // If it's a type name: automatically include its init, drop, and all handles
                let init_sym = format!("{}::init", curr);
                if self.dependency_graph.contains_key(&init_sym)
                    && reachable.insert(init_sym.clone()) {
                        queue.push(init_sym);
                    }
                if let Some(bp) = self.current_env.borrow().lookup_blueprint(&curr) {
                    for h_name in bp.handle_signatures.keys() {
                        let h_sym = format!("{}::{}", curr, h_name);
                        if reachable.insert(h_sym.clone()) {
                            queue.push(h_sym);
                        }
                    }
                }
                if let Some(meta) = self.global_metadata.get(&curr) {
                    for h_name in meta.handle_signatures.keys() {
                        let h_sym = format!("{}::{}", curr, h_name);
                        if reachable.insert(h_sym.clone()) {
                            queue.push(h_sym);
                        }
                    }
                }
                let drop_sym = format!("{}::drop", curr);
                if self.dependency_graph.contains_key(&drop_sym)
                    && reachable.insert(drop_sym.clone()) {
                        queue.push(drop_sym);
                    }
                // If curr is a derived class, check if any virtual methods of its base were already reached
                if let Some(base) = self.class_hierarchy.get(&curr) {
                    for sym in &reachable.clone() {
                        if let Some(idx) = sym.find("::") {
                            let (t, m) = sym.split_at(idx);
                            if t == base {
                                let derived_m = format!("{}::{}", curr, &m[2..]);
                                if reachable.insert(derived_m.clone()) {
                                    queue.push(derived_m);
                                }
                            }
                        }
                    }
                }
            }

            if let Some(deps) = self.dependency_graph.get(&curr) {
                for dep in deps {
                    if reachable.insert(dep.clone()) {
                        queue.push(dep.clone());
                    }
                }
            }
        }

        reachable
    }
}
