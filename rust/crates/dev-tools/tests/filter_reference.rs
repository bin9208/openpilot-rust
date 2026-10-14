use openpilot_dev_tools::filter_reference::compare;
use sha2::{Digest, Sha256};

const HEADER: &str = "case\tstep\tx\trc\tdt\tinitialized\tfirst\tbounce\n";
const INPUT: &str = "0\t0\t1.00000000000000000\t0.20000000000000001\t0.01000000000000000\ttrue";

fn fixture() -> (Vec<u8>, String) {
    let reference = [1_f64, -2_f64]
        .into_iter()
        .flat_map(f64::to_le_bytes)
        .collect();
    let input_hash = format!("{:x}", Sha256::digest(format!("{INPUT}\n")));
    (reference, input_hash)
}

#[test]
fn retains_original_absolute_and_relative_tolerance() {
    let (reference, hash) = fixture();
    let report = compare(
        &format!("{HEADER}{INPUT}\t1.0000000000005\t-2.0000000000015\n"),
        &reference,
        &hash,
    )
    .unwrap();
    assert_eq!(report.outputs, 2);
    assert!(report.max_abs_error > 1e-12);
}

#[test]
fn changed_output_is_rejected_including_nan_and_infinity() {
    let (reference, hash) = fixture();
    for value in ["1.00000001", "NaN", "inf", "-inf", "bad"] {
        assert!(
            compare(
                &format!("{HEADER}{INPUT}\t{value}\t-2\n"),
                &reference,
                &hash
            )
            .is_err(),
            "{value}"
        );
    }
}

#[test]
fn input_change_missing_duplicate_and_extra_columns_are_rejected() {
    let (reference, hash) = fixture();
    let good = format!("{HEADER}{INPUT}\t1\t-2\n");
    for trace in [
        HEADER.to_owned(),
        good.replace("0\t0\t", "0\t1\t"),
        format!("{good}{INPUT}\t1\t-2\n"),
        good.replace("\t-2\n", "\t-2\textra\n"),
        good.replace("first\t", "wrong\t"),
    ] {
        assert!(compare(&trace, &reference, &hash).is_err());
    }
}

#[test]
fn malformed_reference_is_rejected() {
    let (mut reference, hash) = fixture();
    let trace = format!("{HEADER}{INPUT}\t1\t-2\n");
    assert!(compare(&trace, &[], &hash).is_err());
    reference.pop();
    assert!(compare(&trace, &reference, &hash).is_err());
    assert!(compare(&trace, &[0; 16], "bad digest").is_err());
}
