use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::rc::Rc;

use crate::backend::cpp::runtime::generate_runtime_header;
use crate::backend::cranelift::aot::CraneliftAotBackend;
use crate::backend::*;
use crate::frontend::parser::ast::*;
use crate::frontend::parser::ast_json::ast_to_json_string;
use crate::middle_end::interpreter::eval::Interpreter;
use crate::middle_end::semantic::analyzer::decl::make_sig_key;
use crate::middle_end::semantic::analyzer::SemanticAnalyzer;
use crate::middle_end::semantic::environment::{Environment, FnSignature};

pub mod backend;
pub mod cli;
pub mod frontend;
pub mod loader;
pub mod middle_end;
pub mod project;

pub fn report_visual_error(source: &str, file: &str, line: usize, column: usize, err_msg: &str) {
    let lines: Vec<&str> = source.lines().collect();

    let context_lines = 2;
    let start_line = if line > context_lines {
        line - context_lines
    } else {
        1
    };
    let end_line = std::cmp::min(line + context_lines, lines.len());
    let line_str_len = format!("{}", end_line).len().max(3);

    eprintln!("\n\x1b[31;1merror:\x1b[0m {}", err_msg);
    if file.is_empty() {
        eprintln!("  \x1b[36;1m-->\x1b[0m line {}:{}", line, column);
    } else {
        eprintln!("  \x1b[36;1m-->\x1b[0m {}:{}:{}", file, line, column);
    }
    eprintln!("{:width$} \x1b[36;1m|\x1b[0m", "", width = line_str_len);

    for i in start_line..=end_line {
        let i_minus_1 = i - 1;
        if i_minus_1 < lines.len() {
            let l_text = lines[i_minus_1];
            if i == line {
                eprintln!(
                    "{:width$} \x1b[36;1m|\x1b[0m {}",
                    i,
                    l_text,
                    width = line_str_len
                );
                let col_offset = if column > 0 { column - 1 } else { 0 };
                let padding = " ".repeat(col_offset);
                eprintln!(
                    "{:width$} \x1b[36;1m|\x1b[0m {}\x1b[31;1m^\x1b[0m",
                    "",
                    padding,
                    width = line_str_len
                );
            } else {
                eprintln!(
                    "{:width$} \x1b[36;1m|\x1b[0m {}",
                    i,
                    l_text,
                    width = line_str_len
                );
            }
        }
    }
    eprintln!("{:width$} \x1b[36;1m|\x1b[0m\n", "", width = line_str_len);
}

pub fn extract_line_col_from_msg(msg: &str) -> (Option<usize>, Option<usize>) {
    if let Some(idx) = msg.find("at line ") {
        let sub = &msg[idx + 8..];
        let mut parts = sub
            .split(|c: char| !c.is_ascii_digit())
            .filter(|s| !s.is_empty());
        let line = parts.next().and_then(|s| s.parse::<usize>().ok());
        let col = parts.next().and_then(|s| s.parse::<usize>().ok());
        (line, col)
    } else {
        (None, None)
    }
}

pub fn locate_semantic_error_in_source(source: &str, err_msg: &str) -> (usize, usize) {
    if let (Some(l), Some(c)) = extract_line_col_from_msg(err_msg) {
        return (l, c);
    }

    let mut quoted = Vec::new();
    let mut start_quote = None;
    for (i, ch) in err_msg.char_indices() {
        if ch == '\'' {
            if let Some(s) = start_quote {
                quoted.push(&err_msg[s + 1..i]);
                start_quote = None;
            } else {
                start_quote = Some(i);
            }
        }
    }

    let lines: Vec<&str> = source.lines().collect();

    if err_msg.contains("Return type mismatch") {
        for (idx, line) in lines.iter().enumerate().rev() {
            let trimmed = line.trim();
            if trimmed.starts_with("return ") || trimmed == "return;" || trimmed.starts_with("return\t") {
                let pos = line.find("return").unwrap_or(0);
                return (idx + 1, pos + 1);
            }
        }
    }

    if err_msg.contains("declared but never used") && !quoted.is_empty() {
        let var_name = quoted[0];
        for (idx, line) in lines.iter().enumerate() {
            if let Some(pos) = line.find(var_name) {
                let before = if pos == 0 { ' ' } else { line[..pos].chars().last().unwrap_or(' ') };
                let after_idx = pos + var_name.len();
                let after = if after_idx >= line.len() { ' ' } else { line[after_idx..].chars().next().unwrap_or(' ') };
                if (!before.is_alphanumeric() && before != '_') && (!after.is_alphanumeric() && after != '_') {
                    return (idx + 1, pos + 1);
                }
            }
        }
    }

    if err_msg.contains("function") && !quoted.is_empty() {
        let fn_name = quoted[0];
        for (idx, line) in lines.iter().enumerate() {
            if let Some(pos) = line.find(&format!("fn {}", fn_name)) {
                return (idx + 1, pos + 4);
            }
            if let Some(pos) = line.find(&format!("method {}", fn_name)) {
                return (idx + 1, pos + 8);
            }
        }
    }

    if err_msg.contains("Cannot reassign") && !quoted.is_empty() {
        let var_name = quoted[0];
        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if (trimmed.starts_with(var_name) || line.contains(&format!(" {}", var_name)))
                && (trimmed.contains('=') || trimmed.contains("+=") || trimmed.contains("-="))
                && !trimmed.starts_with("const ")
                && !trimmed.starts_with("int")
                && !trimmed.starts_with("str")
                && !trimmed.starts_with("bool")
            {
                if let Some(pos) = line.find(var_name) {
                    return (idx + 1, pos + 1);
                }
            }
        }
    }

    if err_msg.contains("Type '") && !quoted.is_empty() {
        let type_name = quoted[0];
        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("int")
                || trimmed.starts_with("uint")
                || trimmed.starts_with("str")
                || trimmed.starts_with("bool")
                || trimmed.starts_with("float")
                || trimmed.starts_with("const ")
            {
                continue;
            }
            if let Some(pos) = line.find(type_name) {
                return (idx + 1, pos + 1);
            }
        }
    }

    for sym in &quoted {
        if sym.is_empty() {
            continue;
        }
        for (idx, line) in lines.iter().enumerate() {
            let mut search_from = 0;
            while let Some(pos) = line[search_from..].find(*sym) {
                let actual_pos = search_from + pos;
                let before = if actual_pos == 0 {
                    ' '
                } else {
                    line[..actual_pos].chars().last().unwrap_or(' ')
                };
                let after_idx = actual_pos + sym.len();
                let after = if after_idx >= line.len() {
                    ' '
                } else {
                    line[after_idx..].chars().next().unwrap_or(' ')
                };
                let is_word_before = before.is_alphanumeric() || before == '_';
                let is_word_after = after.is_alphanumeric() || after == '_';
                if !is_word_before && !is_word_after {
                    return (idx + 1, actual_pos + 1);
                }
                search_from = actual_pos + sym.len();
            }
        }
    }

    (1, 1)
}

pub fn report_semantic_error(file: &str, source: Option<&str>, err_msg: &str) {
    for single_err in err_msg.lines() {
        let single_err = single_err.trim();
        if single_err.is_empty() {
            continue;
        }
        let clean_msg = if let Some(stripped) = single_err.strip_prefix("Semantic Error:") {
            stripped.trim()
        } else {
            single_err
        };

        if let Some(src) = source {
            let (line, col) = locate_semantic_error_in_source(src, single_err);
            report_visual_error(src, file, line, col, clean_msg);
        } else {
            eprintln!("\n\x1b[31;1merror:\x1b[0m {}", clean_msg);
            eprintln!("  \x1b[36;1m-->\x1b[0m {}:1:1\n", file);
        }
    }
}

fn module_import_deps(ast: &[Stmt]) -> Vec<(String, Option<Vec<String>>)> {
    ast.iter()
        .filter_map(|stmt| {
            if let Stmt::Declaration(Decl::Import {
                module_path,
                imports,
                abi,
                ..
            }) = stmt
            {
                if abi.is_some()
                    || module_path
                        .first()
                        .is_some_and(|p| p.ends_with(".h") || p.ends_with(".hpp"))
                {
                    None
                } else {
                    Some((module_path.join("/"), imports.clone()))
                }
            } else {
                None
            }
        })
        .collect()
}

fn inject_module_exports(
    analyzer: &mut SemanticAnalyzer,
    dep_name: &str,
    imports: &Option<Vec<String>>,
    envs: &HashMap<String, Rc<RefCell<Environment>>>,
    all_fn_overloads: &HashMap<String, HashMap<String, Vec<FnSignature>>>,
    debug: bool,
) -> Result<(), String> {
    let env = envs
        .get(dep_name)
        .ok_or_else(|| format!("Error: Module {} not found.", dep_name))?;
    let full_path = dep_name.replace('/', "::");
    let base_name = dep_name.split('/').next_back().unwrap_or(dep_name);
    let mut prefixes = vec![base_name.to_string()];
    if full_path != base_name {
        prefixes.push(full_path);
    }

    let symbols = env.borrow().symbols.clone();
    for (sym_name, info) in symbols {
        if info.visibility == Visibility::Public {
            let should_inject = match imports {
                Some(selected) => selected.contains(&sym_name),
                None => true,
            };
            if should_inject {
                if debug {
                    println!("Module {} injected symbol: {}", dep_name, sym_name);
                }
                analyzer
                    .current_env
                    .borrow_mut()
                    .define(sym_name.clone(), info.clone())
                    .ok();
                if !sym_name.contains("::") {
                    for prefix in &prefixes {
                        let qual = format!("{}::{}", prefix, sym_name);
                        analyzer
                            .current_env
                            .borrow_mut()
                            .define(qual, info.clone())
                            .ok();
                    }
                }
            }
        }
    }
    if let Some(mod_overloads) = all_fn_overloads.get(dep_name) {
        for (fn_name, sigs) in mod_overloads {
            let should_inject = match imports {
                Some(selected) => selected.contains(fn_name),
                None => true,
            };
            if should_inject {
                analyzer
                    .fn_overloads
                    .entry(fn_name.clone())
                    .or_default()
                    .extend(sigs.clone());
                if !fn_name.contains("::") {
                    for prefix in &prefixes {
                        let qual = format!("{}::{}", prefix, fn_name);
                        analyzer
                            .fn_overloads
                            .entry(qual)
                            .or_default()
                            .extend(sigs.clone());
                    }
                }
            }
        }
    }
    let blueprints = env.borrow().blueprints.clone();
    for (bp_name, bp_data) in blueprints {
        let should_inject = match imports {
            Some(selected) => selected.contains(&bp_name),
            None => true,
        };
        if should_inject {
            if debug {
                println!("Module {} injected blueprint: {}", dep_name, bp_name);
            }
            let mut cur_opt = analyzer.current_env.borrow().lookup_blueprint(&bp_name);
            if let Some(ref mut cur_bp) = cur_opt {
                if cur_bp.fields.is_empty() && !bp_data.fields.is_empty() {
                    cur_bp.fields = bp_data.fields.clone();
                } else {
                    for (k, v) in &bp_data.fields {
                        cur_bp.fields.entry(k.clone()).or_insert(v.clone());
                    }
                }
                if cur_bp.methods.is_empty() && !bp_data.methods.is_empty() {
                    cur_bp.methods = bp_data.methods.clone();
                } else {
                    for (k, v) in &bp_data.methods {
                        cur_bp.methods.entry(k.clone()).or_insert(v.clone());
                    }
                }
                for h in &bp_data.handles {
                    cur_bp.handles.insert(*h);
                }
                for (k, v) in &bp_data.handle_signatures {
                    cur_bp
                        .handle_signatures
                        .entry(k.clone())
                        .or_insert(v.clone());
                }
                for f in &bp_data.private_fields {
                    cur_bp.private_fields.insert(f.clone());
                }
                for m in &bp_data.private_methods {
                    cur_bp.private_methods.insert(m.clone());
                }
                if !bp_data.generics.is_empty() {
                    cur_bp.generics = bp_data.generics.clone();
                }
                analyzer
                    .current_env
                    .borrow_mut()
                    .define_blueprint(bp_name.clone(), cur_bp.clone());
                if !bp_name.contains("::") {
                    for prefix in &prefixes {
                        let qual = format!("{}::{}", prefix, bp_name);
                        analyzer
                            .current_env
                            .borrow_mut()
                            .define_blueprint(qual, cur_bp.clone());
                    }
                }
            } else {
                analyzer
                    .current_env
                    .borrow_mut()
                    .define_blueprint(bp_name.clone(), bp_data.clone());
                if !bp_name.contains("::") {
                    for prefix in &prefixes {
                        let qual = format!("{}::{}", prefix, bp_name);
                        analyzer
                            .current_env
                            .borrow_mut()
                            .define_blueprint(qual, bp_data.clone());
                    }
                }
            }
        }
    }
    let namespaces = env.borrow().namespaces.clone();
    for (ns_name, ns_env) in namespaces {
        analyzer
            .current_env
            .borrow_mut()
            .define_namespace(ns_name.clone(), Rc::clone(&ns_env));
        if !ns_name.contains("::") {
            for prefix in &prefixes {
                if prefix != &ns_name {
                    let qual = format!("{}::{}", prefix, ns_name);
                    analyzer
                        .current_env
                        .borrow_mut()
                        .define_namespace(qual, Rc::clone(&ns_env));
                }
            }
        }
        let ns_symbols = ns_env.borrow().symbols.clone();
        for (sym_name, info) in ns_symbols {
            if info.visibility == Visibility::Public {
                let qual_sym = format!("{}::{}", ns_name, sym_name);
                analyzer
                    .current_env
                    .borrow_mut()
                    .define_or_update(qual_sym.clone(), info.clone());
                for prefix in &prefixes {
                    let full_qual = format!("{}::{}", prefix, sym_name);
                    analyzer
                        .current_env
                        .borrow_mut()
                        .define_or_update(full_qual, info.clone());
                }
            }
        }
        let ns_blueprints = ns_env.borrow().blueprints.clone();
        for (bp_name, bp_data) in ns_blueprints {
            let qual_bp = format!("{}::{}", ns_name, bp_name);
            analyzer
                .current_env
                .borrow_mut()
                .define_blueprint(qual_bp, bp_data.clone());
            for prefix in &prefixes {
                let full_qual = format!("{}::{}", prefix, bp_name);
                analyzer
                    .current_env
                    .borrow_mut()
                    .define_blueprint(full_qual, bp_data.clone());
            }
        }
    }
    Ok(())
}

fn prune_ast_for_reachability(
    ast: &[Stmt],
    reachable: &HashSet<String>,
    fn_overloads: &HashMap<String, Vec<FnSignature>>,
    is_main_file: bool,
) -> Vec<Stmt> {
    let mut pruned = Vec::new();
    for stmt in ast {
        match stmt {
            Stmt::Declaration(decl) => {
                if let Some(pruned_decl) =
                    prune_decl_for_reachability(decl, reachable, fn_overloads, is_main_file)
                {
                    pruned.push(Stmt::Declaration(pruned_decl));
                }
            }
            other => {
                pruned.push(other.clone());
            }
        }
    }
    pruned
}

fn prune_decl_for_reachability(
    decl: &Decl,
    reachable: &HashSet<String>,
    fn_overloads: &HashMap<String, Vec<FnSignature>>,
    is_main_file: bool,
) -> Option<Decl> {
    match decl {
        Decl::FnDecl { name, params, .. } => {
            if name == "main" {
                return Some(decl.clone());
            }
            let sig_key = make_sig_key(name, params);
            let has_multiple_overloads = fn_overloads.get(name).map_or(0, |sigs| {
                let mut distinct = HashSet::new();
                for s in sigs {
                    distinct.insert(make_sig_key(&s.name, &s.params));
                }
                distinct.len()
            }) > 1;

            let is_kept = if has_multiple_overloads {
                reachable.contains(&sig_key)
            } else {
                reachable.contains(name)
                    || (name.contains("_br") && {
                        let base = name.split("_br").next().unwrap();
                        reachable.contains(base)
                    })
            };

            if is_kept {
                Some(decl.clone())
            } else {
                None
            }
        }

        Decl::ClassDecl {
            visibility,
            name,
            extends,
            handles,
            public_block,
            private_block,
            static_block,
            generics,
            handle_block,
            constructor,
        } => {
            if !reachable.contains(name) {
                return None;
            }

            // For module files (stdlib), keep ALL methods of reachable types.
            // We can't guarantee tracking all internal method-to-method calls,
            // so we only prune methods for user-defined types in the main file.
            if !is_main_file {
                return Some(decl.clone());
            }

            let filter_methods = |block: &[Decl]| -> Vec<Decl> {
                block
                    .iter()
                    .filter(|d| match d {
                        Decl::FnDecl { name: m_name, .. } => {
                            let full_m = format!("{}::{}", name, m_name);
                            reachable.contains(&full_m)
                        }
                        _ => true,
                    })
                    .cloned()
                    .collect()
            };

            Some(Decl::ClassDecl {
                visibility: visibility.clone(),
                name: name.clone(),
                extends: extends.clone(),
                handles: handles.clone(),
                public_block: filter_methods(public_block),
                private_block: filter_methods(private_block),
                static_block: filter_methods(static_block),
                generics: generics.clone(),
                handle_block: handle_block.clone(),
                constructor: constructor.clone(),
            })
        }

        Decl::StructDecl {
            visibility,
            name,
            handles,
            public_block,
            private_block,
            handle_block,
            static_block,
            constructor,
        } => {
            if !reachable.contains(name) {
                return None;
            }

            // For module files, keep all methods of reachable types.
            if !is_main_file {
                return Some(decl.clone());
            }

            let filter_methods = |block: &[Decl]| -> Vec<Decl> {
                block
                    .iter()
                    .filter(|d| match d {
                        Decl::FnDecl { name: m_name, .. } => {
                            let full_m = format!("{}::{}", name, m_name);
                            reachable.contains(&full_m)
                        }
                        _ => true,
                    })
                    .cloned()
                    .collect()
            };

            Some(Decl::StructDecl {
                visibility: visibility.clone(),
                name: name.clone(),
                handles: handles.clone(),
                public_block: filter_methods(public_block),
                private_block: filter_methods(private_block),
                handle_block: handle_block.clone(),
                static_block: filter_methods(static_block),
                constructor: constructor.clone(),
            })
        }

        Decl::ImplDecl {
            target,
            target_generics,
            is_handle_impl,
            methods,
            handle_block,
        } => {
            // If the target type is reachable, keep its impl methods
            if reachable.contains(target) {
                return Some(decl.clone());
            }

            let filter_methods: Vec<Decl> = methods
                .iter()
                .filter(|d| match d {
                    Decl::FnDecl { name: m_name, .. } => {
                        let full_m = format!("{}::{}", target, m_name);
                        reachable.contains(&full_m) || reachable.contains(m_name)
                    }
                    _ => true,
                })
                .cloned()
                .collect();

            if filter_methods.is_empty() && handle_block.is_empty() {
                None
            } else {
                Some(Decl::ImplDecl {
                    target: target.clone(),
                    target_generics: target_generics.clone(),
                    is_handle_impl: *is_handle_impl,
                    methods: filter_methods,
                    handle_block: handle_block.clone(),
                })
            }
        }

        Decl::EnumDecl {
            visibility,
            name,
            generics,
            handles,
            handle_block,
            variants,
        } => {
            if !reachable.contains(name) {
                return None;
            }

            // For module files, keep all handles of reachable enum types.
            if !is_main_file {
                return Some(decl.clone());
            }

            Some(Decl::EnumDecl {
                visibility: visibility.clone(),
                name: name.clone(),
                generics: generics.clone(),
                handles: handles.clone(),
                handle_block: handle_block.clone(),
                variants: variants.clone(),
            })
        }

        Decl::BlueprintDecl { name, .. } => {
            if !reachable.contains(name) {
                return None;
            }
            Some(decl.clone())
        }

        Decl::MachineDecl { name, .. } => {
            if !reachable.contains(name) {
                return None;
            }
            Some(decl.clone())
        }

        Decl::ObjectDecl { name, .. } => {
            if !reachable.contains(name) {
                return None;
            }
            Some(decl.clone())
        }

        Decl::ExternFnDecl { name, alias, .. } => {
            let is_kept =
                reachable.contains(name) || alias.as_ref().is_some_and(|a| reachable.contains(a));
            if is_kept {
                Some(decl.clone())
            } else {
                None
            }
        }

        Decl::ExternBlockDecl { abi, decls } => {
            let filtered_decls: Vec<Decl> = decls
                .iter()
                .filter_map(|d| {
                    prune_decl_for_reachability(d, reachable, fn_overloads, is_main_file)
                })
                .collect();
            if filtered_decls.is_empty() {
                None
            } else {
                Some(Decl::ExternBlockDecl {
                    abi: abi.clone(),
                    decls: filtered_decls,
                })
            }
        }

        Decl::VarDecl { name, .. } => {
            if is_main_file {
                Some(decl.clone())
            } else {
                if reachable.contains(name) {
                    Some(decl.clone())
                } else {
                    None
                }
            }
        }

        _ => Some(decl.clone()),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if cli::handle_subcommands(&args) {
        return;
    }

    let cli::CliOptions {
        path,
        manifest_info: _,
        manifest,
        project_root,
        target,
        custom_includes,
        backend,
        emit_ir: _emit_ir,
        ir_output_file: _ir_output_file,
        emit_cpp,
        debug,
        use_aot,
        custom_output,
        ast_output_file,
        emit_ast,
        clean_artifacts,
        debug_errors,
        run_after_compile,
        is_build_cmd: _,
        run_args,
        is_test_file: _,
        is_module_project,
    } = cli::parse_cli(&args);

    let mut loader = match (&project_root, &manifest) {
        (Some(root), Some(m)) => {
            loader::ProjectLoader::new_with_project(root.clone(), Some(m.clone()))
        }
        _ => loader::ProjectLoader::new(),
    };
    for inc in custom_includes {
        loader.include_paths.push(inc);
    }
    loader.verbose = debug;

    let mut program = match loader.load(&path, target.as_deref()) {
        Ok(p) => p,
        Err(e) => {
            if e.starts_with("Entry file") || e.starts_with("Target module") {
                eprintln!("{}", e);
            }
            std::process::exit(1);
        }
    };

    if debug {
        println!("AST Length: {}", program.main_ast.len());
        std::fs::write("ast_debug.txt", format!("{:#?}", program.main_ast)).unwrap();
    }

    if emit_ast {
        let json_str = ast_to_json_string(&program.main_ast);
        if let Some(out_f) = ast_output_file {
            std::fs::write(&out_f, &json_str).expect("Failed to write AST JSON file");
            println!("AST exported successfully to {}", out_f);
        } else {
            println!("{}", json_str);
        }
        return;
    }

    let mut envs: HashMap<String, Rc<RefCell<Environment>>> = HashMap::new();
    let mut all_fn_overloads: HashMap<String, HashMap<String, Vec<FnSignature>>> = HashMap::new();
    let mut analyzed_modules = HashSet::new();

    let mut all_module_dep_graphs: HashMap<String, HashSet<String>> = HashMap::new();
    let mut all_module_class_hierarchies: HashMap<String, String> = HashMap::new();

    // Analyze dependency modules in dependency order (e.g. std before std/list).
    while analyzed_modules.len() < program.modules.len() {
        let mut progressed = false;
        for module in &program.modules {
            if analyzed_modules.contains(&module.name) {
                continue;
            }

            let deps = module_import_deps(&module.ast);
            if !deps.iter().all(|(dep, _)| analyzed_modules.contains(dep)) {
                continue;
            }

            if debug {
                println!("Analyzing module: {}", module.name);
            }
            let mut analyzer = SemanticAnalyzer::new(program.global_metadata.clone());
            analyzer.target_backend = backend.clone();
            analyzer.diagnostics.current_file = module.name.clone();
            if envs.contains_key("std/types") && module.name != "std/types" {
                inject_module_exports(
                    &mut analyzer,
                    "std/types",
                    &None,
                    &envs,
                    &all_fn_overloads,
                    debug,
                )
                .ok();
            }
            if envs.contains_key("std") && module.name != "std" {
                inject_module_exports(&mut analyzer, "std", &None, &envs, &all_fn_overloads, debug)
                    .ok();
            }
            for (dep_name, imports) in &deps {
                if let Err(e) = inject_module_exports(
                    &mut analyzer,
                    dep_name,
                    imports,
                    &envs,
                    &all_fn_overloads,
                    debug,
                ) {
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }

            if let Err(e) = analyzer.analyze(&module.ast) {
                let mod_file = loader
                    .resolve_path(&module.name)
                    .unwrap_or_else(|| format!("{}.fast", module.name));
                let mod_source = std::fs::read_to_string(&mod_file).ok();
                report_semantic_error(&mod_file, mod_source.as_deref(), &e);
                std::process::exit(1);
            }
            for (k, v) in analyzer.dependency_graph {
                all_module_dep_graphs.entry(k).or_default().extend(v);
            }
            for (k, v) in analyzer.class_hierarchy {
                all_module_class_hierarchies.insert(k, v);
            }
            envs.insert(module.name.clone(), analyzer.current_env);
            all_fn_overloads.insert(module.name.clone(), analyzer.fn_overloads.clone());
            analyzed_modules.insert(module.name.clone());
            progressed = true;
        }

        if !progressed {
            eprintln!("Error: cyclic or missing module dependencies detected.");
            std::process::exit(1);
        }
    }

    // Analyze main file
    if debug {
        println!("\n=== Semantic Analysis ===");
    }
    let mut main_analyzer = SemanticAnalyzer::new(program.global_metadata.clone());
    main_analyzer.target_backend = backend.clone();
    main_analyzer.diagnostics.current_file = path.clone();
    main_analyzer.diagnostics.source_code = std::fs::read_to_string(&path).ok();
    main_analyzer.scope.current_context = Some("main".to_string());
    main_analyzer.diagnostics.debug_errors = debug_errors;
    for (k, v) in all_module_dep_graphs {
        main_analyzer
            .dependency_graph
            .entry(k)
            .or_default()
            .extend(v);
    }
    for (k, v) in all_module_class_hierarchies {
        main_analyzer.class_hierarchy.insert(k, v);
    }

    if envs.contains_key("std/types") {
        inject_module_exports(
            &mut main_analyzer,
            "std/types",
            &None,
            &envs,
            &all_fn_overloads,
            debug,
        )
        .ok();
    }
    if envs.contains_key("std") {
        inject_module_exports(
            &mut main_analyzer,
            "std",
            &None,
            &envs,
            &all_fn_overloads,
            debug,
        )
        .ok();
    }

    // Inject exported symbols from main's imports
    for (mod_name, imports) in &program.main_deps {
        if let Err(e) = inject_module_exports(
            &mut main_analyzer,
            mod_name,
            imports,
            &envs,
            &all_fn_overloads,
            debug,
        ) {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }

    for stmt in &program.main_ast {
        if let Stmt::Declaration(Decl::Import {
            module_path,
            abi: Some(abi),
            ..
        }) = stmt
        {
            let mod_str = module_path.join("/");
            println!(
                "Including external library <{}> (ABI: @{})...",
                mod_str, abi
            );
        }
    }

    match main_analyzer.analyze(&program.main_ast) {
        Ok(_) => {
            if debug {
                println!("Semantic Analysis Passed successfully!");
            }
        }
        Err(e) => {
            report_semantic_error(&path, main_analyzer.diagnostics.source_code.as_deref(), &e);
            std::process::exit(1);
        }
    }

    if debug_errors {
        println!("Debug Errors check finished. No critical errors found.");
        return;
    }

    let mut combined_fn_overloads: HashMap<String, Vec<FnSignature>> = HashMap::new();
    for mod_overloads in all_fn_overloads.values() {
        for (fn_name, sigs) in mod_overloads {
            combined_fn_overloads
                .entry(fn_name.clone())
                .or_default()
                .extend(sigs.clone());
        }
    }
    for (fn_name, sigs) in &main_analyzer.fn_overloads {
        combined_fn_overloads
            .entry(fn_name.clone())
            .or_default()
            .extend(sigs.clone());
    }

    let reachable_symbols = main_analyzer.get_reachable_symbols();
    if debug {
        println!("\n=== Reachable Symbols (Tree-Shaking) ===");
        let mut sorted_reachable: Vec<_> = reachable_symbols.iter().collect();
        sorted_reachable.sort();
        for sym in sorted_reachable {
            println!("  reachable: {}", sym);
        }
    }

    // Compile-Time AST Evaluation & Constant Folding Pass via Interpreter
    let mut interp = Interpreter::new();
    interp.target_backend = backend.clone();
    interp.current_env = Some(main_analyzer.current_env.clone());
    interp.fn_overloads = main_analyzer.fn_overloads.clone();
    interp.load_from_program(&program.main_ast, &program.modules);
    interp.load_functions_from_env(&main_analyzer.current_env.borrow());
    for module in &mut program.modules {
        if let Err(e) = interp.evaluate_ast(&mut module.ast) {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
    if let Err(e) = interp.evaluate_ast(&mut program.main_ast) {
        eprintln!("{}", e);
        std::process::exit(1);
    }

    // Code Generation
    let (out_path, exe_path, build_dir, out_path_base) = if let Some(ref out) = custom_output {
        if out.ends_with(".cpp") {
            let exe =
                out.trim_end_matches(".cpp").to_string() + if cfg!(windows) { ".exe" } else { "" };
            let p = std::path::Path::new(out);
            let b_dir = p.parent().unwrap_or(std::path::Path::new("")).to_path_buf();
            let stem = p
                .file_stem()
                .unwrap_or(std::ffi::OsStr::new("output"))
                .to_string_lossy();
            let base = b_dir
                .join(format!("{}", stem))
                .to_string_lossy()
                .into_owned();
            (out.clone(), exe, b_dir, base)
        } else if out.ends_with(".exe") || out.ends_with(".out") {
            let p = std::path::Path::new(out);
            let b_dir = p.parent().unwrap_or(std::path::Path::new("")).to_path_buf();
            let stem = p
                .file_stem()
                .unwrap_or(std::ffi::OsStr::new("output"))
                .to_string_lossy();
            let cpp = b_dir
                .join(format!("{}.cpp", stem))
                .to_string_lossy()
                .into_owned();
            let base = b_dir
                .join(format!("{}", stem))
                .to_string_lossy()
                .into_owned();
            (cpp, out.clone(), b_dir, base)
        } else {
            let out_p = std::path::Path::new(out);
            if out_p.is_dir() || !out.contains('.') {
                let _ = std::fs::create_dir_all(out_p);
                let stem = std::path::Path::new(&path)
                    .file_stem()
                    .unwrap_or(std::ffi::OsStr::new("output"))
                    .to_string_lossy();
                let cpp = out_p
                    .join(format!("{}.cpp", stem))
                    .to_string_lossy()
                    .into_owned();
                let exe = out_p
                    .join(if cfg!(windows) {
                        format!("{}.exe", stem)
                    } else {
                        format!("{}", stem)
                    })
                    .to_string_lossy()
                    .into_owned();
                let base = out_p
                    .join(format!("{}", stem))
                    .to_string_lossy()
                    .into_owned();
                (cpp, exe, out_p.to_path_buf(), base)
            } else {
                let b_dir = out_p
                    .parent()
                    .unwrap_or(std::path::Path::new(""))
                    .to_path_buf();
                let base = out.clone();
                (out.clone() + ".cpp", out.clone(), b_dir, base)
            }
        }
    } else {
        let source_path = std::path::Path::new(&path);
        let b_dir = if let Some(ref root) = project_root {
            root.join("build")
        } else {
            let parent_dir = source_path.parent().unwrap_or(std::path::Path::new(""));
            parent_dir.join("build")
        };
        let stem = if let Some(ref m) = manifest {
            std::borrow::Cow::Borrowed(m.package.name.as_str())
        } else {
            source_path
                .file_stem()
                .unwrap_or(std::ffi::OsStr::new("output"))
                .to_string_lossy()
        };
        if !b_dir.exists() {
            std::fs::create_dir_all(&b_dir).unwrap();
        }
        let cpp = b_dir
            .join(format!("{}.cpp", stem))
            .to_string_lossy()
            .into_owned();
        let exe = b_dir
            .join(if cfg!(windows) {
                format!("{}.exe", stem)
            } else {
                format!("{}", stem)
            })
            .to_string_lossy()
            .into_owned();
        let base = b_dir
            .join(format!("{}", stem))
            .to_string_lossy()
            .into_owned();
        (cpp, exe, b_dir, base)
    };

    let lib_dir = build_dir.join("lib");
    let clib_dir = build_dir.join("clib");
    let _ = std::fs::create_dir_all(&lib_dir);
    let _ = std::fs::create_dir_all(&clib_dir);

    // Copy C library headers/sources to build/clib
    let std_dirs = loader::get_std_dirs();
    let std_clib = std_dirs.iter().map(|d| d.join("clib")).find(|p| p.exists());
    if let Some(ref clib_src) = std_clib {
        for entry in std::fs::read_dir(clib_src).into_iter().flatten().flatten() {
            let target2 = clib_dir.join(entry.file_name());
            let should_copy = if target2.exists() {
                std::fs::read(entry.path()).ok() != std::fs::read(&target2).ok()
            } else {
                true
            };
            if should_copy {
                let _ = std::fs::copy(entry.path(), target2);
            }
        }
    }

    // Write build/clib/fast_runtime.h only for C++ backend
    if backend == "cpp" {
        let runtime_header = generate_runtime_header();
        let runtime_path = clib_dir.join("fast_runtime.h");
        let should_write = match std::fs::read_to_string(&runtime_path) {
            Ok(content) => content != runtime_header,
            Err(_) => true,
        };
        if should_write {
            let _ = std::fs::write(&runtime_path, &runtime_header);
        }
    }

    // Collect all C source files and precompiled object files in build/clib to pass directly to g++/gcc
    let mut clib_c_files = Vec::new();
    let mut clib_o_files = Vec::new();
    if clib_dir.exists() {
        for entry in std::fs::read_dir(&clib_dir).into_iter().flatten().flatten() {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("o") {
                clib_o_files.push(p);
            }
        }
        for entry in std::fs::read_dir(&clib_dir).into_iter().flatten().flatten() {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("c") {
                let o_path = p.with_extension("o");
                if !o_path.exists() {
                    clib_c_files.push(p);
                }
            }
        }
    }

    let check_pkg_dirs = vec![
        manifest
            .as_ref()
            .and_then(|m| project_root.as_ref().map(|r| m.get_packages_dir(r))),
        Some(std::path::PathBuf::from("packages")),
    ];
    for p_dir in check_pkg_dirs.into_iter().flatten() {
        if p_dir.exists() {
            for pkg_entry in std::fs::read_dir(&p_dir).into_iter().flatten().flatten() {
                if pkg_entry.path().is_dir() {
                    for f in std::fs::read_dir(pkg_entry.path())
                        .into_iter()
                        .flatten()
                        .flatten()
                    {
                        let p = f.path();
                        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                        let already_linked = clib_o_files.iter().any(|existing| {
                            existing.file_stem().and_then(|s| s.to_str()) == Some(stem)
                        }) || clib_c_files.iter().any(|existing| {
                            existing.file_stem().and_then(|s| s.to_str()) == Some(stem)
                        });
                        if !already_linked {
                            if p.extension().and_then(|e| e.to_str()) == Some("o") {
                                clib_o_files.push(p);
                            } else if p.extension().and_then(|e| e.to_str()) == Some("c") {
                                let o_path = p.with_extension("o");
                                if !o_path.exists() {
                                    clib_c_files.push(p);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if backend == "cranelift" {
        if debug {
            println!("\n=== Code Generation (Cranelift IR) ===");
        }

        // Modular Compilation:
        // Compile each loaded module into its own .o object file in lib_dir
        let mut module_obj_files = Vec::new();

        for m in &program.modules {
            let mod_safe_name = m.name.replace(['/', '\\', '-'], "_");
            let mod_out_o = lib_dir
                .join(format!("{}.o", mod_safe_name))
                .to_string_lossy()
                .into_owned();

            if use_aot {
                let mut mod_backend = CraneliftAotBackend::new();
                mod_backend.compile_ast(&m.ast);
                mod_backend.finalize(&mod_out_o);
                module_obj_files.push(mod_out_o);
            }
        }

        if use_aot {
            let out_o = format!("{}.o", out_path_base);

            let mut aot_backend = CraneliftAotBackend::new();
            aot_backend.compile_ast(&program.main_ast);
            aot_backend.finalize(&out_o);

            println!("Linking {} into {}...", out_o, exe_path);
            let mut gcc_cmd = std::process::Command::new("gcc");
            gcc_cmd.arg(&out_o);
            for mod_o in &module_obj_files {
                gcc_cmd.arg(mod_o);
            }
            for c_file in &clib_c_files {
                gcc_cmd.arg(c_file);
            }
            for o_file in &clib_o_files {
                gcc_cmd.arg(o_file);
            }
            gcc_cmd.arg("-Wl,--gc-sections").arg("-o").arg(&exe_path);
            let linker_status = gcc_cmd.status();

            match linker_status {
                Ok(s) if s.success() => {
                    println!("Native compilation successful! Executable is {}", exe_path);
                    if run_after_compile {
                        let mut run_cmd = std::process::Command::new(&exe_path);
                        run_cmd.args(&run_args);
                        match run_cmd.status() {
                            Ok(r_status) => {
                                if let Some(code) = r_status.code() {
                                    if code != 0 {
                                        std::process::exit(code);
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to execute '{}': {}", exe_path, e);
                                std::process::exit(1);
                            }
                        }
                    }
                }
                _ => {
                    eprintln!("Linker failed to produce executable from {}", out_o);
                }
            }
        } else {
            let mut cl_backend = cranelift::CraneliftBackend::new();
            cl_backend.compile_ast(&program.main_ast);
            cl_backend.finalize();
            println!("Cranelift JIT execution completed!");
        }
        return;
    }

    if debug {
        println!("\n=== Code Generation (C++) ===");
    }

    let mut initial_codegen = cpp::generator::CodeGenerator::new();
    for module in &program.modules {
        initial_codegen.scan_declarations(&module.ast);
    }
    initial_codegen.scan_declarations(&program.main_ast);

    let mut accumulated_machine_types = initial_codegen.machine_types;
    let mut accumulated_primitive_impl_methods = initial_codegen.primitive_impl_methods;
    let mut accumulated_property_access_types = initial_codegen.property_access_types;
    let mut accumulated_type_own_members = initial_codegen.type_own_members;
    let mut accumulated_struct_field_types = initial_codegen.struct_field_types;
    let mut accumulated_namespaces = initial_codegen.namespaces;
    let mut accumulated_type_aliases = initial_codegen.type_aliases;

    let base_std_names = [
        "std/types",
        "std/error",
        "std/mem",
        "std/fmt",
        "std/io",
        "std/string",
        "std/range",
        "std",
    ];
    let mut base_std_headers = Vec::new();
    let mut other_module_headers = Vec::new();

    for module in &program.modules {
        let raw_ns = module
            .name
            .split('/')
            .next()
            .unwrap_or(&module.name)
            .replace("-", "_");
        let cpp_namespace = if raw_ns == "std" {
            "fast_std".to_string()
        } else {
            raw_ns
        };

        // For module files, we do NOT prune at the AST level.
        // The C++ linker's --gc-sections handles dead code elimination for library code.
        // AST-level pruning of modules would corrupt shared .hpp files in concurrent
        // test scenarios where different tests need different symbol subsets.
        let filtered_ast: Vec<Stmt> = module
            .ast
            .iter()
            .filter_map(|stmt| match stmt {
                Stmt::Declaration(Decl::FnDecl { name, .. }) if name.starts_with("@compile::") => {
                    None
                }
                _ => Some(stmt.clone()),
            })
            .collect();

        let mut codegen = cpp::generator::CodeGenerator::new();
        codegen.machine_types = accumulated_machine_types.clone();
        codegen.primitive_impl_methods = accumulated_primitive_impl_methods.clone();
        codegen.property_access_types = accumulated_property_access_types.clone();
        codegen.type_own_members = accumulated_type_own_members.clone();
        codegen.struct_field_types = accumulated_struct_field_types.clone();
        codegen.namespaces = accumulated_namespaces.clone();
        codegen.type_aliases = accumulated_type_aliases.clone();
        let module_cpp = codegen.generate(&filtered_ast, false, false);
        accumulated_machine_types.extend(codegen.machine_types.clone());
        accumulated_struct_field_types.extend(codegen.struct_field_types.clone());
        accumulated_namespaces.extend(codegen.namespaces.clone());
        accumulated_type_aliases.extend(codegen.type_aliases.clone());
        for (k, v) in codegen.primitive_impl_methods {
            let list = accumulated_primitive_impl_methods.entry(k).or_default();
            for t in v {
                if !list.contains(&t) {
                    list.push(t);
                }
            }
        }
        accumulated_property_access_types.extend(codegen.property_access_types.clone());
        for (k, v) in codegen.type_own_members {
            accumulated_type_own_members.entry(k).or_default().extend(v);
        }

        let safe_name = module.name.replace('/', "_") + ".hpp";
        let mut header_content = String::new();
        header_content.push_str("#pragma once\n");
        header_content.push_str("#include <clib/fast_runtime.h>\n");
        for inc in &codegen.c_includes {
            header_content.push_str(&format!("{}\n", inc));
        }
        if !base_std_names.contains(&module.name.as_str()) {
            header_content.push_str("#include <lib/fast_std.hpp>\n");
        }
        for (dep, _) in module_import_deps(&module.ast) {
            let dep_safe = dep.replace('/', "_") + ".hpp";
            header_content.push_str(&format!("#include <lib/{}>\n", dep_safe));
        }
        header_content.push_str(&format!("namespace {} {{\n", cpp_namespace));
        header_content.push_str("using ::fastlang_as_str;\n");
        header_content.push_str(&module_cpp);
        header_content.push_str(&format!("\n}} // namespace {}\n", cpp_namespace));

        let header_path = lib_dir.join(&safe_name);
        let should_write = match std::fs::read_to_string(&header_path) {
            Ok(content) => content != header_content,
            Err(_) => true,
        };
        if should_write {
            let _ = std::fs::write(&header_path, &header_content);
        }

        if base_std_names.contains(&module.name.as_str()) {
            base_std_headers.push(safe_name);
        } else {
            other_module_headers.push(safe_name);
        }
    }

    base_std_headers.sort_by_key(|h| {
        base_std_names
            .iter()
            .position(|name| {
                let safe = name.replace('/', "_") + ".hpp";
                &safe == h
            })
            .unwrap_or(usize::MAX)
    });

    // Generate build/lib/fast_std.hpp (core prelude: std/types, std/error, std)
    let mut fast_std_content = String::new();
    fast_std_content.push_str("#pragma once\n");
    fast_std_content.push_str("#include <clib/fast_runtime.h>\n");
    fast_std_content.push_str("#include <clib/fmt.h>\n");
    fast_std_content.push_str("#include <clib/io.h>\n");
    let core_prelude = ["std_types.hpp", "std_error.hpp", "std.hpp"];
    for h in &core_prelude {
        if lib_dir.join(h).exists() || base_std_headers.contains(&h.to_string()) {
            fast_std_content.push_str(&format!("#include <lib/{}>\n", h));
        }
    }
    let fast_std_path = lib_dir.join("fast_std.hpp");
    let should_write_std = match std::fs::read_to_string(&fast_std_path) {
        Ok(content) => content != fast_std_content,
        Err(_) => true,
    };
    if should_write_std {
        let _ = std::fs::write(&fast_std_path, &fast_std_content);
    }

    let mut main_codegen = cpp::generator::CodeGenerator::new();
    main_codegen.machine_types = accumulated_machine_types;
    main_codegen.primitive_impl_methods = accumulated_primitive_impl_methods;
    main_codegen.property_access_types = accumulated_property_access_types;
    main_codegen.type_own_members = accumulated_type_own_members;
    main_codegen.struct_field_types = accumulated_struct_field_types;
    main_codegen.namespaces = accumulated_namespaces;
    main_codegen.type_aliases = accumulated_type_aliases;
    let pruned_main_ast = if is_module_project {
        program.main_ast.clone()
    } else {
        prune_ast_for_reachability(
            &program.main_ast,
            &reachable_symbols,
            &combined_fn_overloads,
            true,
        )
    };
    let main_cpp = main_codegen.generate(&pruned_main_ast, false, !is_module_project);

    let mut final_cpp = String::new();
    final_cpp.push_str("#include <clib/fast_runtime.h>\n");

    if envs.contains_key("std") || envs.contains_key("std/types") || !base_std_headers.is_empty() {
        final_cpp.push_str("#include <lib/fast_std.hpp>\n");
    }

    // Include all other loaded modules directly in final_cpp
    for module in &program.modules {
        let safe_name = module.name.replace('/', "_") + ".hpp";
        if module.name != "std" && module.name != "std/types" && module.name != "std/error" {
            let inc_line = format!("#include <lib/{}>\n", safe_name);
            if !final_cpp.contains(&inc_line) {
                final_cpp.push_str(&inc_line);
            }
        }
    }

    for (dep_mod, _) in &program.main_deps {
        let safe_name = dep_mod.replace('/', "_") + ".hpp";
        let inc_line = format!("#include <lib/{}>\n", safe_name);
        if !final_cpp.contains(&inc_line) {
            final_cpp.push_str(&inc_line);
        }
    }

    for inc in &main_codegen.c_includes {
        final_cpp.push_str(&format!("{}\n", inc));
    }

    if envs.contains_key("std") || !base_std_headers.is_empty() {
        final_cpp.push_str("\nusing namespace fast_std;\nusing ::fastlang_as_str;\n\n");
    }

    final_cpp.push_str(&main_cpp);

    let mut write_success = false;
    for _ in 0..10 {
        if fs::write(&out_path, &final_cpp).is_ok() {
            write_success = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    if !write_success {
        fs::write(&out_path, &final_cpp).expect("Failed to write output.cpp");
    }
    if debug {
        println!("Successfully generated C++ code to {}", out_path);
    }

    if emit_cpp {
        println!("C++ emission complete (--emit-cpp specified, skipped g++ compilation).");
        return;
    }

    if is_module_project {
        let mod_stem = manifest
            .as_ref()
            .map(|m| m.package.name.as_str())
            .unwrap_or("lib");
        let obj_path = build_dir.join(format!("{}.o", mod_stem));
        let hpp_path = build_dir.join(format!("{}.hpp", mod_stem));
        let mut header_content = String::new();
        header_content.push_str("#pragma once\n");
        header_content.push_str(&final_cpp);
        let _ = fs::write(&hpp_path, &header_content);

        if debug {
            println!("Compiling library object to {}...", obj_path.display());
        }

        let mut gpp_cmd = std::process::Command::new("g++");
        gpp_cmd.arg("-c");
        gpp_cmd.arg(&out_path);
        gpp_cmd
            .arg("-o")
            .arg(&obj_path)
            .arg("-std=c++20")
            .arg("-ffunction-sections")
            .arg("-fdata-sections")
            .arg("-I.")
            .arg(format!("-I{}", build_dir.display()))
            .arg(format!("-I{}", lib_dir.display()))
            .arg("-Isrc/std")
            .arg("-Isrc");
        let check_pkg_includes = vec![
            manifest
                .as_ref()
                .and_then(|m| project_root.as_ref().map(|r| m.get_packages_dir(r))),
            Some(std::path::PathBuf::from("packages")),
        ];
        for pkg_dir in check_pkg_includes.into_iter().flatten() {
            if pkg_dir.exists() {
                gpp_cmd.arg(format!("-I{}", pkg_dir.display()));
                if let Ok(entries) = std::fs::read_dir(&pkg_dir) {
                    for entry in entries.flatten() {
                        if entry.path().is_dir() {
                            gpp_cmd.arg(format!("-I{}", entry.path().display()));
                        }
                    }
                }
            }
        }
        if let Some(ref root) = project_root {
            gpp_cmd.arg(format!("-I{}", root.display()));
        }
        for s_dir in &std_dirs {
            gpp_cmd.arg(format!("-I{}", s_dir.display()));
            if let Some(parent) = s_dir.parent() {
                gpp_cmd.arg(format!("-I{}", parent.display()));
            }
        }

        let status = gpp_cmd.status();
        match status {
            Ok(s) if s.success() => {
                println!("Module library compiled successfully!");
                println!("  C++ Header: {}", hpp_path.display());
                println!("  Object:     {}", obj_path.display());
                if clean_artifacts {
                    let _ = fs::remove_file(&out_path);
                    let _ = fs::remove_dir_all(&lib_dir);
                    let _ = fs::remove_dir_all(&clib_dir);
                }
                return;
            }
            _ => {
                eprintln!(
                    "C++ module compilation failed! Check {} for errors.",
                    out_path
                );
                std::process::exit(1);
            }
        }
    }

    if debug {
        println!("Compiling to {}...", exe_path);
    }

    let mut gpp_cmd = std::process::Command::new("g++");
    gpp_cmd.arg(&out_path);
    for c_file in &clib_c_files {
        gpp_cmd.arg(c_file);
    }
    for o_file in &clib_o_files {
        gpp_cmd.arg(o_file);
    }
    gpp_cmd
        .arg("-o")
        .arg(&exe_path)
        .arg("-std=c++20")
        .arg("-ffunction-sections")
        .arg("-fdata-sections")
        .arg("-Wl,--gc-sections")
        .arg("-I.")
        .arg(format!("-I{}", build_dir.display()))
        .arg(format!("-I{}", lib_dir.display()))
        .arg("-Isrc/std")
        .arg("-Isrc");
    if cfg!(windows) {
        gpp_cmd.arg("-lws2_32");
    }
    if let Some(ref m) = manifest {
        if let Some(ref flags) = m.package.link_flags {
            for flag in flags {
                gpp_cmd.arg(flag);
            }
        }
    }
    let check_pkg_includes = vec![
        manifest
            .as_ref()
            .and_then(|m| project_root.as_ref().map(|r| m.get_packages_dir(r))),
        Some(std::path::PathBuf::from("packages")),
    ];
    for pkg_dir in check_pkg_includes.into_iter().flatten() {
        if pkg_dir.exists() {
            gpp_cmd.arg(format!("-I{}", pkg_dir.display()));
            if let Ok(entries) = std::fs::read_dir(&pkg_dir) {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        gpp_cmd.arg(format!("-I{}", entry.path().display()));
                    }
                }
            }
        }
    }
    if let Some(ref root) = project_root {
        gpp_cmd.arg(format!("-I{}", root.display()));
    }
    for s_dir in &std_dirs {
        gpp_cmd.arg(format!("-I{}", s_dir.display()));
        if let Some(parent) = s_dir.parent() {
            gpp_cmd.arg(format!("-I{}", parent.display()));
        }
    }

    let status = gpp_cmd.status();

    match status {
        Ok(s) if s.success() => {
            println!("Compilation successful! Executable is {}", exe_path);
            if clean_artifacts {
                let _ = fs::remove_file(&out_path);
                let _ = fs::remove_dir_all(&lib_dir);
                let _ = fs::remove_dir_all(&clib_dir);
            }
            if run_after_compile {
                println!("\x1b[36;1m--- [Running {}] ---\x1b[0m\n", exe_path);
                let mut run_cmd = std::process::Command::new(&exe_path);
                run_cmd.args(&run_args);
                match run_cmd.status() {
                    Ok(r_status) => {
                        println!();
                        if let Some(code) = r_status.code() {
                            if code != 0 {
                                std::process::exit(code);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to execute '{}': {}", exe_path, e);
                        std::process::exit(1);
                    }
                }
            }
        }
        _ => {
            eprintln!("C++ compilation failed! Check {} for errors.", out_path);
            std::process::exit(1);
        }
    }
}
