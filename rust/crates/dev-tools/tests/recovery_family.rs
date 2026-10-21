use openpilot_dev_tools::recovery_family::{
    admit_actions_corpus, admit_upload_corpus, encode_frame, run_cwp_pair, run_http_pair,
    run_support_pair, run_support_shutdown, run_terminal_lifecycle, run_terminal_pair,
    ActionsCorpusConfig, CwpPairConfig, HttpPairConfig, SupportPairConfig, SupportShutdownConfig,
    TerminalLifecycleConfig, TerminalPairConfig, UploadCorpusConfig,
};
use std::{env, path::PathBuf};

fn required_path(name: &str) -> PathBuf {
    PathBuf::from(
        env::var_os(name)
            .unwrap_or_else(|| panic!("{name} must be set by the recovery family gate")),
    )
}

#[test]
fn websocket_client_wire_matches_unchanged_python_controller() {
    // Literal source-controller mask 03 07 0b 13 and frame length boundaries.
    assert_eq!(
        encode_frame(1, b"hi", true, true),
        [0x81, 0x82, 3, 7, 11, 19, b'h' ^ 3, b'i' ^ 7]
    );
    assert_eq!(encode_frame(2, b"x", true, false), [0x82, 1, b'x']);
    assert_eq!(encode_frame(9, b"probe263", true, true)[..2], [0x89, 0x88]);
    assert_eq!(encode_frame(1, &[b'a'; 125], true, true)[1], 0xfd);
    assert_eq!(
        encode_frame(1, &[b'a'; 126], true, true)[1..4],
        [0xfe, 0, 126]
    );
    assert_eq!(
        encode_frame(1, &[b'a'; 65_536], true, true)[1..10],
        [0xff, 0, 0, 0, 0, 0, 1, 0, 0]
    );
    assert_eq!(encode_frame(0, b"end", false, true)[0], 0);
}

#[test]
fn http_original_and_native_pair() {
    let output = required_path("RECOVERY_FAMILY_HTTP_OUTPUT");
    let config = HttpPairConfig {
        repository: &required_path("RECOVERY_FAMILY_ROOT"),
        python: &required_path("RECOVERY_FAMILY_PYTHON"),
        source_wrapper: &required_path("RECOVERY_FAMILY_SOURCE_WRAPPER"),
        source_server: &required_path("RECOVERY_FAMILY_SOURCE_SERVER"),
        candidate: &required_path("RECOVERY_FAMILY_BINARY"),
        output: &output,
    };
    run_http_pair(&config).expect("independent recovery HTTP pair");
}

#[test]
fn cwp_original_and_native_fourteen_cases() {
    let output = required_path("RECOVERY_FAMILY_CWP_OUTPUT");
    let config = CwpPairConfig {
        repository: &required_path("RECOVERY_FAMILY_ROOT"),
        python: &required_path("RECOVERY_FAMILY_PYTHON"),
        source_script: &required_path("RECOVERY_FAMILY_CWP_SOURCE"),
        source_server: &required_path("RECOVERY_FAMILY_SOURCE_SERVER"),
        test_binary: &required_path("RECOVERY_FAMILY_CWP_TEST_BINARY"),
        output: &output,
    };
    run_cwp_pair(&config).expect("independent CWP fourteen-case pair");
}

#[test]
fn terminal_original_and_native_pty_pair() {
    let output = required_path("RECOVERY_FAMILY_TERMINAL_OUTPUT");
    let config = TerminalPairConfig {
        repository: &required_path("RECOVERY_FAMILY_ROOT"),
        python: &required_path("RECOVERY_FAMILY_PYTHON"),
        source_wrapper: &required_path("RECOVERY_FAMILY_SOURCE_WRAPPER"),
        source_server: &required_path("RECOVERY_FAMILY_SOURCE_SERVER"),
        candidate: &required_path("RECOVERY_FAMILY_BINARY"),
        output: &output,
    };
    run_terminal_pair(&config).expect("independent recovery PTY/WebSocket pair");
}

#[test]
fn support_pending_start_shutdown_is_bounded() {
    let output = required_path("RECOVERY_FAMILY_SHUTDOWN_OUTPUT");
    let config = SupportShutdownConfig {
        repository: &required_path("RECOVERY_FAMILY_ROOT"),
        candidate: &required_path("RECOVERY_FAMILY_BINARY"),
        output: &output,
    };
    run_support_shutdown(&config).expect("owned pending-support cancellation");
}

#[test]
fn terminal_native_lifecycle_and_descendant_cleanup() {
    let output = required_path("RECOVERY_FAMILY_LIFECYCLE_OUTPUT");
    let config = TerminalLifecycleConfig {
        repository: &required_path("RECOVERY_FAMILY_ROOT"),
        candidate: &required_path("RECOVERY_FAMILY_BINARY"),
        output: &output,
    };
    run_terminal_lifecycle(&config).expect("owned recovery PTY lifecycle");
}

#[test]
fn support_original_and_native_interactive_pair() {
    let output = required_path("RECOVERY_FAMILY_SUPPORT_OUTPUT");
    let config = SupportPairConfig {
        repository: &required_path("RECOVERY_FAMILY_ROOT"),
        python: &required_path("RECOVERY_FAMILY_PYTHON"),
        source_wrapper: &required_path("RECOVERY_FAMILY_SUPPORT_SOURCE"),
        source_server: &required_path("RECOVERY_FAMILY_SOURCE_SERVER"),
        candidate: &required_path("RECOVERY_FAMILY_BINARY"),
        fixture_binary: &required_path("RECOVERY_FAMILY_ACTIONS_EXAMPLE"),
        process_child: &required_path("RECOVERY_FAMILY_PROCESS_CHILD"),
        output: &output,
    };
    run_support_pair(&config).expect("independent interactive support pair");
}

#[test]
fn actions_and_tmux_unchanged_corpus_is_mandatory() {
    let output = required_path("RECOVERY_FAMILY_ACTIONS_OUTPUT");
    let config = ActionsCorpusConfig {
        repository: &required_path("RECOVERY_FAMILY_ROOT"),
        python: &required_path("RECOVERY_FAMILY_PYTHON"),
        controller: &required_path("RECOVERY_FAMILY_ACTIONS_SOURCE"),
        fixtures: &required_path("RECOVERY_FAMILY_ACTIONS_FIXTURES"),
        tools: &required_path("RECOVERY_FAMILY_ACTIONS_TOOLS"),
        source_server: &required_path("RECOVERY_FAMILY_SOURCE_SERVER"),
        repo_update: &required_path("RECOVERY_FAMILY_REPO_UPDATE"),
        candidate: &required_path("RECOVERY_FAMILY_ACTIONS_EXAMPLE"),
        process_child: &required_path("RECOVERY_FAMILY_PROCESS_CHILD"),
        tmux: &required_path("RECOVERY_FAMILY_TMUX"),
        output: &output,
    };
    admit_actions_corpus(&config).expect("unchanged actions and tmux corpus admission");
}

#[test]
fn upload_unchanged_corpus_is_mandatory() {
    let output = required_path("RECOVERY_FAMILY_UPLOAD_OUTPUT");
    let config = UploadCorpusConfig {
        repository: &required_path("RECOVERY_FAMILY_ROOT"),
        python: &required_path("RECOVERY_FAMILY_PYTHON"),
        controller: &required_path("RECOVERY_FAMILY_UPLOAD_SOURCE"),
        fixtures: &required_path("RECOVERY_FAMILY_ACTIONS_FIXTURES"),
        source_server: &required_path("RECOVERY_FAMILY_SOURCE_SERVER"),
        candidate: &required_path("RECOVERY_FAMILY_UPLOAD_EXAMPLE"),
        actions_binary: &required_path("RECOVERY_FAMILY_ACTIONS_EXAMPLE"),
        process_child: &required_path("RECOVERY_FAMILY_PROCESS_CHILD"),
        output: &output,
    };
    admit_upload_corpus(&config).expect("unchanged upload multipart/HTTP corpus admission");
}
