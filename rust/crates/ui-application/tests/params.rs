use openpilot_ui_application::params::{
    store::{Mutation, Store},
    Read,
};
#[test]
fn private_params_fifo_flush_preserves_last_write_and_can_restart_worker(
) -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let store = Store::new(openpilot_params::Params::open(root.path(), "ui-test")?);
    for i in 0..100 {
        store.put_nonblocking(Mutation {
            key: "ShowModelView".into(),
            value: i.to_string().into_bytes(),
        })?;
    }
    store.flush()?;
    assert_eq!(store.integer("ShowModelView")?, 99);
    store.put_bool_nonblocking("ScreenRecord", true)?;
    store.flush()?;
    assert!(store.boolean("ScreenRecord")?);
    store.remove("GithubSshKeys")?;
    Ok(())
}

#[test]
fn stored_float_uses_source_float32_prefix_and_rejects_range_errors(
) -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let store = Store::new(openpilot_params::Params::open(root.path(), "ui-float")?);
    assert_eq!(store.float("CustomSR")?, 0.0);
    for (bytes, expected) in [
        (&b""[..], 0.0),
        (&b" \t+0.1garbage"[..], f64::from(0.1_f32)),
        (&b"12.5\0-99"[..], 12.5),
        (&b"0x1.8p+2tail"[..], 6.0),
        (&b"16777217"[..], 16777216.0),
        (&b"3.4028234e38"[..], f64::from(f32::MAX)),
    ] {
        store.put("CustomSR", bytes)?;
        assert_eq!(store.float("CustomSR")?, expected, "{bytes:?}");
    }
    for bytes in [&b"invalid"[..], &b"\0 7"[..], &b"1e39"[..], &b"1e-50"[..]] {
        store.put("CustomSR", bytes)?;
        assert!(
            matches!(store.float("CustomSR"), Err(openpilot_ui_application::Error::Parameter(key)) if key == "CustomSR"),
            "{bytes:?}"
        );
    }
    store.put("CustomSR", b"-0suffix")?;
    assert_eq!(store.float("CustomSR")?.to_bits(), (-0.0_f64).to_bits());
    store.put("CustomSR", b"nan(payload)")?;
    assert!(store.float("CustomSR")?.is_nan());
    store.put("CustomSR", b"-infinity trailing")?;
    assert_eq!(store.float("CustomSR")?, f64::NEG_INFINITY);
    Ok(())
}
