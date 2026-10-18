fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [root, digest] = args.as_slice() else {
        return Err("usage: signal_qcom_validate PACKAGE EXPECTED_MANIFEST_SHA256".into());
    };
    let kernels =
        openpilot_signal_observer::qcom_provider::validate(std::path::Path::new(root), digest)?;
    println!(
        "admitted detector={} classifier={} A630 kernels; no GPU opened",
        kernels[0], kernels[1]
    );
    Ok(())
}
