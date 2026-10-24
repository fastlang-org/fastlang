use std::collections::HashMap;
use std::fs;
use std::io::{stdin, stdout, Write};
use std::path::{Path, PathBuf};
use std::process::{exit, Command};

use crate::project::{
    resolve_and_fetch_dependency, scaffold, DependencyRegistry, FastManifest, GlobalConfig,
    KnownDependency, ProjectType,
};

pub fn print_help() {
    println!(
        r#"FastLang Compiler (fast / fast_lang) v0.1.6-alpha
High-performance compiled language with fine-grained memory management and explicit scopes.

USAGE:
    fast [SUBCOMMAND] [OPTIONS]
    fast [OPTIONS] <SOURCE_FILE> [-- <ARGS>...]

SUBCOMMANDS:
    new <NAME> [--type bin|lib] [-y]   Create a new FastLang project package
    init [--type bin|lib] [-y]         Initialize a FastLang package in current directory
    run [SCRIPT|FILE] [ARGS...]        Run project entry point, named script, or specific source file
    build [FILE] [OPTIONS]             Compile project or source file without executing
    test                               Discover and run all project tests in tests/
    clean [PATH]                       Delete build/ directory and cached build artifacts
    lib <list|add|remove> [PATH]       Manage compiler-wide global library search paths
    registry <list|add|import> [...]   Manage known package registry and dependency URLs
    pkg <add|install> [NAME]           Install or add dependencies to current project

ARGUMENTS:
    <SOURCE_FILE>                  Path to the main entry source file (.fast or .fs)
    -- <ARGS>...                   Pass arguments directly to the executed program

OPTIONS:
    -h, --help, -help              Print this help information and exit
    -d, --debug, -v, --verbose     Enable verbose debug and compilation trace output
    --debug-error, --debug-errors  Accumulate and report all semantic errors without generating code
    -o, --output <PATH>            Specify output binary or build directory path
    -I, --include <DIR>            Add module search directory
    -b, --backend <BACKEND>        Set code generator backend: 'cpp' (C++20, default) or 'cranelift'
    --ast, --emit-ast [FILE]       Dump parsed AST as formatted JSON (to stdout or FILE)
    --ast-out, --ast-file, --json-ast <FILE>
                                   Export parsed AST as formatted JSON directly to FILE
    --emit-cpp, --cpp-only         Generate and emit C++ source file without compiling binary
    --emit-ir, --print-ir          Print intermediate representation (IR) output
    --ir-out, --ir-file <FILE>     Write IR output directly to specified file
    --aot                          Enable Ahead-Of-Time (AOT) compilation via Cranelift backend
    --target <TARGET>              Specify target architecture/platform
    --clean                        Remove temporary C++ and library files after successful build

PROJECT MANIFEST (Fast.toml):
    [package]
    name = "my_app"
    version = "0.1.0"
    type = "bin"                   # "bin" (executable) or "module" / "lib" (library)
    main = "src/main.fast"         # Entry source file
    backend = "cpp"                # Preferred compiler backend ("cpp" or "cranelift")

    [scripts]
    dev = "fast run"               # Custom task runner scripts

    [paths]
    packages = "packages"          # Local package dependencies folder
    libraries = []                 # Additional library search paths

EXAMPLES:
    fast new my_app
    fast new my_lib --type lib -y
    fast run
    fast run my_script
    fast run -- arg1 arg2
    fast build
    fast test
    fast lib add C:/libs/my_shared_lib
    fast lib list
    fast main.fast -o build/main.exe
    fast clean
"#
    );
}

#[allow(clippy::result_unit_err)]
pub fn run_project_tests(project_root: &Path) -> Result<(), ()> {
    let tests_dir = project_root.join("tests");
    if !tests_dir.exists() {
        println!("No tests directory found at '{}'.", tests_dir.display());
        return Ok(());
    }

    let mut entries: Vec<PathBuf> = match fs::read_dir(&tests_dir) {
        Ok(read_dir) => read_dir
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
                    (ext == "fast" || ext == "fs") && p.is_file()
                } else {
                    false
                }
            })
            .collect(),
        Err(e) => {
            eprintln!("Failed to read tests directory: {}", e);
            return Err(());
        }
    };

    if entries.is_empty() {
        println!("No test files found in '{}'.", tests_dir.display());
        return Ok(());
    }

    entries.sort();

    println!("\n=== Running FastLang Project Tests ===");
    println!(
        "Found {} test(s) in {}\n",
        entries.len(),
        tests_dir.display()
    );

    let self_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("fast_lang"));
    let mut passed = 0;
    let mut failed = 0;

    for test_path in entries {
        let file_name = test_path.file_name().unwrap_or_default().to_string_lossy();
        let stem = test_path.file_stem().unwrap_or_default().to_string_lossy();
        print!("--> Running test: {} ... ", file_name);
        let _ = stdout().flush();

        let build_dir = project_root.join("build");
        let _ = fs::create_dir_all(&build_dir);
        let test_bin = build_dir.join(format!("{}.exe", stem));

        let compile_output = Command::new(&self_exe)
            .arg(&test_path)
            .arg("-o")
            .arg(&test_bin)
            .output();

        match compile_output {
            Ok(out) if out.status.success() => {
                let run_output = Command::new(&test_bin).output();
                match run_output {
                    Ok(r_out) if r_out.status.success() => {
                        println!("PASSED");
                        passed += 1;
                    }
                    Ok(r_out) => {
                        println!("FAILED (runtime exit code: {:?})", r_out.status.code());
                        let stderr = String::from_utf8_lossy(&r_out.stderr);
                        if !stderr.trim().is_empty() {
                            eprintln!("    Stderr: {}", stderr.trim());
                        }
                        failed += 1;
                    }
                    Err(e) => {
                        println!("FAILED to execute binary: {}", e);
                        failed += 1;
                    }
                }
            }
            Ok(out) => {
                println!("FAILED to compile");
                let stderr = String::from_utf8_lossy(&out.stderr);
                if !stderr.trim().is_empty() {
                    eprintln!("    Compiler error: {}", stderr.trim());
                }
                failed += 1;
            }
            Err(e) => {
                println!("FAILED to run compiler: {}", e);
                failed += 1;
            }
        }
    }

    println!("\nTest Results: {} passed, {} failed\n", passed, failed);
    if failed > 0 {
        Err(())
    } else {
        Ok(())
    }
}

/// Handles early standalone CLI subcommands.
/// Returns true if a command was handled and execution should exit.
pub fn handle_subcommands(args: &[String]) -> bool {
    if args.len() == 1
        || args.contains(&"--help".to_string())
        || args.contains(&"-h".to_string())
        || args.contains(&"-help".to_string())
    {
        print_help();
        return true;
    }

    // 1. Subcommand: clean
    if args.len() > 1 && args[1] == "clean" {
        let target_dir = if args.len() > 2 {
            PathBuf::from(&args[2])
        } else {
            PathBuf::from(".")
        };
        let build_dir = if target_dir.file_name().and_then(|s| s.to_str()) == Some("build") {
            target_dir
        } else {
            target_dir.join("build")
        };
        if build_dir.exists() {
            if let Err(e) = fs::remove_dir_all(&build_dir) {
                eprintln!("Failed to clean {}: {}", build_dir.display(), e);
                exit(1);
            }
            println!("Cleaned build directory: {}", build_dir.display());
        } else {
            println!(
                "Nothing to clean. ({}) does not exist.",
                build_dir.display()
            );
        }
        return true;
    }

    // 2. Subcommand: new <name> [--type bin|lib] [-y]
    if args.len() > 1 && args[1] == "new" {
        let mut proj_name: Option<String> = None;
        let mut p_type: Option<ProjectType> = None;
        let mut non_interactive = false;

        let mut idx = 2;
        while idx < args.len() {
            if (args[idx] == "--type" || args[idx] == "-t") && idx + 1 < args.len() {
                p_type = ProjectType::from_str_opt(&args[idx + 1]);
                idx += 2;
            } else if args[idx] == "-y" || args[idx] == "--yes" {
                non_interactive = true;
                idx += 1;
            } else if !args[idx].starts_with('-') && proj_name.is_none() {
                proj_name = Some(args[idx].clone());
                idx += 1;
            } else {
                idx += 1;
            }
        }

        let name = match proj_name {
            Some(n) => n,
            None => {
                if non_interactive {
                    eprintln!("Error: Project name is required. Usage: fast new <name> [--type bin|lib] [-y]");
                    exit(1);
                }
                print!("Enter project name: ");
                let _ = stdout().flush();
                let mut input = String::new();
                stdin().read_line(&mut input).unwrap_or_default();
                let trimmed = input.trim();
                if trimmed.is_empty() {
                    eprintln!("Project name cannot be empty.");
                    exit(1);
                }
                trimmed.to_string()
            }
        };

        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        match scaffold::create_new_project(&name, &cwd, p_type, non_interactive) {
            Ok(dir) => {
                println!(
                    "Created new FastLang project '{}' at {}",
                    name,
                    dir.display()
                );
                println!("Get started with:");
                if name != "." {
                    println!("  cd {}", name);
                }
                println!("  fast run");
            }
            Err(e) => {
                eprintln!("Error creating project: {}", e);
                exit(1);
            }
        }
        return true;
    }

    // 3. Subcommand: init [--type bin|lib] [-y]
    if args.len() > 1 && args[1] == "init" {
        let mut p_type: Option<ProjectType> = None;
        let mut _non_interactive = false;

        let mut idx = 2;
        while idx < args.len() {
            if (args[idx] == "--type" || args[idx] == "-t") && idx + 1 < args.len() {
                p_type = ProjectType::from_str_opt(&args[idx + 1]);
                idx += 2;
            } else if args[idx] == "-y" || args[idx] == "--yes" {
                _non_interactive = true;
                idx += 1;
            } else {
                idx += 1;
            }
        }

        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let proj_name = cwd
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("my_project")
            .to_string();
        let target_type = p_type.unwrap_or(ProjectType::Bin);

        match scaffold::init_project_in_dir(&cwd, proj_name, target_type) {
            Ok(_) => {
                println!("Initialized FastLang project in '{}'", cwd.display());
                println!("Run project with: fast run");
            }
            Err(e) => {
                eprintln!("Error initializing project: {}", e);
                exit(1);
            }
        }
        return true;
    }

    // 4. Subcommand: lib <list | add <path> | remove <path>>
    if args.len() > 1 && args[1] == "lib" {
        let sub = args.get(2).map(|s| s.as_str()).unwrap_or("list");
        match sub {
            "list" => {
                let config = GlobalConfig::load();
                if config.library_paths.is_empty() {
                    println!(
                        "No global library paths registered. Add one using 'fast lib add <path>'."
                    );
                } else {
                    println!("Registered Global Library Paths:");
                    for (i, p) in config.library_paths.iter().enumerate() {
                        println!("  [{}] {}", i + 1, p);
                    }
                }
            }
            "add" => {
                let lib_path_arg = match args.get(3) {
                    Some(p) => p,
                    None => {
                        eprintln!("Usage: fast lib add <path>");
                        exit(1);
                    }
                };
                let lib_path = Path::new(lib_path_arg);
                if !lib_path.exists() {
                    eprintln!(
                        "Warning: path '{}' does not currently exist on disk.",
                        lib_path.display()
                    );
                }
                let mut config = GlobalConfig::load();
                match config.add_library_path(lib_path) {
                    Ok(_) => {
                        println!("Added global library path: {}", lib_path.display());
                    }
                    Err(e) => {
                        eprintln!("Failed to save global config: {}", e);
                        exit(1);
                    }
                }
            }
            "remove" | "rm" => {
                let lib_path_arg = match args.get(3) {
                    Some(p) => p,
                    None => {
                        eprintln!("Usage: fast lib remove <path>");
                        exit(1);
                    }
                };
                let mut config = GlobalConfig::load();
                match config.remove_library_path(lib_path_arg) {
                    Ok(true) => {
                        println!("Removed global library path: {}", lib_path_arg);
                    }
                    Ok(false) => {
                        println!(
                            "Path '{}' was not found in registered libraries.",
                            lib_path_arg
                        );
                    }
                    Err(e) => {
                        eprintln!("Failed to save global config: {}", e);
                        exit(1);
                    }
                }
            }
            other => {
                eprintln!(
                    "Unknown lib subcommand '{}'. Use 'add', 'list', or 'remove'.",
                    other
                );
                exit(1);
            }
        }
        return true;
    }

    // 4b. Subcommand: registry <list | add <name> <url> [version] [desc] | import <file>>
    if args.len() > 1 && args[1] == "registry" {
        let sub = args.get(2).map(|s| s.as_str()).unwrap_or("list");
        match sub {
            "list" => {
                let reg = DependencyRegistry::load();
                let list = reg.list();
                if list.is_empty() {
                    println!("No dependencies registered in known registry.");
                } else {
                    println!("Known Dependency Registry ({} packages):", list.len());
                    println!("{:<15} {:<10} {:<45} DESCRIPTION", "NAME", "VERSION", "URL");
                    println!("{:-<15} {:-<10} {:-<45} {:-<20}", "", "", "", "");
                    for dep in list {
                        let ver = dep.version.as_deref().unwrap_or("-");
                        let desc = dep.description.as_deref().unwrap_or("");
                        println!("{:<15} {:<10} {:<45} {}", dep.name, ver, dep.url, desc);
                    }
                }
            }
            "add" => {
                let name = match args.get(3) {
                    Some(n) => n.clone(),
                    None => {
                        eprintln!("Usage: fast registry add <name> <url> [version] [description]");
                        exit(1);
                    }
                };
                let url = match args.get(4) {
                    Some(u) => u.clone(),
                    None => {
                        eprintln!("Usage: fast registry add <name> <url> [version] [description]");
                        exit(1);
                    }
                };
                let version = args.get(5).cloned();
                let description = args.get(6).cloned();

                let mut reg = DependencyRegistry::load();
                let dep = KnownDependency {
                    name: name.clone(),
                    version,
                    url,
                    description,
                };
                match reg.add(dep) {
                    Ok(_) => println!(
                        "Successfully registered package '{}' in known registry.",
                        name
                    ),
                    Err(e) => {
                        eprintln!("Failed to save registry: {}", e);
                        exit(1);
                    }
                }
            }
            "import" => {
                let file_path = match args.get(3) {
                    Some(f) => Path::new(f),
                    None => {
                        eprintln!("Usage: fast registry import <path-to-dependencies.toml>");
                        exit(1);
                    }
                };
                let mut reg = DependencyRegistry::load();
                match reg.import_file(file_path) {
                    Ok(count) => {
                        println!(
                            "Successfully imported {} package(s) into known dependency registry.",
                            count
                        );
                    }
                    Err(e) => {
                        eprintln!("Import failed: {}", e);
                        exit(1);
                    }
                }
            }
            other => {
                eprintln!(
                    "Unknown registry subcommand '{}'. Use 'list', 'add', or 'import'.",
                    other
                );
                exit(1);
            }
        }
        return true;
    }

    // 4c. Subcommand: pkg <add <name> | install> (or fast add <name>)
    if (args.len() > 1 && args[1] == "pkg") || (args.len() > 1 && args[1] == "add") {
        let is_direct_add = args[1] == "add";
        let sub = if is_direct_add {
            "add"
        } else {
            args.get(2).map(|s| s.as_str()).unwrap_or("install")
        };
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let (manifest_opt, project_root) = match FastManifest::find_in_dir_or_parents(&cwd) {
            Some((m, root)) => (Some(m), root),
            None => {
                eprintln!("Error: No Fast.toml manifest found in current directory or parents.");
                exit(1);
            }
        };
        let mut manifest = manifest_opt.unwrap();
        let packages_dir = manifest.get_packages_dir(&project_root);

        match sub {
            "add" => {
                let pkg_name_idx = if is_direct_add { 2 } else { 3 };
                let pkg_name = match args.get(pkg_name_idx) {
                    Some(n) => n.clone(),
                    None => {
                        eprintln!("Usage: fast pkg add <name> (or fast add <name>)");
                        exit(1);
                    }
                };
                let req_version = args.get(pkg_name_idx + 1).cloned();

                match resolve_and_fetch_dependency(&pkg_name, req_version.as_deref(), &packages_dir)
                {
                    Ok(_) => {
                        let deps = manifest.dependencies.get_or_insert_with(HashMap::new);
                        deps.insert(
                            pkg_name.clone(),
                            req_version.unwrap_or_else(|| "latest".to_string()),
                        );
                        let _ = manifest.save_to_file(&project_root.join("Fast.toml"));
                        println!(
                            "Package '{}' added to Fast.toml and installed in packages/.",
                            pkg_name
                        );
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        exit(1);
                    }
                }
            }
            "install" => {
                if let Some(ref deps) = manifest.dependencies {
                    println!(
                        "Installing dependencies from Fast.toml ({} packages)...",
                        deps.len()
                    );
                    for (name, ver) in deps {
                        if let Err(e) =
                            resolve_and_fetch_dependency(name, Some(ver.as_str()), &packages_dir)
                        {
                            eprintln!("Warning: Failed to install package '{}': {}", name, e);
                        }
                    }
                    println!("Dependencies installation complete.");
                } else {
                    println!("No dependencies found in Fast.toml.");
                }
            }
            other => {
                eprintln!(
                    "Unknown pkg subcommand '{}'. Use 'add' or 'install'.",
                    other
                );
                exit(1);
            }
        }
        return true;
    }

    // 5. Subcommand: test
    if args.len() > 1 && args[1] == "test" {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let project_root = FastManifest::find_in_dir_or_parents(&cwd)
            .map(|(_, root)| root)
            .unwrap_or(cwd);
        if run_project_tests(&project_root).is_err() {
            exit(1);
        }
        return true;
    }

    // 6. Subcommand: run <script_name> (if defined in Fast.toml scripts)
    if args.len() > 1 && args[1] == "run" {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let manifest_info = FastManifest::find_in_dir_or_parents(&cwd);
        if let Some((ref manifest, ref project_root)) = manifest_info {
            if args.len() > 2 && !args[2].starts_with('-') && args[2] != "--" {
                if let Some(ref scripts) = manifest.scripts {
                    if let Some(cmd) = scripts.get(&args[2]) {
                        let trailing_args = &args[3..];
                        #[cfg(windows)]
                        let mut process = Command::new("cmd");
                        #[cfg(windows)]
                        {
                            process.current_dir(project_root);
                            process.arg("/C").arg(cmd);
                            for a in trailing_args {
                                process.arg(a);
                            }
                        }
                        #[cfg(not(windows))]
                        let mut process = {
                            let mut full_cmd = cmd.clone();
                            for a in trailing_args {
                                full_cmd.push(' ');
                                full_cmd.push_str(a);
                            }
                            let mut cmd_proc = Command::new("sh");
                            cmd_proc.current_dir(project_root);
                            cmd_proc.arg("-c").arg(&full_cmd);
                            cmd_proc
                        };

                        let status = match process.status() {
                            Ok(s) => s.code().unwrap_or(0),
                            Err(e) => {
                                eprintln!("Failed to execute script '{}': {}", args[2], e);
                                exit(1);
                            }
                        };
                        exit(status);
                    }
                }
            }
        }
    }

    false
}

#[derive(Debug, Clone)]
pub struct CliOptions {
    pub path: String,
    pub manifest_info: Option<(FastManifest, PathBuf)>,
    pub manifest: Option<FastManifest>,
    pub project_root: Option<PathBuf>,
    pub target: Option<String>,
    pub custom_includes: Vec<String>,
    pub backend: String,
    pub emit_ir: bool,
    pub ir_output_file: Option<String>,
    pub emit_cpp: bool,
    pub debug: bool,
    pub use_aot: bool,
    pub custom_output: Option<String>,
    pub ast_output_file: Option<String>,
    pub emit_ast: bool,
    pub clean_artifacts: bool,
    pub debug_errors: bool,
    pub run_after_compile: bool,
    pub is_build_cmd: bool,
    pub run_args: Vec<String>,
    pub is_test_file: bool,
    pub is_module_project: bool,
}

pub fn parse_cli(args: &[String]) -> CliOptions {
    let mut run_after_compile = false;
    let mut is_build_cmd = false;
    let mut i = 1;

    if args.len() > 1 && args[1] == "run" {
        run_after_compile = true;
        i = 2;
    } else if args.len() > 1 && args[1] == "build" {
        is_build_cmd = true;
        i = 2;
    }

    let mut user_target_file: Option<String> = None;
    let mut target = None;
    let mut custom_includes = Vec::new();
    let mut cli_backend: Option<String> = None;
    let mut emit_ir = false;
    let mut ir_output_file: Option<String> = None;
    let mut emit_cpp = false;
    let mut debug = false;
    let mut use_aot = false;
    let mut custom_output: Option<String> = None;
    let mut ast_output_file: Option<String> = None;
    let mut emit_ast = false;
    let mut clean_artifacts = false;
    let mut debug_errors = false;
    let mut run_args = Vec::new();

    while i < args.len() {
        if args[i] == "--" {
            run_args.extend(args[i + 1..].iter().cloned());
            break;
        } else if args[i] == "--help" || args[i] == "-h" || args[i] == "-help" {
            print_help();
            exit(0);
        } else if args[i] == "--target" && i + 1 < args.len() {
            target = Some(args[i + 1].clone());
            i += 2;
        } else if (args[i] == "-I" || args[i] == "--include") && i + 1 < args.len() {
            let include_path = args[i + 1].clone();
            let include_path = if include_path.ends_with('/') || include_path.ends_with('\\') {
                include_path
            } else {
                format!("{}/", include_path)
            };
            custom_includes.push(include_path);
            i += 2;
        } else if (args[i] == "--backend" || args[i] == "-b") && i + 1 < args.len() {
            cli_backend = Some(args[i + 1].clone());
            i += 2;
        } else if (args[i] == "--ir-out" || args[i] == "--ir-file") && i + 1 < args.len() {
            emit_ir = true;
            ir_output_file = Some(args[i + 1].clone());
            i += 2;
        } else if args[i] == "--print-ir" || args[i] == "--emit-ir" {
            emit_ir = true;
            if i + 1 < args.len()
                && !args[i + 1].starts_with('-')
                && (args[i + 1].ends_with(".ir") || args[i + 1].ends_with(".txt"))
            {
                ir_output_file = Some(args[i + 1].clone());
                i += 2;
            } else {
                i += 1;
            }
        } else if args[i] == "--emit-cpp" || args[i] == "--cpp-only" {
            emit_cpp = true;
            i += 1;
        } else if (args[i] == "--ast-out" || args[i] == "--ast-file" || args[i] == "--json-ast")
            && i + 1 < args.len()
        {
            emit_ast = true;
            ast_output_file = Some(args[i + 1].clone());
            i += 2;
        } else if args[i] == "--emit-ast" || args[i] == "--ast" {
            emit_ast = true;
            if i + 1 < args.len()
                && !args[i + 1].starts_with('-')
                && (args[i + 1].ends_with(".json")
                    || args[i + 1].ends_with(".txt")
                    || args[i + 1].ends_with(".ast"))
            {
                ast_output_file = Some(args[i + 1].clone());
                i += 2;
            } else {
                i += 1;
            }
        } else if args[i] == "--debug"
            || args[i] == "-d"
            || args[i] == "--verbose"
            || args[i] == "-v"
        {
            debug = true;
            i += 1;
        } else if args[i] == "--debug-error" || args[i] == "--debug-errors" {
            debug_errors = true;
            i += 1;
        } else if (args[i] == "-o" || args[i] == "--output") && i + 1 < args.len() {
            custom_output = Some(args[i + 1].clone());
            i += 2;
        } else if args[i] == "--clean" {
            clean_artifacts = true;
            i += 1;
        } else if args[i] == "--aot" {
            use_aot = true;
            i += 1;
        } else if !args[i].starts_with('-') && user_target_file.is_none() {
            let is_file = args[i].ends_with(".fast")
                || args[i].ends_with(".fs")
                || Path::new(&args[i]).is_file();
            if is_file || !run_after_compile {
                user_target_file = Some(args[i].clone());
            } else {
                run_args.push(args[i].clone());
            }
            i += 1;
        } else if run_after_compile && !args[i].starts_with('-') {
            run_args.push(args[i].clone());
            i += 1;
        } else {
            i += 1;
        }
    }

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let (path, manifest_info) = if let Some(ref target_f) = user_target_file {
        let pb = PathBuf::from(target_f);
        let m = FastManifest::find_in_dir_or_parents(&pb)
            .or_else(|| FastManifest::find_in_dir_or_parents(&cwd));
        (target_f.clone(), m)
    } else {
        let m = FastManifest::find_in_dir_or_parents(&cwd);
        if let Some((ref manifest, ref project_root)) = m {
            let entry = manifest.get_entry_point(project_root);
            (entry.to_string_lossy().to_string(), m)
        } else if Path::new("src/main.fast").exists() {
            ("src/main.fast".to_string(), None)
        } else if Path::new("main.fast").exists() {
            ("main.fast".to_string(), None)
        } else if run_after_compile || is_build_cmd {
            eprintln!("Error: No Fast.toml project manifest found and no source file specified.");
            exit(1);
        } else {
            ("fast.fs".to_string(), None)
        }
    };

    let (manifest, project_root) = match manifest_info {
        Some((ref m, ref root)) => (Some(m.clone()), Some(root.clone())),
        None => (None, None),
    };

    let backend = cli_backend
        .or_else(|| manifest.as_ref().and_then(|m| m.package.backend.clone()))
        .unwrap_or_else(|| "cpp".to_string());

    if custom_output.is_some() && backend == "cranelift" {
        use_aot = true;
    }

    let is_test_file = {
        let pb = Path::new(&path);
        pb.components()
            .any(|c| c.as_os_str() == "tests" || c.as_os_str() == "test")
    };

    let is_module_project = !is_test_file
        && manifest
            .as_ref()
            .map(|m| m.package.project_type == ProjectType::Module)
            .unwrap_or(false);

    if is_module_project && run_after_compile {
        eprintln!("Error: Cannot execute a module/library project with 'fast run'.");
        eprintln!("Use 'fast build' to compile the library, or 'fast test' to run tests.");
        exit(1);
    }

    CliOptions {
        path,
        manifest_info,
        manifest,
        project_root,
        target,
        custom_includes,
        backend,
        emit_ir,
        ir_output_file,
        emit_cpp,
        debug,
        use_aot,
        custom_output,
        ast_output_file,
        emit_ast,
        clean_artifacts,
        debug_errors,
        run_after_compile,
        is_build_cmd,
        run_args,
        is_test_file,
        is_module_project,
    }
}
