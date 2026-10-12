use openpilot_cluster::usb::{ParamsRuntime, Runtime};
use openpilot_params::Params;

#[test]
fn runtime_flags_follow_owned_params_changes() {
    // Given: an owned namespace with loading and active flags independently controlled.
    let temporary = tempfile::tempdir().unwrap();
    let params = Params::open(temporary.path(), "fixture").unwrap();
    let runtime = ParamsRuntime::at(temporary.path(), "fixture");
    params.put_bool("UsbGpuLoading", true).unwrap();
    params.put_bool("UsbGpuActive", false).unwrap();
    // When: the runtime reads the actual files after the loading transition.
    let observed = (runtime.gpu_active(), runtime.reset_protected());
    // Then: loading protects reset without advertising active transfers.
    assert_eq!(observed, (false, true));
}

#[test]
fn connected_update_is_committed_when_source_helper_returns() {
    // Given: a source-compatible Params owner with its real FIFO writer.
    let temporary = tempfile::tempdir().unwrap();
    let params = Params::open(temporary.path(), "fixture").unwrap();
    let mut runtime = ParamsRuntime::at(temporary.path(), "fixture");
    // When: the connected helper queues its update and releases the writer like source temporary Params.
    runtime.connected(true);
    // Then: an independent Params owner observes the atomic source byte value.
    assert_eq!(
        params.get("ClusterHudConnected").unwrap().as_deref(),
        Some(b"1".as_slice())
    );
}

#[test]
fn source_boolean_reader_treats_unreadable_file_as_empty() {
    // Given: a directory occupying the loading key, as original C++ read_file returns empty.
    let temporary = tempfile::tempdir().unwrap();
    let _params = Params::open(temporary.path(), "fixture").unwrap();
    std::fs::create_dir(temporary.path().join("fixture/UsbGpuLoading")).unwrap();
    let runtime = ParamsRuntime::at(temporary.path(), "fixture");
    // When: reset protection reads the actual directory key and absent active key.
    let protected = runtime.reset_protected();
    // Then: the original empty-read decision is preserved.
    assert!(!protected);
}
