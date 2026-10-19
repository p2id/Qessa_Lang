use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("qessa_tests_{}_{}", std::process::id(), id));
        fs::create_dir(&path).expect("create test directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn compiler() -> Command {
    Command::new(env!("CARGO_BIN_EXE_qessac"))
}

fn example(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn run(args: &[&str]) -> Output {
    compiler().args(args).output().expect("run qessac")
}

#[test]
fn run_forwards_arguments_and_embeds_the_standard_library() {
    let dir = TestDir::new();
    let input = dir.path().join("input.txt");
    fs::write(&input, "alpha beta\ngamma\n").expect("write fixture");
    let input = input.to_string_lossy().into_owned();

    let cat = run(&["run", &example("cat.qs"), &input]);
    assert!(
        cat.status.success(),
        "{}",
        String::from_utf8_lossy(&cat.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&cat.stdout), "alpha beta\ngamma\n");

    let wc = run(&["run", &example("wc.qs"), &input]);
    assert!(
        wc.status.success(),
        "{}",
        String::from_utf8_lossy(&wc.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&wc.stdout),
        "lines: 2\nwords: 3\nchars: 17\n"
    );
}

#[test]
fn read_file_handles_inputs_larger_than_one_megabyte() {
    let dir = TestDir::new();
    let input = dir.path().join("large.txt");
    let payload = vec![b'Q'; 1_250_000];
    fs::write(&input, &payload).expect("write large fixture");
    let input = input.to_string_lossy().into_owned();

    let cat = run(&["run", &example("cat.qs"), &input]);
    assert!(
        cat.status.success(),
        "{}",
        String::from_utf8_lossy(&cat.stderr)
    );
    assert_eq!(cat.stdout, payload);
}

#[test]
fn run_propagates_the_program_exit_code() {
    let dir = TestDir::new();
    let source = dir.path().join("exit.qs");
    fs::write(&source, "fn main() { return 7 }\n").expect("write source");
    let output = run(&["run", source.to_str().expect("UTF-8 path")]);
    assert_eq!(output.status.code(), Some(7));
    assert!(output.stderr.is_empty());
}

#[test]
fn function_arguments_beyond_six_follow_the_x86_64_stack_abi_order() {
    let dir = TestDir::new();
    let source = dir.path().join("stack_args.qs");
    fs::write(
        &source,
        r#"fn seventh(a, b, c, d, e, f, g) { return g }
fn eighth(a, b, c, d, e, f, g, h) { return g * 100 + h }
fn main() {
    print_int(seventh(1, 2, 3, 4, 5, 6, 7)); print_str("\n")
    print_int(eighth(1, 2, 3, 4, 5, 6, 7, 8)); print_str("\n")
    return 0
}
"#,
    )
    .expect("write source");

    let output = run(&["run", source.to_str().expect("UTF-8 source path")]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "7\n708\n");
}

#[test]
fn later_local_binding_shadows_an_earlier_binding() {
    let dir = TestDir::new();
    let source = dir.path().join("shadow.qs");
    fs::write(
        &source,
        "fn main() { let value = 11; let value = 29; return value }\n",
    )
    .expect("write source");

    let output = run(&["run", source.to_str().expect("UTF-8 source path")]);
    assert_eq!(output.status.code(), Some(29));
}

#[test]
fn self_hosted_compiler_bootstraps_through_stage_three() {
    let dir = TestDir::new();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = dir.path().join("qessac.qs");
    let stdlib = dir.path().join("stdlib.qs");
    let stage1 = dir.path().join("qessac-stage1");
    let stage2 = dir.path().join("qessac-stage2");
    let stage3 = dir.path().join("qessac-stage3");
    let abi_source = dir.path().join("abi.qs");
    let stage1_program = dir.path().join("stage1-program");
    let stage2_program = dir.path().join("stage2-program");
    let stage3_program = dir.path().join("stage3-program");
    let empty_dir = dir.path().join("empty-input");
    fs::create_dir(&empty_dir).expect("create empty-input directory");
    let empty_source = empty_dir.join("empty.qs");
    let stage2_empty_output = empty_dir.join("stage2-empty");
    let stage3_empty_output = empty_dir.join("stage3-empty");

    fs::copy(root.join("src/qessac/qessac.qs"), &source).expect("copy self-hosted compiler");
    fs::copy(root.join("src/qessac/stdlib.qs"), &stdlib).expect("copy standard library");
    fs::write(&empty_source, "").expect("write empty Qessa source");

    let compile_stage1 = compiler()
        .args([
            "compile",
            source.to_str().expect("UTF-8 source path"),
            "-o",
            stage1.to_str().expect("UTF-8 output path"),
        ])
        .output()
        .expect("compile stage-1");
    assert!(
        compile_stage1.status.success(),
        "stage-1 compile failed:\n{}\n{}",
        String::from_utf8_lossy(&compile_stage1.stdout),
        String::from_utf8_lossy(&compile_stage1.stderr)
    );

    let compile_stage2 = Command::new(&stage1)
        .args([
            source.to_str().expect("UTF-8 source path"),
            stage2.to_str().expect("UTF-8 output path"),
        ])
        .current_dir(dir.path())
        .output()
        .expect("run stage-1 to produce stage-2");
    assert!(
        compile_stage2.status.success(),
        "stage-1 failed to compile itself:\n{}\n{}",
        String::from_utf8_lossy(&compile_stage2.stdout),
        String::from_utf8_lossy(&compile_stage2.stderr)
    );

    let run_stage2 = Command::new(&stage2)
        .args([
            empty_source.to_str().expect("UTF-8 empty source path"),
            stage2_empty_output.to_str().expect("UTF-8 output path"),
        ])
        .current_dir(&empty_dir)
        .output()
        .expect("run stage-2 on empty input");
    assert!(
        run_stage2.status.success(),
        "stage-2 failed on empty input:\n{}\n{}",
        String::from_utf8_lossy(&run_stage2.stdout),
        String::from_utf8_lossy(&run_stage2.stderr)
    );

    let compile_stage3 = Command::new(&stage2)
        .args([
            source.to_str().expect("UTF-8 source path"),
            stage3.to_str().expect("UTF-8 output path"),
        ])
        .current_dir(dir.path())
        .output()
        .expect("run stage-2 to produce stage-3");
    assert!(
        compile_stage3.status.success(),
        "stage-2 failed to compile itself:\n{}\n{}",
        String::from_utf8_lossy(&compile_stage3.stdout),
        String::from_utf8_lossy(&compile_stage3.stderr)
    );

    let run_stage3 = Command::new(&stage3)
        .args([
            empty_source.to_str().expect("UTF-8 empty source path"),
            stage3_empty_output.to_str().expect("UTF-8 output path"),
        ])
        .current_dir(&empty_dir)
        .output()
        .expect("run stage-3 on empty input");
    assert!(
        run_stage3.status.success(),
        "stage-3 failed on empty input:\n{}\n{}",
        String::from_utf8_lossy(&run_stage3.stdout),
        String::from_utf8_lossy(&run_stage3.stderr)
    );

    fs::write(
        &abi_source,
        "fn seventh(a, b, c, d, e, f, g) { return g }\n\
         fn eighth(a, b, c, d, e, f, g, h) { return g * 100 + h }\n\
         fn main() { print_int(seventh(1, 2, 3, 4, 5, 6, 7)); print_str(\"\\n\"); \
         print_int(eighth(1, 2, 3, 4, 5, 6, 7, 8)); print_str(\"\\n\"); return 0 }\n",
    )
    .expect("write self-hosted ABI fixture");
    for (stage, executable, label) in [
        (&stage1, &stage1_program, "stage-1"),
        (&stage2, &stage2_program, "stage-2"),
        (&stage3, &stage3_program, "stage-3"),
    ] {
        let compile_program = Command::new(stage)
            .args([
                abi_source.to_str().expect("UTF-8 ABI source path"),
                executable.to_str().expect("UTF-8 executable path"),
            ])
            .current_dir(dir.path())
            .output()
            .expect("compile ABI fixture with self-hosted compiler");
        assert!(
            compile_program.status.success(),
            "{label} failed to compile ABI fixture:\n{}\n{}",
            String::from_utf8_lossy(&compile_program.stdout),
            String::from_utf8_lossy(&compile_program.stderr)
        );
        let execution = Command::new(executable)
            .output()
            .expect("run self-hosted ABI fixture");
        assert!(
            execution.status.success(),
            "{label} ABI fixture exited {:?}:\n{}\n{}",
            execution.status.code(),
            String::from_utf8_lossy(&execution.stdout),
            String::from_utf8_lossy(&execution.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&execution.stdout),
            "7\n708\n",
            "{label} generated incorrect 7/8-argument results"
        );
    }

    let argv_source = dir.path().join("argv.qs");
    fs::write(
        &argv_source,
        "fn main(argc, argv) {\n\
             print_int(argc); print_str(\"\\n\")\n\
             if argc > 1 { print_str(__peek64(argv + 8)); print_str(\"\\n\") }\n\
             if argc > 2 { print_str(__peek64(argv + 16)); print_str(\"\\n\") }\n\
             return 0\n\
         }\n",
    )
    .expect("write self-hosted argv fixture");
    for (stage, label) in [
        (&stage1, "stage-1"),
        (&stage2, "stage-2"),
        (&stage3, "stage-3"),
    ] {
        let executable = dir.path().join(format!("{label}-argv"));
        let compile = Command::new(stage)
            .args([
                argv_source.to_string_lossy().into_owned(),
                executable.to_string_lossy().into_owned(),
            ])
            .current_dir(dir.path())
            .output()
            .expect("compile argv fixture with self-hosted compiler");
        assert!(
            compile.status.success(),
            "{label} failed to compile argv fixture:\n{}\n{}",
            String::from_utf8_lossy(&compile.stdout),
            String::from_utf8_lossy(&compile.stderr)
        );

        let no_args = Command::new(&executable)
            .output()
            .expect("run argv fixture without program arguments");
        assert!(no_args.status.success());
        assert_eq!(String::from_utf8_lossy(&no_args.stdout), "1\n");

        let with_args = Command::new(&executable)
            .args(["alpha", "beta"])
            .output()
            .expect("run argv fixture with two program arguments");
        assert!(with_args.status.success());
        assert_eq!(
            String::from_utf8_lossy(&with_args.stdout),
            "3\nalpha\nbeta\n",
            "{label} passed incorrect argc/argv values"
        );
    }

    // Keep the supported self-hosted subset honest: every example below must
    // compile and produce exactly the same observable result as the Rust CLI.
    for name in [
        "arrays.qs",
        "factorial.qs",
        "full_test.qs",
        "hello.qs",
        "pointers.qs",
        "recursion.qs",
        "simple_loop.qs",
        "test_final.qs",
    ] {
        let native = run(&["run", &example(name)]);
        assert!(
            native.status.success(),
            "Rust compiler failed on {name}:\n{}\n{}",
            String::from_utf8_lossy(&native.stdout),
            String::from_utf8_lossy(&native.stderr)
        );

        for (stage, label) in [
            (&stage1, "stage-1"),
            (&stage2, "stage-2"),
            (&stage3, "stage-3"),
        ] {
            let stem = name.trim_end_matches(".qs");
            let executable = dir.path().join(format!("{label}-{stem}"));
            let compile = Command::new(stage)
                .args([example(name), executable.to_string_lossy().into_owned()])
                .current_dir(dir.path())
                .output()
                .expect("compile example with self-hosted compiler");
            assert!(
                compile.status.success(),
                "{label} failed to compile {name}:\n{}\n{}",
                String::from_utf8_lossy(&compile.stdout),
                String::from_utf8_lossy(&compile.stderr)
            );

            let execution = Command::new(&executable)
                .output()
                .unwrap_or_else(|error| panic!("run {label} output for {name}: {error}"));
            assert!(
                execution.status.success(),
                "{label} output for {name} exited {:?}:\n{}\n{}",
                execution.status.code(),
                String::from_utf8_lossy(&execution.stdout),
                String::from_utf8_lossy(&execution.stderr)
            );
            assert_eq!(
                execution.stdout, native.stdout,
                "{label} output differs from Rust output for {name}"
            );
            assert_eq!(
                execution.stderr, native.stderr,
                "{label} stderr differs from Rust output for {name}"
            );
        }
    }

    let large_source = dir.path().join("large-source.qs");
    let mut large_contents = Vec::with_capacity(1_250_040);
    large_contents.extend_from_slice(b"//");
    large_contents.extend(std::iter::repeat(b'x').take(1_250_000));
    large_contents.extend_from_slice(b"\nfn main() { return 0 }\n");
    fs::write(&large_source, large_contents).expect("write large self-hosted source");
    for (stage, label) in [(&stage2, "stage-2"), (&stage3, "stage-3")] {
        let executable = dir.path().join(format!("{label}-large-source"));
        let compile = Command::new(stage)
            .args([
                large_source.to_string_lossy().into_owned(),
                executable.to_string_lossy().into_owned(),
            ])
            .current_dir(dir.path())
            .output()
            .expect("compile large source with self-hosted compiler");
        assert!(
            compile.status.success(),
            "{label} failed to read/compile source over 1 MiB:\n{}\n{}",
            String::from_utf8_lossy(&compile.stdout),
            String::from_utf8_lossy(&compile.stderr)
        );
        let execution = Command::new(executable)
            .status()
            .expect("run executable produced from large source");
        assert!(execution.success());
    }

    // These Rust-supported examples currently use declarations that the
    // self-hosted parser does not implement. Require a diagnostic failure and
    // make sure no partial executable is left behind.
    for name in [
        "comprehensive.qs",
        "enums.qs",
        "modules.qs",
        "regions.qs",
        "structs.qs",
    ] {
        let executable = dir.path().join(format!(
            "stage2-unsupported-{}",
            name.trim_end_matches(".qs")
        ));
        let compile = Command::new(&stage2)
            .args([example(name), executable.to_string_lossy().into_owned()])
            .current_dir(dir.path())
            .output()
            .expect("invoke self-hosted compiler on a known unsupported feature");
        assert!(
            !compile.status.success(),
            "stage-2 unexpectedly accepted unsupported example {name}"
        );
        let diagnostic = String::from_utf8_lossy(&compile.stdout);
        assert!(
            diagnostic.contains("error:"),
            "stage-2 failed without a diagnostic for {name}:\n{diagnostic}\n{}",
            String::from_utf8_lossy(&compile.stderr)
        );
        assert!(
            !executable.exists(),
            "stage-2 left an executable for unsupported example {name}"
        );
    }

    let malformed_source = dir.path().join("malformed.qs");
    let malformed_output = dir.path().join("malformed-output");
    fs::write(&malformed_source, "fn main( {\n").expect("write malformed source");
    let malformed = Command::new(&stage2)
        .args([
            malformed_source.to_string_lossy().into_owned(),
            malformed_output.to_string_lossy().into_owned(),
        ])
        .current_dir(dir.path())
        .output()
        .expect("run self-hosted compiler on malformed source");
    assert!(!malformed.status.success());
    assert!(String::from_utf8_lossy(&malformed.stdout).contains("error:"));
    assert!(!malformed_output.exists());

    assert_eq!(
        fs::read(&stage2).expect("read stage-2"),
        fs::read(&stage3).expect("read stage-3"),
        "stage-2 and stage-3 compiler outputs should be reproducible"
    );
}

#[test]
fn compile_emits_a_runnable_elf_executable() {
    let dir = TestDir::new();
    let source = dir.path().join("compiled.qs");
    let executable = dir.path().join("compiled");
    fs::write(
        &source,
        "fn main() { syscall_write(1, \"compiled OK\\n\", 12); return 0 }\n",
    )
    .expect("write source");
    let output = run(&[
        "compile",
        source.to_str().expect("UTF-8 source path"),
        "-o",
        executable.to_str().expect("UTF-8 executable path"),
        "--no-stdlib",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let program = Command::new(&executable)
        .output()
        .expect("run compiled ELF");
    assert!(program.status.success());
    assert_eq!(String::from_utf8_lossy(&program.stdout), "compiled OK\n");
}

#[test]
fn parser_recognizes_new_expressions() {
    let dir = TestDir::new();
    let source = dir.path().join("new_expr.qs");
    fs::write(&source, "fn main() { new Widget() }\n").expect("write source");
    let source = source.to_string_lossy().into_owned();
    let output = run(&["ast", &source]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("New Widget(0 args)"));
}

#[test]
fn unresolved_function_is_reported_before_an_executable_is_written() {
    let dir = TestDir::new();
    let source = dir.path().join("unknown.qs");
    let executable = dir.path().join("unknown");
    fs::write(&source, "fn main() { missing_function() }\n").expect("write source");
    let output = run(&[
        "compile",
        source.to_str().expect("UTF-8 source path"),
        "-o",
        executable.to_str().expect("UTF-8 executable path"),
        "--no-stdlib",
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown function 'missing_function'"));
    assert!(!executable.exists());
}

#[test]
fn invalid_programs_fail_with_diagnostics_before_emitting_elf() {
    let dir = TestDir::new();
    let cases = [
        (
            "unknown_identifier",
            "fn main() { return missing_value }",
            "unknown identifier 'missing_value'",
        ),
        (
            "unknown_syscall",
            "fn main() { syscall_not_real(0) }",
            "unknown syscall 'syscall_not_real'",
        ),
        (
            "missing_main",
            "fn helper() { return 0 }",
            "must define an entry function `fn main`",
        ),
        (
            "duplicate_main",
            "fn main() {} fn main() {}",
            "duplicate function definition 'main'",
        ),
    ];

    for (name, source_text, expected_error) in cases {
        let source = dir.path().join(format!("{name}.qs"));
        let executable = dir.path().join(name);
        fs::write(&source, source_text).expect("write source");
        let source = source.to_string_lossy().into_owned();
        let executable_path = executable.to_string_lossy().into_owned();
        let output = run(&["compile", &source, "-o", &executable_path, "--no-stdlib"]);

        assert!(!output.status.success(), "{name} unexpectedly compiled");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected_error),
            "{name}: expected {expected_error:?}, got {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!executable.exists(), "{name} produced an executable");
    }
}

#[test]
fn final_example_accepts_semicolons_and_checks_its_claimed_results() {
    let output = run(&["run", &example("test_final.qs")]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "arith: OK",
        "bit: OK",
        "shift: OK",
        "while: OK",
        "for+arr: OK",
        "arridx: OK",
        "break: OK",
        "compound: OK",
        "continue: OK",
        "fib: OK",
        "ptr: OK",
        "ptrwr: OK",
        "streq: OK",
        "logic: OK",
        "neg: OK",
        "=== All Passed ===",
    ] {
        assert!(
            stdout.contains(expected),
            "missing {expected:?} in:\n{stdout}"
        );
    }
    assert!(!stdout.contains("TEST FAILURE"));
}

#[test]
fn no_stdlib_mode_runs_standalone_syscall_programs() {
    let output = run(&["run", "--no-stdlib", &example("hello.qs")]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "Hello, World!\n");
}

#[test]
fn all_standalone_examples_execute_successfully() {
    for name in [
        "arrays.qs",
        "comprehensive.qs",
        "enums.qs",
        "factorial.qs",
        "full_test.qs",
        "hello.qs",
        "modules.qs",
        "pointers.qs",
        "recursion.qs",
        "regions.qs",
        "simple_loop.qs",
        "structs.qs",
    ] {
        let output = run(&["run", &example(name)]);
        assert!(
            output.status.success(),
            "{name} failed with status {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

// ---------------------------------------------------------------------------
// M1 regression tests (QES-SEC-001 / QES-SEC-002). Written without a Rust
// toolchain available: status IMPLEMENTED_UNVERIFIED until `cargo test` runs.
// ---------------------------------------------------------------------------

#[test]
fn rust_compiler_rejects_unimplemented_constructs_instead_of_compiling_them_to_zero() {
    let dir = TestDir::new();
    let cases = [
        (
            "string_pattern",
            "fn main() {\n    let s = \"a\"\n    match s {\n        \"a\" => {\n            return 1\n        }\n    }\n    return 0\n}\n",
            "unsupported string pattern",
        ),
        (
            "closure",
            "fn main() {\n    let f = |x| x + 1\n    return 0\n}\n",
            "unsupported closure",
        ),
    ];

    for (name, source_text, expected_error) in cases {
        let source = dir.path().join(format!("{name}.qs"));
        let executable = dir.path().join(name);
        fs::write(&source, source_text).expect("write source");
        let source = source.to_string_lossy().into_owned();
        let executable_path = executable.to_string_lossy().into_owned();
        let output = run(&["compile", &source, "-o", &executable_path, "--no-stdlib"]);

        assert!(!output.status.success(), "{name} unexpectedly compiled");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected_error),
            "{name}: expected {expected_error:?}, got {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!executable.exists(), "{name} produced an executable");
    }
}

#[test]
fn self_hosted_compiler_rejects_unknown_syscalls_and_functions() {
    let dir = TestDir::new();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = dir.path().join("qessac.qs");
    let stage1 = dir.path().join("qessac-stage1");
    fs::copy(root.join("src/qessac/qessac.qs"), &source).expect("copy self-hosted compiler");
    fs::copy(root.join("src/qessac/stdlib.qs"), dir.path().join("stdlib.qs"))
        .expect("copy standard library");

    let build = compiler()
        .args([
            "compile",
            source.to_str().expect("UTF-8 source path"),
            "-o",
            stage1.to_str().expect("UTF-8 output path"),
        ])
        .output()
        .expect("compile stage-1");
    assert!(
        build.status.success(),
        "stage-1 compile failed:\n{}\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let cases = [
        (
            "unknown_syscall",
            "fn main() { syscall_not_real(0) }\n",
            "unknown syscall 'syscall_not_real'",
        ),
        (
            "unknown_function",
            "fn main() { missing_function() }\n",
            "unknown function 'missing_function'",
        ),
        (
            "unknown_identifier",
            "fn main() { return missing_value }\n",
            "unknown identifier 'missing_value'",
        ),
        (
            "unknown_assignment",
            "fn main() { missing_value = 1 }\n",
            "unknown identifier 'missing_value'",
        ),
    ];

    for (name, source_text, expected_error) in cases {
        let program = dir.path().join(format!("{name}.qs"));
        let executable = dir.path().join(name);
        fs::write(&program, source_text).expect("write source");
        let output = Command::new(&stage1)
            .args([
                program.to_str().expect("UTF-8 source path"),
                executable.to_str().expect("UTF-8 output path"),
            ])
            .current_dir(dir.path())
            .output()
            .expect("run stage-1");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        assert!(!output.status.success(), "{name}: stage-1 accepted the program");
        assert!(text.contains(expected_error), "{name}: expected {expected_error:?}, got {text}");
        assert!(!executable.exists(), "{name}: stage-1 produced an executable");
    }

    // A constant whose value is 0 must still resolve (cg_lookup_const returns 0 for it).
    let zero_src = dir.path().join("zero_const.qs");
    let zero_exe = dir.path().join("zero_const");
    fs::write(&zero_src, "let ZERO = 0\nfn main() { return ZERO }\n").expect("write source");
    let compiled = Command::new(&stage1)
        .args([
            zero_src.to_str().expect("UTF-8 source path"),
            zero_exe.to_str().expect("UTF-8 output path"),
        ])
        .current_dir(dir.path())
        .output()
        .expect("run stage-1 on zero constant");
    assert!(
        compiled.status.success(),
        "zero-valued constant rejected:\n{}{}",
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&compiled.stderr)
    );
    let ran = Command::new(&zero_exe).output().expect("run zero_const");
    assert!(ran.status.success(), "zero_const exited with {:?}", ran.status.code());
}
