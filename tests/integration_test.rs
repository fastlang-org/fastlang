use std::fs;
use std::path::Path;
use std::process::Command;

#[test]
fn run_all_fs_tests() {
    let backend = std::env::var("FASTLANG_BACKEND").unwrap_or_else(|_| "cpp".to_string());
    run_directory_tests(Path::new("tests"), &backend);
}

#[test]
fn run_all_examples() {
    run_directory_tests(Path::new("examples"), "cpp");
}
#[test]
fn run_cranelift_aot_tests() {
    run_directory_tests(Path::new("tests"), "cranelift");
}

#[test]
fn test_project_system_cli() {
    let fast_bin = env!("CARGO_BIN_EXE_fast_lang");
    let test_dir = std::path::PathBuf::from("target/test_projects");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).unwrap();

    let app_name = "sample_app";
    let app_dir = test_dir.join(app_name);

    // 1. Test 'fast new'
    let status_new = Command::new(fast_bin)
        .current_dir(&test_dir)
        .arg("new")
        .arg(app_name)
        .arg("-y")
        .status()
        .expect("failed to run fast new");
    assert!(status_new.success(), "fast new failed");
    assert!(app_dir.join("Fast.toml").exists(), "Fast.toml should exist");
    assert!(
        app_dir.join("src/main.fast").exists(),
        "src/main.fast should exist"
    );

    assert!(
        !app_dir.join("tests").exists(),
        "bin project should not have tests/ folder"
    );

    // 2. Test 'fast run'
    let output_run = Command::new(fast_bin)
        .current_dir(&app_dir)
        .arg("run")
        .output()
        .expect("failed to run fast run");
    assert!(
        output_run.status.success(),
        "fast run failed: {}",
        String::from_utf8_lossy(&output_run.stderr)
    );
    let stdout = String::from_utf8_lossy(&output_run.stdout);
    assert!(
        stdout.contains("Hello from sample_app!"),
        "Unexpected output: {}",
        stdout
    );

    // Verify on-demand std: only types, error, and imported fmt are generated
    assert!(app_dir.join("build/lib/std_types.hpp").exists(), "std_types.hpp should exist");
    assert!(app_dir.join("build/lib/std_error.hpp").exists(), "std_error.hpp should exist");
    assert!(app_dir.join("build/lib/std_fmt.hpp").exists(), "std_fmt.hpp should exist");
    assert!(!app_dir.join("build/lib/std_io.hpp").exists(), "std_io.hpp must NOT be generated if unimported");
    assert!(!app_dir.join("build/lib/std_mem.hpp").exists(), "std_mem.hpp must NOT be generated if unimported");
    assert!(!app_dir.join("build/lib/std_string.hpp").exists(), "std_string.hpp must NOT be generated if unimported");
    assert!(!app_dir.join("build/lib/std_range.hpp").exists(), "std_range.hpp must NOT be generated if unimported");

    // 3. Test 'fast run <script>'
    let toml_path = app_dir.join("Fast.toml");
    let toml_content = fs::read_to_string(&toml_path).unwrap();
    let updated_toml = toml_content.replace("[scripts]", "[scripts]\ncheck_version = \"echo FAST_SCRIPT_WORKS\"");
    fs::write(&toml_path, updated_toml).unwrap();

    let output_script = Command::new(fast_bin)
        .current_dir(&app_dir)
        .arg("run")
        .arg("check_version")
        .output()
        .expect("failed to run script");
    assert!(output_script.status.success(), "custom script failed");
    let script_out = String::from_utf8_lossy(&output_script.stdout);
    assert!(
        script_out.contains("FAST_SCRIPT_WORKS"),
        "Script output should contain FAST_SCRIPT_WORKS: {}",
        script_out
    );

    // 4. Test 'fast test' on bin project (should gracefully report no tests directory)
    let output_test = Command::new(fast_bin)
        .current_dir(&app_dir)
        .arg("test")
        .output()
        .expect("failed to run fast test");
    assert!(output_test.status.success(), "fast test failed");
    let test_out = String::from_utf8_lossy(&output_test.stdout);
    assert!(
        test_out.contains("No tests directory found"),
        "Expected no tests notice: {}",
        test_out
    );

    // 5. Test 'fast clean'
    let status_clean = Command::new(fast_bin)
        .current_dir(&app_dir)
        .arg("clean")
        .status()
        .expect("failed to run fast clean");
    assert!(status_clean.success(), "fast clean failed");
    assert!(
        !app_dir.join("build").exists(),
        "build/ should be removed after clean"
    );

    // 5. Test 'fast new' library/module
    let lib_name = "sample_lib";
    let lib_dir = test_dir.join(lib_name);
    let status_lib_new = Command::new(fast_bin)
        .current_dir(&test_dir)
        .arg("new")
        .arg(lib_name)
        .arg("--type")
        .arg("lib")
        .arg("-y")
        .status()
        .expect("failed to run fast new lib");
    assert!(status_lib_new.success(), "fast new lib failed");
    assert!(
        lib_dir.join("src/lib.fast").exists(),
        "src/lib.fast should exist"
    );
    assert!(
        lib_dir.join("tests/test_basic.fast").exists(),
        "library should scaffold test harness"
    );

    // 6. Test 'fast build' on library
    let status_lib_build = Command::new(fast_bin)
        .current_dir(&lib_dir)
        .arg("build")
        .status()
        .expect("failed to run fast build on lib");
    assert!(status_lib_build.success(), "fast build on lib failed");
    assert!(
        lib_dir.join("build/sample_lib.hpp").exists(),
        "sample_lib.hpp should exist"
    );
    assert!(
        lib_dir.join("build/sample_lib.o").exists(),
        "sample_lib.o should exist"
    );

    // 7. Test 'fast test' on library
    let output_lib_test = Command::new(fast_bin)
        .current_dir(&lib_dir)
        .arg("test")
        .output()
        .expect("failed to run fast test on lib");
    assert!(output_lib_test.status.success(), "fast test on lib failed");
    let lib_test_out = String::from_utf8_lossy(&output_lib_test.stdout);
    assert!(
        lib_test_out.contains("PASSED"),
        "Tests did not pass: {}",
        lib_test_out
    );
    assert!(
        !lib_dir.join("build/test_bin").exists(),
        "test_bin directory should NOT exist"
    );

    // Cleanup
    let _ = fs::remove_dir_all(&test_dir);
}

fn run_directory_tests(test_dir: &Path, backend: &str) {
    if !test_dir.exists() {
        println!("Directory {:?} not found.", test_dir);
        return;
    }

    let mut entries = fs::read_dir(test_dir)
        .expect("failed to read tests directory")
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, std::io::Error>>()
        .unwrap();

    entries.sort();

    use std::sync::{Arc, Mutex};
    use std::thread;

    let passed_tests = Arc::new(Mutex::new(0));
    let failed_tests = Arc::new(Mutex::new(0));
    let logs = Arc::new(Mutex::new(String::new()));

    // Limit concurrency to chunks of 8.
    for chunk in entries.chunks(8) {
        let mut chunk_handles = vec![];
        for path in chunk {
            let path = path.clone();
            let ext = path.extension().and_then(|s| s.to_str());
            if !path.is_file() || (ext != Some("fast") && ext != Some("fs")) {
                continue;
            }

            let backend = backend.to_string();
            let passed = Arc::clone(&passed_tests);
            let failed = Arc::clone(&failed_tests);
            let logs_arc = Arc::clone(&logs);

            chunk_handles.push(thread::spawn(move || {
                let filename = path.file_name().unwrap().to_str().unwrap().to_string();
                if let Ok(filter) = std::env::var("FASTLANG_FILTER") {
                    if !filter.is_empty() && !filename.contains(&filter) {
                        return;
                    }
                }
                println!("--> Starting test: {}", filename);
                let should_fail = filename.starts_with("fail_")
                    || filename.contains("_fail_")
                    || filename.contains("fail");

                let parent = path.parent().unwrap_or(Path::new(""));
                let build_dir = parent.join("build");
                let base_name = filename
                    .strip_suffix(".fast")
                    .or_else(|| filename.strip_suffix(".fs"))
                    .unwrap_or(&filename);
                let build_exe = build_dir.join(if cfg!(windows) {
                    format!("{}.exe", base_name)
                } else {
                    base_name.to_string()
                });
                let build_cpp = build_dir.join(format!("{}.cpp", base_name));
                let build_o = build_dir.join(format!("{}.o", base_name));
                let build_ir = build_dir.join(format!("{}.ir", base_name));
                let _ = fs::remove_file(&build_exe);
                let _ = fs::remove_file(&build_cpp);
                let _ = fs::remove_file(&build_o);
                let _ = fs::remove_file(&build_ir);

                let mut cmd = Command::new(env!("CARGO_BIN_EXE_fast_lang"));
                cmd.arg(path.to_str().unwrap());
                cmd.arg("-o").arg(build_exe.to_str().unwrap());
                if backend == "cranelift" || backend == "native" {
                    cmd.arg("--backend").arg("cranelift").arg("--aot");
                }

                let output = cmd.output().expect("failed to execute process");

                let mut local_log = String::new();
                let mut is_success = true;

                if should_fail {
                    if output.status.success() {
                        local_log.push_str(&format!(
                            "
 FAILED: Test {} was expected to fail but succeeded!
Stdout: {}
",
                            filename,
                            String::from_utf8_lossy(&output.stdout)
                        ));
                        is_success = false;
                    } else {
                        local_log.push_str(&format!(
                            " PASSED (Expected Failure): {}
",
                            filename
                        ));
                    }
                } else {
                    if !output.status.success() {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        local_log.push_str(&format!(
                            "
 FAILED (Unexpected Failure): {}
Stdout:
{}
Stderr:
{}
",
                            filename,
                            stdout.trim(),
                            stderr.trim()
                        ));
                        is_success = false;
                    } else {
                        let content = fs::read_to_string(&path).unwrap();
                        let mut expected_lines = Vec::new();
                        let mut input_lines = Vec::new();
                        let mut manual_run = false;
                        let mut in_block_comment = false;
                        for line in content.lines() {
                            let trimmed = line.trim();
                            if trimmed.contains("/*") {
                                in_block_comment = true;
                            }
                            if trimmed.contains("*/") {
                                in_block_comment = false;
                                continue;
                            }
                            if in_block_comment {
                                continue;
                            }
                            if line.contains("// MANUAL_RUN") {
                                manual_run = true;
                            }
                            if let Some(idx) = line.find("// EXPECT:") {
                                expected_lines.push(line[idx + 10..].trim().to_string());
                            }
                            if let Some(idx) = line.find("// INPUT:") {
                                input_lines.push(line[idx + 9..].trim().to_string());
                            }
                        }

                        if !build_exe.exists() {
                            local_log.push_str(&format!(
                                "\n FAILED (Executable Not Found): {} (expected {:?})\n",
                                filename, build_exe
                            ));
                            is_success = false;
                        } else if manual_run {
                            local_log.push_str(&format!(
                                " PASSED (Compilation Verified / Manual Run Server): {}\n",
                                filename
                            ));
                            is_success = true;
                        } else {
                            let app_output = if !input_lines.is_empty() {
                                use std::io::Write;
                                let mut child = Command::new(&build_exe)
                                    .stdin(std::process::Stdio::piped())
                                    .stdout(std::process::Stdio::piped())
                                    .stderr(std::process::Stdio::piped())
                                    .spawn()
                                    .expect("failed to spawn compiled app");

                                if let Some(mut stdin) = child.stdin.take() {
                                    let input_data = input_lines.join("\n") + "\n";
                                    let _ = stdin.write_all(input_data.as_bytes());
                                }

                                child
                                    .wait_with_output()
                                    .expect("failed to wait on compiled app")
                            } else {
                                Command::new(&build_exe)
                                    .output()
                                    .expect("failed to execute compiled app")
                            };
                            if !app_output.status.success() {
                                let app_stderr = String::from_utf8_lossy(&app_output.stderr);
                                let app_stdout = String::from_utf8_lossy(&app_output.stdout);
                                local_log.push_str(&format!(
                                    "
 FAILED (Execution Error): {} exited with code {:?}
Stdout:
{}
Stderr:
{}
",
                                    filename,
                                    app_output.status.code(),
                                    app_stdout.trim(),
                                    app_stderr.trim()
                                ));
                                is_success = false;
                            } else if !expected_lines.is_empty() {
                                let app_stdout = String::from_utf8_lossy(&app_output.stdout);
                                let actual_lines: Vec<&str> = app_stdout
                                    .lines()
                                    .map(|s| s.trim())
                                    .filter(|s| !s.is_empty())
                                    .collect();
                                let mut expected_idx = 0;
                                for actual in actual_lines {
                                    if expected_idx < expected_lines.len()
                                        && actual == expected_lines[expected_idx]
                                    {
                                        expected_idx += 1;
                                    }
                                }
                                if expected_idx < expected_lines.len() {
                                    local_log.push_str(&format!(
                                        "
 FAILED (Output Mismatch): {}
Expected to find: '{}'
Actual Output:
{}
",
                                        filename, expected_lines[expected_idx], app_stdout
                                    ));
                                    is_success = false;
                                } else {
                                    local_log.push_str(&format!(
                                        " PASSED: {}
",
                                        filename
                                    ));
                                }
                            } else {
                                local_log.push_str(&format!(
                                    " PASSED: {}
",
                                    filename
                                ));
                            }
                        }
                    }
                }

                let _ = fs::remove_file(&build_exe); // Cleanup
                let _ = fs::remove_file(&build_cpp); // Cleanup
                let _ = fs::remove_file(&build_o); // Cleanup
                let _ = fs::remove_file(&build_ir); // Cleanup

                let mut logs_lock = logs_arc.lock().unwrap();
                logs_lock.push_str(&local_log);
                print!("{}", local_log);

                if is_success {
                    *passed.lock().unwrap() += 1;
                } else {
                    *failed.lock().unwrap() += 1;
                }
            }));
        }
        for h in chunk_handles {
            h.join().unwrap();
        }
    }

    let passed = *passed_tests.lock().unwrap();
    let failed = *failed_tests.lock().unwrap();

    let debug_mode = std::env::var("FASTLANG_TEST_DEBUG")
        .map(|v| v == "1")
        .unwrap_or(false);
    if !debug_mode && failed == 0 {
        let build_dir = test_dir.join("build");
        if build_dir.exists() {
            let _ = fs::remove_dir_all(&build_dir);
        }
    }

    println!("==================================================");
    println!("Test Run Complete: {} Passed, {} Failed", passed, failed);

    if failed > 0 {
        panic!("{} tests failed! Check the logs above.", failed);
    }
}
