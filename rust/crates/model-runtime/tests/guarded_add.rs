#![cfg(feature = "native-skip-miri")]

use openpilot_model_runtime::guarded_add;

use guarded_add::{canonicalize, RewriteError, RewriteStatus, MAX_SOURCE_BYTES};
use sha2::Digest;

fn ir(select: &str, add: &str) -> String {
    format!(
        "define float @f(float %x, float %y, i1 %c) {{\nentry:\n  %sel = {select}\n  %out = {add}\n  ret float %out\n}}\n"
    )
}

#[test]
fn rewrites_both_zero_arm_and_operand_orders() {
    let cases = [
        (
            "select i1 %c, float 0.0, float %y",
            "fadd nsz float %sel, %x",
            "%diagnostic_guarded_sum_0 = fadd float %x, %y\n  %diagnostic_guarded_sum_0.zero = fadd float %x, 0.0\n  %out = select i1 %c, float %diagnostic_guarded_sum_0.zero, float %diagnostic_guarded_sum_0",
        ),
        (
            "select i1 %c, float %y, float 0.0",
            "fadd arcp nsz float %x, %sel",
            "%diagnostic_guarded_sum_0 = fadd arcp float %x, %y\n  %diagnostic_guarded_sum_0.zero = fadd float %x, 0.0\n  %out = select i1 %c, float %diagnostic_guarded_sum_0, float %diagnostic_guarded_sum_0.zero",
        ),
        (
            "select i1 %c, float 0.0, float %y",
            "fadd nsz contract float %x, %sel",
            "%diagnostic_guarded_sum_0 = fadd contract float %x, %y\n  %diagnostic_guarded_sum_0.zero = fadd float %x, 0.0\n  %out = select i1 %c, float %diagnostic_guarded_sum_0.zero, float %diagnostic_guarded_sum_0",
        ),
        (
            "select i1 %c, float %y, float 0.0",
            "fadd afn nsz float %sel, %x",
            "%diagnostic_guarded_sum_0 = fadd afn float %x, %y\n  %diagnostic_guarded_sum_0.zero = fadd float %x, 0.0\n  %out = select i1 %c, float %diagnostic_guarded_sum_0, float %diagnostic_guarded_sum_0.zero",
        ),
    ];

    for (select, add, expected) in cases {
        let result = canonicalize(&ir(select, add)).unwrap();
        assert_eq!(result.receipt.status, RewriteStatus::Rewritten);
        assert_eq!(result.receipt.rewrite_count, 1);
        assert!(result.source.contains(expected));
        assert_eq!(result.receipt.rewrites[0].removed_flag, "nsz");
    }
}

#[test]
fn leaves_unsupported_forms_byte_exact() {
    let cases = [
        ("select i1 %c, float 0.0, float %y", "fadd float %x, %sel"),
        (
            "select i1 %c, float -0.0, float %y",
            "fadd nsz float %x, %sel",
        ),
        (
            "select i1 %c, float 0.0, float 0.0",
            "fadd nsz float %x, %sel",
        ),
        (
            "select i1 %c, float 0.0, float %y",
            "fadd fast float %x, %sel",
        ),
        (
            "select i1 %c, float 0.0, float %y",
            "fadd nnan nsz float %x, %sel",
        ),
    ];

    for (select, add) in cases {
        let source = ir(select, add);
        let result = canonicalize(&source).unwrap();
        assert_eq!(result.source.as_bytes(), source.as_bytes());
        assert_eq!(result.receipt.status, RewriteStatus::Unchanged);
        assert_eq!(result.receipt.rewrite_count, 0);
    }

    let unchanged_crlf = ir(
        "select i1 %c, float -0.0, float %y",
        "fadd nsz float %x, %sel",
    )
    .replace('\n', "\r\n");
    let result = canonicalize(&unchanged_crlf).unwrap();
    assert_eq!(result.source.as_bytes(), unchanged_crlf.as_bytes());

    let crlf = ir(
        "select i1 %c, float 0.0, float %y",
        "fadd nsz float %x, %sel",
    )
    .replace('\n', "\r\n");
    let result = canonicalize(&crlf).unwrap();
    assert!(result.source.contains("\r\n"));
    assert!(
        !result.source.contains("\n")
            || result.source.matches("\r\n").count() == result.source.matches('\n').count()
    );
}

#[test]
fn generated_name_collision_fails_closed_per_function() {
    let source = "define float @f(float %x, float %y, i1 %c) {\nentry:\n  %diagnostic_guarded_sum_0 = fadd float %x, %y\n  %sel = select i1 %c, float 0.0, float %y\n  %out = fadd nsz float %x, %sel\n}\n";
    assert!(matches!(
        canonicalize(source),
        Err(RewriteError::GeneratedNameCollision { .. })
    ));

    let multi = "define float @one(float %x, float %y, i1 %c) {\nentry:\n  %sel = select i1 %c, float 0.0, float %y\n  %out = fadd nsz float %x, %sel\n  ret float %out\n}\ndefine float @two(float %x, float %y, i1 %c) {\nentry:\n  %sel = select i1 %c, float %y, float 0.0\n  %out = fadd nsz float %sel, %x\n  ret float %out\n}\n";
    let result = canonicalize(multi).unwrap();
    assert_eq!(result.receipt.rewrite_count, 2);
    assert_eq!(result.receipt.rewrites[0].function, "one");
    assert_eq!(result.receipt.rewrites[1].function, "two");
    assert!(result
        .source
        .contains("%diagnostic_guarded_sum_1 = fadd float %x, %y"));
}

#[test]
fn selector_bindings_do_not_cross_functions_and_receipt_is_sealed() {
    let source = "define float @one(float %x, float %y, i1 %c) {\nentry:\n  %sel = select i1 %c, float 0.0, float %y\n  %out = fadd nsz float %x, %sel\n  ret float %out\n}\ndefine float @two(float %x, float %y) {\nentry:\n  %out = fadd nsz float %x, %sel\n  ret float %out\n}\n";
    let result = canonicalize(source).unwrap();
    assert_eq!(result.receipt.rewrite_count, 1);
    assert_eq!(result.receipt.rewrites[0].function, "one");
    assert_eq!(result.receipt.input_bytes, source.len());
    assert_eq!(result.receipt.output_bytes, result.source.len());
    assert_eq!(
        result.receipt.output_source_sha256,
        format!("{:x}", sha2::Sha256::digest(result.source.as_bytes()))
    );
}

#[test]
fn ambiguous_select_operands_and_trailing_comments_remain_unchanged() {
    let ambiguous = "define float @f(float %x, float %y, i1 %c) {\nentry:\n  %a = select i1 %c, float 0.0, float %x\n  %b = select i1 %c, float %y, float 0.0\n  %out = fadd nsz float %a, %b\n}\n";
    let result = canonicalize(ambiguous).unwrap();
    assert_eq!(result.receipt.rewrite_count, 0);
    assert_eq!(result.source.as_bytes(), ambiguous.as_bytes());

    let comment = ir(
        "select i1 %c, float 0.0, float %y",
        "fadd nsz float %x, %sel ; retain this comment",
    );
    let result = canonicalize(&comment).unwrap();
    assert_eq!(result.receipt.rewrite_count, 0);
    assert_eq!(result.source.as_bytes(), comment.as_bytes());
}

#[test]
fn rewrite_preserves_missing_final_newline() {
    let source = "define float @f(float %x, float %y, i1 %c) {\nentry:\n  %sel = select i1 %c, float 0.0, float %y\n  %out = fadd nsz float %x, %sel\n  ret float %out\n}";
    let result = canonicalize(source).unwrap();
    assert_eq!(result.receipt.rewrite_count, 1);
    assert!(!result.source.ends_with('\n'));
    assert!(result.source.ends_with('}'));
}

#[test]
fn rejects_oversized_source_before_parsing() {
    let source = " ".repeat(MAX_SOURCE_BYTES + 1);
    assert!(matches!(
        canonicalize(&source),
        Err(RewriteError::SourceTooLarge { .. })
    ));
}

#[test]
fn rejects_oversized_identifiers_before_copying_them() {
    let long_name = "n".repeat(2048);
    let source = format!("define float @{long_name}() {{\n ret float 0.0\n}}\n");
    assert!(matches!(
        canonicalize(&source),
        Err(RewriteError::NameTooLarge { .. })
    ));

    let long_ssa = format!("%{} = add i32 1, 2\n", "v".repeat(2048));
    let source = format!("define void @f() {{\n{long_ssa} ret void\n}}\n");
    assert!(matches!(
        canonicalize(&source),
        Err(RewriteError::NameTooLarge { .. })
    ));
}

#[test]
fn quoted_names_and_basic_blocks_reserve_generated_namespace() {
    for collision in [
        "%diagnostic_guarded_sum_0.zero = fadd float %x, %y",
        "%\"diagnostic_guarded_sum_0.zero\" = fadd float %x, %y",
        "diagnostic_guarded_sum_0.zero:",
        "%\"diagnostic_guarded_sum_0\" = fadd float %x, %y",
        "%\"diagnostic_guarded_sum_\\30\" = fadd float %x, %y",
        "diagnostic_guarded_sum_0:",
        "\"diagnostic_guarded_sum_\\30\":",
    ] {
        let source = ir(
            "select i1 %c, float 0.0, float %y",
            "fadd nsz float %x, %sel",
        )
        .replace("entry:", collision);
        assert!(
            matches!(
                canonicalize(&source),
                Err(RewriteError::GeneratedNameCollision { .. })
            ),
            "{collision}"
        );
    }
}

#[test]
fn unsupported_scalar_vector_flags_and_metadata_are_unchanged() {
    for select in [
        "select i1 %c, float 0x0000000000000000, float %y",
        "select i1 %c, half 0.0, half %y",
        "select <2 x i1> %c, <2 x float> zeroinitializer, <2 x float> %y",
        "select fast i1 %c, float 0.0, float %y",
        "select i1 %c, float 0.0, float %y, !annotation !0",
    ] {
        let source = ir(select, "fadd nsz float %x, %sel");
        assert_eq!(canonicalize(&source).unwrap().source, source);
    }
    for add in [
        "fadd nsz nsz float %x, %sel",
        "fadd ninf nsz float %x, %sel",
        "fadd reassoc nsz float %x, %sel",
        "fadd nsz <2 x float> %x, %sel",
        "fadd nsz float %x, %sel, !dbg !0",
    ] {
        let source = ir("select i1 %c, float 0.0, float %y", add);
        assert_eq!(canonicalize(&source).unwrap().source, source);
    }
}

#[test]
fn bounded_metadata_fails_before_unbounded_receipt_growth() {
    let lines = "\n".repeat(262_145);
    assert!(matches!(
        canonicalize(&lines),
        Err(RewriteError::TooManySourceLines { .. })
    ));
    let functions = "define void @f() {\n ret void\n}\n".repeat(16_385);
    assert!(matches!(
        canonicalize(&functions),
        Err(RewriteError::TooManyFunctions { .. })
    ));
    let mut rewrites = String::from(
        "define void @f(float %x, float %y, i1 %c) {\n %sel = select i1 %c, float 0.0, float %y\n",
    );
    for i in 0..16_385 {
        rewrites.push_str(&format!(" %r{i} = fadd nsz float %x, %sel\n"));
    }
    rewrites.push_str(" ret void\n}\n");
    assert!(matches!(
        canonicalize(&rewrites),
        Err(RewriteError::TooManyRewrites { .. })
    ));
}

#[test]
fn receipt_growth_is_rejected_from_bounded_source() {
    let name = "f".repeat(1024);
    let mut source = format!("define void @{name}(float %x, float %y, i1 %c) {{\n %sel = select i1 %c, float 0.0, float %y\n");
    for i in 0..6000 {
        source.push_str(&format!(" %r{i} = fadd nsz float %x, %sel\n"));
    }
    source.push_str(" ret void\n}\n");
    assert!(source.len() < MAX_SOURCE_BYTES);
    assert!(matches!(
        canonicalize(&source),
        Err(RewriteError::ReceiptTooLarge { .. })
    ));
}

#[test]
fn escaped_or_commented_names_do_not_create_false_collisions() {
    let source = ir(
        "select i1 %c, float 0.0, float %y",
        "fadd nsz float %x, %sel",
    )
    .replace(
        "entry:",
        "entry:\n ; %diagnostic_guarded_sum_0 = reserved only in a comment",
    );
    assert_eq!(canonicalize(&source).unwrap().receipt.rewrite_count, 1);
}

// Hosted explicit-provider control: never part of default local test selection.
#[cfg(unix)]
#[test]
#[ignore = "requires hosted explicit natural-host LLVM config, old helper, and output root"]
fn zero_arm_keeps_addition_bits_and_records_original_codegen() {
    use openpilot_model_runtime::compiler_backend_cpu::llvm::{compile_isolated, Config};
    use std::{
        fs,
        io::Write,
        path::PathBuf,
        process::{Command, Stdio},
        time::Duration,
    };
    assert_eq!(std::env::var("GITHUB_ACTIONS").unwrap(), "true");
    let out = PathBuf::from(std::env::var_os("GUARDED_ADD_PROBE_OUTPUT").unwrap());
    fs::create_dir(&out).unwrap();
    let config: Config = serde_json::from_slice(
        &fs::read(std::env::var_os("GUARDED_ADD_PROBE_CONFIG").unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        config.library,
        PathBuf::from(std::env::var_os("LLVM_PATH").unwrap())
    );
    let old_helper = PathBuf::from(std::env::var_os("GUARDED_ADD_OLD_HELPER").unwrap());
    let old_bytes = fs::read(&old_helper).unwrap();
    let old_pin = std::env::var("GUARDED_ADD_OLD_SHA256").unwrap();
    assert_eq!(format!("{:x}", sha2::Sha256::digest(&old_bytes)), old_pin);
    let mut source = String::new();
    for zero_true in [false, true] {
        for selected_left in [false, true] {
            let name = format!("probe_{}_{}", u8::from(zero_true), u8::from(selected_left));
            let arms = if zero_true {
                "float 0.0, float %y"
            } else {
                "float %y, float 0.0"
            };
            let operands = if selected_left { "%s, %x" } else { "%x, %s" };
            source.push_str(&format!("define void @{name}(i32* %out, i32 %xb, i32 %yb, i32 %flag) {{\nentry:\n %x = bitcast i32 %xb to float\n %y = bitcast i32 %yb to float\n %c = icmp ne i32 %flag, 0\n %s = select i1 %c, {arms}\n %r = fadd nsz arcp contract afn float {operands}\n %bits = bitcast float %r to i32\n store i32 %bits, i32* %out\n %half = fptrunc float %r to half\n %hb = bitcast half %half to i16\n %hi = zext i16 %hb to i32\n %p1 = getelementptr i32, i32* %out, i32 1\n store i32 %hi, i32* %p1\n %back = fpext half %half to float\n %bb = bitcast float %back to i32\n %p2 = getelementptr i32, i32* %out, i32 2\n store i32 %bb, i32* %p2\n ret void\n}}\n"));
        }
    }
    let fixed = canonicalize(&source).unwrap();
    assert_eq!(fixed.receipt.rewrite_count, 4);
    let mut child = Command::new(&old_helper)
        .arg("--receipt")
        .arg(out.join("old-rewrite.json"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let old = child.wait_with_output().unwrap();
    assert!(
        old.status.success(),
        "{}",
        String::from_utf8_lossy(&old.stderr)
    );
    let old = String::from_utf8(old.stdout).unwrap();
    let strict = source.replace("fadd nsz arcp contract afn", "fadd");
    let mut libraries = Vec::new();
    for (label, ir) in [
        ("strict", strict.as_str()),
        ("original-nsz", source.as_str()),
        ("old-helper", old.as_str()),
        ("candidate", fixed.source.as_str()),
    ] {
        fs::write(out.join(format!("{label}.ll")), ir).unwrap();
        let compiled = compile_isolated(
            std::path::Path::new(env!("CARGO_BIN_EXE_offline-llvm-compile")),
            &config,
            ir,
            Duration::from_secs(60),
        )
        .unwrap();
        let object = out.join(format!("{label}.o"));
        fs::write(&object, &compiled.object).unwrap();
        fs::write(
            out.join(format!("{label}.optimized.ll")),
            &compiled.optimized_ir,
        )
        .unwrap();
        fs::write(
            out.join(format!("{label}.receipt.json")),
            serde_json::to_vec_pretty(&compiled.receipt).unwrap(),
        )
        .unwrap();
        let library = out.join(format!("{label}.so"));
        let linked = Command::new("clang")
            .args(["-shared", "-Wl,-z,defs"])
            .arg(&object)
            .arg("-o")
            .arg(&library)
            .output()
            .unwrap();
        fs::write(out.join(format!("{label}.link.stderr")), &linked.stderr).unwrap();
        assert!(
            linked.status.success(),
            "{}",
            String::from_utf8_lossy(&linked.stderr)
        );
        // SAFETY: these four tiny functions were generated for this executing
        // host from explicit authenticated LLVM inputs, with fixed ABI/ranges.
        libraries.push(unsafe { libloading::Library::new(&library) }.unwrap());
    }
    let mut rows = Vec::new();
    let patterns = [
        0x00000000, 0x80000000, 0x3f800000, 0xbf800000, 0x00000001, 0x80000001, 0x007fffff,
        0x807fffff, 0x7f800000, 0xff800000, 0x7fc00001, 0xffc00001, 0x7f800001, 0xff800001,
        0x7fffffff, 0xffffffff,
    ];
    let mut changed_negative_zero = 0;
    for zero_true in [false, true] {
        for selected_left in [false, true] {
            let name = format!("probe_{}_{}", u8::from(zero_true), u8::from(selected_left));
            for flag in [0_u32, 1] {
                for (x, y) in patterns
                    .into_iter()
                    .map(|x| (x, 0x3f800000))
                    .chain(patterns.into_iter().map(|y| (0x3f800000, y)))
                {
                    let mut actual = Vec::new();
                    for library in &libraries {
                        type Probe = unsafe extern "C" fn(*mut u32, u32, u32, u32);
                        // SAFETY: exact above ABI; output holds three u32 words.
                        let function = unsafe { library.get::<Probe>(name.as_bytes()) }.unwrap();
                        let mut words = [0_u32; 3];
                        unsafe {
                            function(words.as_mut_ptr(), x, y, flag);
                        }
                        actual.push(words);
                    }
                    rows.push(serde_json::json!({"function":name,"x_bits":x,"y_bits":y,"flag":flag,"strict":actual[0],"original_nsz":actual[1],"old_helper":actual[2],"candidate":actual[3]}));
                    // Save all rows even on a failed comparison, never obscure it
                    // with an early test assertion or expected-data replacement.
                    fs::write(
                        out.join("rows.json"),
                        serde_json::to_vec_pretty(&rows).unwrap(),
                    )
                    .unwrap();
                    assert_eq!(
                        actual[3], actual[0],
                        "candidate vs strict addition: {name}, x={x:08x}, y={y:08x}, flag={flag}"
                    );
                    if (flag != 0) == zero_true && x == 0x80000000 {
                        assert_eq!(actual[0], [0, 0, 0]);
                        assert_eq!(actual[2], [0x80000000, 0x8000, 0x80000000]);
                        changed_negative_zero += 1;
                    }
                }
            }
        }
    }
    assert_eq!(changed_negative_zero, 4);
    assert_eq!(rows.len(), 256);
    assert_eq!(fs::read(old_helper).unwrap(), old_bytes);
    fs::write(out.join("complete.json"),serde_json::to_vec_pretty(&serde_json::json!({"rows":rows.len(),"old_helper_negative_zero_defects":changed_negative_zero,"actual_model_operand_capture":false,"historical_or_current_full_model_acceptance":false})).unwrap()).unwrap();
}
