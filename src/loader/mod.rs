use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::frontend::lexer::scanner::Scanner;
use crate::frontend::lexer::token::TokenKind;
use crate::frontend::parser::ast::{Decl, Stmt, TypeMetadata, Visibility};
use crate::frontend::parser::parser::Parser;
use crate::project::{resolve_and_fetch_dependency, FastManifest, GlobalConfig};
use crate::{extract_line_col_from_msg, report_visual_error};
pub fn merge_metadata(target: &mut HashMap<String, TypeMetadata>, incoming: HashMap<String, TypeMetadata>) {
    for (k, v) in incoming {
        let entry = target.entry(k).or_insert_with(|| v.clone());
        entry.methods.extend(v.methods);
        entry.fields.extend(v.fields);
        entry.handles.extend(v.handles);
        entry.handle_signatures.extend(v.handle_signatures);
    }
}

pub struct LoadedModule {
    pub name: String,
    pub path: String,
    pub ast: Vec<Stmt>,
}

pub struct Program {
    pub main_ast: Vec<Stmt>,
    pub modules: Vec<LoadedModule>,
    pub global_metadata: HashMap<String, TypeMetadata>,
    pub main_deps: Vec<(String, Option<Vec<String>>)>,
}

pub struct ProjectLoader {
    pub include_paths: Vec<String>,
    pub project_root: Option<PathBuf>,
    pub packages_dir: Option<PathBuf>,
    pub global_libs: Vec<PathBuf>,
    pub manifest: Option<FastManifest>,
    pub verbose: bool,
    pub macro_patterns: Vec<Decl>,
    pub loaded_macro_paths: HashSet<PathBuf>,
}

pub fn get_std_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(custom) = std::env::var("FAST_STD_PATH") {
        let p = PathBuf::from(custom);
        if p.exists() {
            dirs.push(p);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut curr = exe.clone();
        for _ in 0..5 {
            if curr.pop() {
                let test_src_std = curr.join("src").join("std");
                if test_src_std.exists() && !dirs.contains(&test_src_std) {
                    dirs.push(test_src_std);
                }
                let test_std = curr.join("std");
                if test_std.exists() && !dirs.contains(&test_std) {
                    dirs.push(test_std);
                }
            }
        }
    }
    let local_src_std = PathBuf::from("src/std");
    if local_src_std.exists() && !dirs.contains(&local_src_std) {
        dirs.push(local_src_std);
    }
    let local_std = PathBuf::from("std");
    if local_std.exists() && !dirs.contains(&local_std) {
        dirs.push(local_std);
    }
    dirs
}

impl Default for ProjectLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectLoader {
    pub fn new() -> Self {
        let global_libs = GlobalConfig::load().get_resolved_paths();
        let mut includes = vec![
            "src/".to_string(),
            "src/examples/".to_string(),
            "src/std/".to_string(),
        ];
        for s_dir in get_std_dirs() {
            let path_str = format!("{}/", s_dir.display());
            if !includes.contains(&path_str) {
                includes.push(path_str);
            }
        }
        ProjectLoader {
            include_paths: includes,
            project_root: None,
            packages_dir: None,
            global_libs,
            manifest: None,
            verbose: false,
            macro_patterns: Vec::new(),
            loaded_macro_paths: HashSet::new(),
        }
    }

    pub fn new_with_project(project_root: PathBuf, manifest: Option<FastManifest>) -> Self {
        let mut loader = Self::new();
        loader.project_root = Some(project_root.clone());
        loader.manifest = manifest.clone();
        let pkg_dir = manifest
            .as_ref()
            .map(|m| m.get_packages_dir(&project_root))
            .unwrap_or_else(|| project_root.join("packages"));
        loader.packages_dir = Some(pkg_dir);

        if let Some(ref m) = manifest {
            if let Some(ref paths) = m.paths {
                if let Some(ref libs) = paths.libraries {
                    for lib in libs {
                        let p = PathBuf::from(lib);
                        if p.exists() {
                            loader.global_libs.push(p);
                        }
                    }
                }
            }
        }

        let src_dir = project_root.join("src");
        if src_dir.exists() {
            loader.include_paths.insert(0, format!("{}/", src_dir.display()));
        }
        loader.include_paths.insert(0, format!("{}/", project_root.display()));
        loader
    }

    pub fn resolve_path(&self, mod_name: &str) -> Option<String> {
        let extensions = [".fast", ".fs"];

        // 1. Relative paths (e.g. ./utils or ../shared)
        if mod_name.starts_with("./") || mod_name.starts_with("../") {
            for ext in &extensions {
                let test = format!("{}{}", mod_name, ext);
                if Path::new(&test).exists() {
                    return Some(test);
                }
                if let Some(ref root) = self.project_root {
                    let full = root.join(&test);
                    if full.exists() {
                        return Some(full.to_string_lossy().to_string());
                    }
                }
            }
        }

        // 2. Standard library (std/...)
        let std_dirs = get_std_dirs();
        for ext in &extensions {
            if let Some(sub) = mod_name.strip_prefix("std/") {
                for s_dir in &std_dirs {
                    let test = s_dir.join(format!("{}{}", sub, ext));
                    if test.exists() {
                        return Some(test.to_string_lossy().to_string());
                    }
                }
                let test = format!("src/{}{}", mod_name, ext);
                if Path::new(&test).exists() {
                    return Some(test);
                }
                let test_nostd = format!("{}{}", mod_name, ext);
                if Path::new(&test_nostd).exists() {
                    return Some(test_nostd);
                }
            } else if mod_name == "std" {
                for s_dir in &std_dirs {
                    let test = s_dir.join(format!("std{}", ext));
                    if test.exists() {
                        return Some(test.to_string_lossy().to_string());
                    }
                }
                let test = format!("src/std/std{}", ext);
                if Path::new(&test).exists() {
                    return Some(test);
                }
                let test_nostd = format!("std/std{}", ext);
                if Path::new(&test_nostd).exists() {
                    return Some(test_nostd);
                }
            }
        }

        // 3. Local packages/ directory (e.g. packages/<mod_name>/src/lib.fast or packages/<mod_name>.fast)
        if let Some(ref pkg_dir) = self.packages_dir {
            if pkg_dir.exists() {
                for ext in &extensions {
                    let candidates = [
                        pkg_dir.join(mod_name).join("src").join(format!("lib{}", ext)),
                        pkg_dir.join(mod_name).join("src").join(format!("main{}", ext)),
                        pkg_dir.join(mod_name).join(format!("lib{}", ext)),
                        pkg_dir.join(mod_name).join(format!("{}{}", mod_name, ext)),
                        pkg_dir.join(format!("{}{}", mod_name, ext)),
                    ];
                    for c in &candidates {
                        if c.exists() {
                            return Some(c.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }

        // 4. Global registered library paths
        for lib_dir in &self.global_libs {
            if lib_dir.exists() {
                for ext in &extensions {
                    let candidates = [
                        lib_dir.join(mod_name).join("src").join(format!("lib{}", ext)),
                        lib_dir.join(mod_name).join(format!("{}{}", mod_name, ext)),
                        lib_dir.join(format!("{}{}", mod_name, ext)),
                    ];
                    for c in &candidates {
                        if c.exists() {
                            return Some(c.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }

        // 5. Project root and include paths
        for ext in &extensions {
            for path in &self.include_paths {
                let test = format!("{}{}{}", path, mod_name, ext);
                if Path::new(&test).exists() {
                    return Some(test);
                }
            }

            let local_test = format!("{}{}", mod_name, ext);
            if Path::new(&local_test).exists() {
                return Some(local_test);
            }
        }
        None
    }

    fn collect_deps_from_ast(ast: &[Stmt]) -> Vec<(String, Option<Vec<String>>)> {
        let mut deps = Vec::new();
        for stmt in ast {
            if let Stmt::Declaration(Decl::Import {
                module_path,
                imports,
                abi,
                ..
            }) = stmt
            {
                if abi.is_some() || module_path.first().is_some_and(|p| p.ends_with(".h") || p.ends_with(".hpp")) {
                    continue;
                }
                let mut path_clone = module_path.clone();
                let mod_name = path_clone.join("/");

                if let Some(selected) = imports {
                    let mut found_submodule = false;
                    for sym in selected {
                        let sub_mod = format!("{}/{}", mod_name, sym);
                        if ProjectLoader::resolve_path_static(&sub_mod).is_some() {
                            deps.push((sub_mod, Some(vec![sym.clone()])));
                            found_submodule = true;
                        }
                    }

                    if !found_submodule {
                        if ProjectLoader::resolve_path_static(&mod_name).is_some() {
                            deps.push((mod_name, Some(selected.clone())));
                        } else if path_clone.len() > 1 {
                            let last = path_clone.pop().unwrap();
                            let parent_mod = path_clone.join("/");
                            if ProjectLoader::resolve_path_static(&parent_mod).is_some() {
                                deps.push((parent_mod, Some(vec![last])));
                            }
                        }
                    }
                } else if ProjectLoader::resolve_path_static(&mod_name).is_some() {
                    deps.push((mod_name, None));
                }
            }
        }
        deps
    }

    fn resolve_path_static(mod_name: &str) -> Option<String> {
        let extensions = [".fast", ".fs"];
        let std_dirs = get_std_dirs();
        for ext in &extensions {
            if let Some(sub) = mod_name.strip_prefix("std/") {
                for s_dir in &std_dirs {
                    let test = s_dir.join(format!("{}{}", sub, ext));
                    if test.exists() {
                        return Some(test.to_string_lossy().to_string());
                    }
                }
                let test = format!("src/{}{}", mod_name, ext);
                if Path::new(&test).exists() {
                    return Some(test);
                }
            } else if mod_name == "std" {
                for s_dir in &std_dirs {
                    let test = s_dir.join(format!("std{}", ext));
                    if test.exists() {
                        return Some(test.to_string_lossy().to_string());
                    }
                }
                let test = format!("src/std/std{}", ext);
                if Path::new(&test).exists() {
                    return Some(test);
                }
            }

            if let Ok(curr) = std::env::current_dir() {
                let pkg_dir = curr.join("packages");
                if pkg_dir.exists() {
                    let candidates = [
                        pkg_dir.join(mod_name).join("src").join(format!("lib{}", ext)),
                        pkg_dir.join(mod_name).join("src").join(format!("main{}", ext)),
                        pkg_dir.join(mod_name).join(format!("lib{}", ext)),
                        pkg_dir.join(mod_name).join(format!("{}{}", mod_name, ext)),
                        pkg_dir.join(format!("{}{}", mod_name, ext)),
                    ];
                    for c in &candidates {
                        if c.exists() {
                            return Some(c.to_string_lossy().to_string());
                        }
                    }
                }
            }

            for path in &["src/", "src/examples/", "src/std/"] {
                let test = format!("{}{}{}", path, mod_name, ext);
                if Path::new(&test).exists() {
                    return Some(test);
                }
            }

            let local_test = format!("{}{}", mod_name, ext);
            if Path::new(&local_test).exists() {
                return Some(local_test);
            }
        }
        None
    }

    pub fn process_file_dependencies(&mut self, content: &str) {
        let mut idx = 0;
        while let Some(pos) = content[idx..].find("@dependency") {
            let start = idx + pos + "@dependency".len();
            if let Some(brace_open) = content[start..].find('{') {
                let inner_start = start + brace_open + 1;
                let mut depth = 1;
                let mut inner_end = None;
                for (byte_offset, ch) in content[inner_start..].char_indices() {
                    if ch == '{' {
                        depth += 1;
                    } else if ch == '}' {
                        depth -= 1;
                        if depth == 0 {
                            inner_end = Some(inner_start + byte_offset);
                            break;
                        }
                    }
                }
                if let Some(end) = inner_end {
                    let block_toml = &content[inner_start..end];
                    self.resolve_dependency_block(block_toml);
                    idx = end + 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }

    fn resolve_dependency_block(&mut self, block: &str) {
        let val_res: Result<toml::Value, _> = toml::from_str(block)
            .or_else(|_| toml::from_str(&format!("[dependencies]\n{}", block)));

        if let Ok(val) = val_res {
            let mut extracted = Vec::new();
            if let Some(table) = val.as_table() {
                for key in &["dependencies", "packages", "dependency", "package"] {
                    if let Some(arr) = table.get(*key).and_then(|v| v.as_array()) {
                        for item in arr {
                            if let Some(item_table) = item.as_table() {
                                if let Some(n) = item_table.get("name").and_then(|v| v.as_str()) {
                                    let ver = item_table.get("version").and_then(|v| v.as_str()).map(|s| s.to_string());
                                    extracted.push((n.to_string(), ver));
                                }
                            } else if let Some(n) = item.as_str() {
                                extracted.push((n.to_string(), None));
                            }
                        }
                    } else if let Some(t) = table.get(*key).and_then(|v| v.as_table()) {
                        for (k, v) in t {
                            let ver = match v {
                                toml::Value::String(s) => Some(s.clone()),
                                toml::Value::Table(inner) => inner.get("version").and_then(|x| x.as_str()).map(|s| s.to_string()),
                                _ => None,
                            };
                            extracted.push((k.clone(), ver));
                        }
                    }
                }

                for (k, v) in table {
                    if k == "dependencies" || k == "packages" || k == "dependency" || k == "package" {
                        continue;
                    }
                    let ver = match v {
                        toml::Value::String(s) => Some(s.clone()),
                        toml::Value::Table(inner) => inner.get("version").and_then(|x| x.as_str()).map(|s| s.to_string()),
                        _ => None,
                    };
                    extracted.push((k.clone(), ver));
                }
            }

            let packages_dir = self.packages_dir.clone().unwrap_or_else(|| {
                if let Some(ref root) = self.project_root {
                    root.join("packages")
                } else {
                    PathBuf::from("packages")
                }
            });

            for (name, ver) in extracted {
                if let Some(ref mut m) = self.manifest {
                    let deps = m.dependencies.get_or_insert_with(HashMap::new);
                    if !deps.contains_key(&name) {
                        deps.insert(name.clone(), ver.clone().unwrap_or_else(|| "latest".to_string()));
                        if let Some(ref root) = self.project_root {
                            let _ = m.save_to_file(&root.join("Fast.toml"));
                        }
                    }
                }

                if let Ok(pkg_path) = resolve_and_fetch_dependency(
                    &name,
                    ver.as_deref(),
                    &packages_dir,
                ) {
                    let path_str = format!("{}/", pkg_path.display());
                    if !self.include_paths.contains(&path_str) {
                        self.include_paths.insert(0, path_str);
                    }
                }
            }
        }
    }

    fn resolve_import_deps(
        &mut self,
        deps: &[(String, Option<Vec<String>>)],
        loaded_names: &mut HashSet<String>,
        loaded_paths: &mut HashSet<PathBuf>,
        loaded_modules: &mut Vec<LoadedModule>,
        global_metadata: &mut HashMap<String, TypeMetadata>,
    ) -> Result<(), String> {
        for (mod_name, _imports) in deps {
            if loaded_names.contains(mod_name) {
                continue;
            }

            let actual_path = match self.resolve_path(mod_name) {
                Some(p) => p,
                None => return Err(format!("Module '{}' not found.", mod_name)),
            };
            let canonical_path = Path::new(&actual_path)
                .canonicalize()
                .unwrap_or_else(|_| Path::new(&actual_path).to_path_buf());

            if loaded_paths.contains(&canonical_path) {
                loaded_names.insert(mod_name.clone());
                continue;
            }

            if self.verbose {
                println!("Loading module {}...", mod_name);
            }
            let mod_ast = self.parse_file(&actual_path)?;
            merge_metadata(global_metadata, mod_ast.1);

            loaded_names.insert(mod_name.clone());
            loaded_paths.insert(canonical_path);

            let nested_deps = Self::collect_deps_from_ast(&mod_ast.0);
            self.resolve_import_deps(
                &nested_deps,
                loaded_names,
                loaded_paths,
                loaded_modules,
                global_metadata,
            )?;

            loaded_modules.push(LoadedModule {
                name: mod_name.clone(),
                path: actual_path,
                ast: mod_ast.0,
            });
        }

        Ok(())
    }

    pub fn preload_macros_from_file(&mut self, path: &str) {
        let p = Path::new(path);
        let canonical = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        if self.loaded_macro_paths.contains(&canonical) {
            return;
        }
        self.loaded_macro_paths.insert(canonical);

        let Ok(contents) = std::fs::read_to_string(path) else {
            return;
        };

        let mut scanner = Scanner::new(contents);
        let mut tokens = Vec::new();
        let mut has_macro = false;
        let mut has_import = false;

        loop {
            let tok = scanner.next_token();
            let is_eof = tok.kind == TokenKind::EOF;
            if tok.kind == TokenKind::TypeMacro {
                has_macro = true;
            }
            if tok.kind == TokenKind::Import {
                has_import = true;
            }
            match tok.kind {
                TokenKind::InlineComment | TokenKind::MultiLineComment => {}
                _ => tokens.push(tok),
            }
            if is_eof {
                break;
            }
        }

        if has_import {
            for i in 0..tokens.len() {
                if tokens[i].kind == TokenKind::Import {
                    let mut parts = Vec::new();
                    let mut j = i + 1;
                    while j < tokens.len() {
                        match &tokens[j].kind {
                            TokenKind::Identifier(name) | TokenKind::LabelName(name) => {
                                parts.push(name.clone());
                            }
                            TokenKind::DoubleColon | TokenKind::Divide => {}
                            _ => break,
                        }
                        j += 1;
                    }
                    if !parts.is_empty() {
                        let mod_name = parts.join("/");
                        if let Some(mod_path) = self.resolve_path(&mod_name) {
                            self.preload_macros_from_file(&mod_path);
                        }
                    }
                }
            }
        }

        if has_macro {
            let mut parser = Parser::new(tokens).with_macro_patterns(self.macro_patterns.clone());
            if let Ok(prog) = parser.parse_program() {
                for s in prog.statements {
                    match s {
                        Stmt::Declaration(Decl::MacroDecl { .. }) => {
                            if let Stmt::Declaration(d) = s {
                                self.macro_patterns.push(d);
                            }
                        }
                        Stmt::Declaration(Decl::NamespaceDecl { decls, .. }) => {
                            for d in decls {
                                if matches!(d, Decl::MacroDecl { .. }) {
                                    self.macro_patterns.push(d);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn parse_file(&mut self, path: &str) -> Result<(Vec<Stmt>, HashMap<String, TypeMetadata>), String> {
        let contents = std::fs::read_to_string(path)
            .map_err(|_| format!("Could not read '{}'. Make sure the file exists.", path))?;

        self.process_file_dependencies(&contents);
    
        let mut scanner = Scanner::new(contents.clone());
        let mut tokens = Vec::new();
    
        loop {
            let tok = scanner.next_token();
            let is_eof = tok.kind == TokenKind::EOF;
    
            if let TokenKind::Error(ref msg) = tok.kind {
                eprintln!("[Lexer Error in {}] {}", path, msg);
            }
    
            match tok.kind {
                TokenKind::InlineComment | TokenKind::MultiLineComment => {}
                _ => tokens.push(tok),
            }
    
            if is_eof {
                break;
            }
        }

        // Preload macros from imported modules
        for i in 0..tokens.len() {
            if tokens[i].kind == TokenKind::Import {
                let mut parts = Vec::new();
                let mut j = i + 1;
                while j < tokens.len() {
                    match &tokens[j].kind {
                        TokenKind::Identifier(name) | TokenKind::LabelName(name) => {
                            parts.push(name.clone());
                        }
                        TokenKind::DoubleColon | TokenKind::Divide => {}
                        _ => break,
                    }
                    j += 1;
                }
                if !parts.is_empty() {
                    let mod_name = parts.join("/");
                    if let Some(mod_path) = self.resolve_path(&mod_name) {
                        self.preload_macros_from_file(&mod_path);
                    }
                }
            }
        }
    
        let mut parser = Parser::new(tokens).with_macro_patterns(self.macro_patterns.clone());
        let prog = parser.parse_program();

        match prog {
            Ok(program) => {
                for s in &program.statements {
                    match s {
                        Stmt::Declaration(d @ Decl::MacroDecl { .. }) => {
                            self.macro_patterns.push(d.clone());
                        }
                        Stmt::Declaration(Decl::NamespaceDecl { decls, .. }) => {
                            for d in decls {
                                if matches!(d, Decl::MacroDecl { .. }) {
                                    self.macro_patterns.push(d.clone());
                                }
                            }
                        }
                        _ => {}
                    }
                }
                let mut metadata = parser.metadata.clone();
                let set_ns_opt = program.statements.iter().find_map(|s| {
                    if let Stmt::SetNamespace(ns) = s {
                        Some(ns.clone())
                    } else {
                        None
                    }
                });

                let stmts = if let Some(ns) = set_ns_opt {
                    let resolved_ns = if ns == "." {
                        let p = Path::new(path);
                        let folder_name = p.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or("");
                        let stem_name = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                        if !folder_name.is_empty() && folder_name != "src" && folder_name != "std" && folder_name != "." {
                            folder_name.to_string()
                        } else {
                            stem_name.to_string()
                        }
                    } else {
                        ns
                    };

                    let current_keys: Vec<String> = metadata.keys().cloned().collect();
                    for k in current_keys {
                        let meta = metadata[&k].clone();
                        let qual = format!("{}::{}", resolved_ns, k);
                        metadata.insert(qual, meta);
                    }

                    let mut top_stmts = Vec::new();
                    let mut inner_decls = Vec::new();
                    for s in program.statements {
                        match s {
                            Stmt::SetNamespace(_) => {}
                            Stmt::Declaration(Decl::Import { .. }) => top_stmts.push(s),
                            Stmt::UsingStmt(_) => top_stmts.push(s),
                            Stmt::Declaration(mut d) => {
                                match &mut d {
                                    Decl::FnDecl { ref mut visibility, .. }
                                    | Decl::BlueprintDecl { ref mut visibility, .. }
                                    | Decl::ClassDecl { ref mut visibility, .. }
                                    | Decl::StructDecl { ref mut visibility, .. }
                                    | Decl::EnumDecl { ref mut visibility, .. }
                                    | Decl::MachineDecl { ref mut visibility, .. }
                                    | Decl::MacroDecl { ref mut visibility, .. } => {
                                        *visibility = Visibility::Public;
                                    }
                                    _ => {}
                                }
                                inner_decls.push(d);
                            }
                            _ => top_stmts.push(s),
                        }
                    }

                    top_stmts.push(Stmt::Declaration(Decl::NamespaceDecl {
                        visibility: Visibility::Public,
                        name: resolved_ns,
                        decls: inner_decls,
                    }));

                    top_stmts
                } else {
                    program.statements
                };

                Ok((stmts, metadata))
            }
            Err(err_msg) => {
                let (line_opt, col_opt) = extract_line_col_from_msg(&err_msg);
                let clean_msg = if let Some(line_idx) = err_msg.find("at line ") {
                    err_msg[..line_idx].trim().trim_end_matches('(').trim().to_string()
                } else {
                    err_msg.clone()
                };

                if let Some(l) = line_opt {
                    report_visual_error(&contents, path, l, col_opt.unwrap_or(1), &clean_msg);
                } else {
                    eprintln!("\n\x1b[31;1merror:\x1b[0m {}\n", clean_msg);
                }

                Err(err_msg)
            }
        }
    }

    pub fn load(&mut self, path: &str, target_module: Option<&str>) -> Result<Program, String> {
        let entry_path = if let Some(target) = target_module {
            self.resolve_path(target).ok_or_else(|| format!("Target module '{}' not found in include paths.", target))?
        } else {
            path.to_string()
        };

        if !Path::new(&entry_path).exists() {
            return Err(format!("Entry file '{}' not found.", entry_path));
        }

        if let Some(std_file) = self.resolve_path("std") {
            self.preload_macros_from_file(&std_file);
        }

        println!("Compiling {}...", entry_path);
        let mut global_metadata = HashMap::new();
        let main_ast = match self.parse_file(&entry_path) {
            Ok(res) => {
                merge_metadata(&mut global_metadata, res.1);
                res.0
            }
            Err(e) => return Err(e),
        };

        let deps = Self::collect_deps_from_ast(&main_ast);

        let canonical_entry = Path::new(&entry_path)
            .canonicalize()
            .unwrap_or_else(|_| Path::new(&entry_path).to_path_buf());

        let mut loaded_modules = Vec::new();
        let mut loaded_names = HashSet::new();
        let mut loaded_paths = HashSet::new();
        loaded_paths.insert(canonical_entry);

        // Auto-load standard library prelude (std.fs) if available
        if let Some(std_file) = self.resolve_path("std") {
            let canonical_std = Path::new(&std_file)
                .canonicalize()
                .unwrap_or_else(|_| Path::new(&std_file).to_path_buf());
            if Path::new(&std_file).exists() && !loaded_paths.contains(&canonical_std) {
                let std_ast = self.parse_file(&std_file)?;
                merge_metadata(&mut global_metadata, std_ast.1);
                loaded_names.insert("std".to_string());
                loaded_paths.insert(canonical_std);

                let nested_deps = Self::collect_deps_from_ast(&std_ast.0);
                self.resolve_import_deps(
                    &nested_deps,
                    &mut loaded_names,
                    &mut loaded_paths,
                    &mut loaded_modules,
                    &mut global_metadata,
                )?;
                loaded_modules.push(LoadedModule {
                    name: "std".to_string(),
                    path: std_file,
                    ast: std_ast.0,
                });
            }
        }

        self.resolve_import_deps(
            &deps,
            &mut loaded_names,
            &mut loaded_paths,
            &mut loaded_modules,
            &mut global_metadata,
        )?;

        Ok(Program {
            main_ast,
            modules: loaded_modules,
            global_metadata,
            main_deps: deps,
        })
    }
}
