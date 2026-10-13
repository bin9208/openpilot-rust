use openpilot_cluster::h264::diagnostics::debug::{
    bytes_head, flags_text, packet_summary, should_log,
};

#[test]
fn diagnostic_names_and_limits_preserve_the_source_output() {
    assert_eq!(bytes_head(&[0, 1, 0xaa, 0xff], 3), "00 01 AA");
    assert_eq!(flags_text(0, false, false), "none");
    assert_eq!(
        flags_text(0x0202_0039, true, true),
        "KEYFRAME|PFRAME|BFRAME|CODECCONFIG|EOS|extra=0x1"
    );
    assert_eq!(
        flags_text(0x10, true, true),
        "PFRAME|codec_config_cb|keyframe_cb"
    );
    assert_eq!(packet_summary(&[], 6), "nals=none");
    assert_eq!(
        packet_summary(&[0, 0, 1, 0x65, 3, 0, 0, 1, 0x69, 4, 5], 1),
        "nals=IDR:2,+1 nal_count=2 max=AUD:3"
    );
    assert!(should_log(40, 40, 30));
    assert!(!should_log(41, 40, 30));
    assert!(should_log(60, 40, 30));
    assert!(!should_log(60, 40, 0));
}
