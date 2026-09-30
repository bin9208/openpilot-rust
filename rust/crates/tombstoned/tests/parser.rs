use openpilot_tombstoned::{parse::description, safe_fn};

#[test]
fn excludes_maps_and_coredump_when_building_description() {
    // Given an apport file and an openpilot frame following a system frame.
    let source = "ExecutablePath: /data/openpilot/selfdrive/modeld/modeld\nSignal: 11\nProcMaps:\n secret maps\nProcStatus:\n status\nCoreDump: binary\n trailing\n";
    let trace = "trace header\n#0 0xdead libc(x) at libc.c\n#1 0xbeef target (value=0x42) at selfdrive/modeld.cc:9\n";
    // When extracting the source report description.
    let report = description(source, trace);
    // Then text includes the chosen frame and metadata, but excludes maps/core content.
    assert_eq!(report.path, "selfdrive/modeld/modeld");
    assert_eq!(
        report.message,
        "selfdrive/modeld/modeld - Signal: 11 (SIGSEGV) - target  at selfdrive/modeld.cc:9"
    );
    assert!(!report.contents.contains("secret maps"));
    assert!(!report.contents.contains("binary"));
    assert!(report.contents.ends_with("ProcStatus:\n status\n"));
}
#[test]
fn filename_filter_matches_python_alnum_when_unicode_is_present() {
    // Given combining marks, numerals and printable letters.
    let source = " a/b-한글_Ⅻ²١e\u{301}\u{0345}\u{200b} ";
    // When filtering a crash path.
    let result = safe_fn(source);
    // Then Python's letter/number categories survive, while Other_Alphabetic marks do not.
    assert_eq!(result, "ab한글_Ⅻ²١e");
}
