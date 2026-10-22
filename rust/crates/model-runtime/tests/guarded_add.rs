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
            "%diagnostic_guarded_sum_0 = fadd float %x, %y\n  %out = select i1 %c, float %x, float %diagnostic_guarded_sum_0",
        ),
        (
            "select i1 %c, float %y, float 0.0",
            "fadd arcp nsz float %x, %sel",
            "%diagnostic_guarded_sum_0 = fadd arcp float %x, %y\n  %out = select i1 %c, float %diagnostic_guarded_sum_0, float %x",
        ),
        (
            "select i1 %c, float 0.0, float %y",
            "fadd nsz contract float %x, %sel",
            "%diagnostic_guarded_sum_0 = fadd contract float %x, %y\n  %out = select i1 %c, float %x, float %diagnostic_guarded_sum_0",
        ),
        (
            "select i1 %c, float %y, float 0.0",
            "fadd afn nsz float %sel, %x",
            "%diagnostic_guarded_sum_0 = fadd afn float %x, %y\n  %out = select i1 %c, float %diagnostic_guarded_sum_0, float %x",
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
