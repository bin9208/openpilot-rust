use openpilot_jetlink::wire::{self, Header, Version};

#[test]
fn both_protocols_preserve_source_header_bytes_and_stream_identity() {
    let header = Header {
        kind: 8,
        sequence: 42,
        flags: 1,
        length: 128,
        reserved: 7,
    };
    for version in [Version::V2, Version::V3] {
        let bytes = header.encode_version(version);
        assert_eq!(&bytes[..4], b"JLNK");
        assert_eq!(&bytes[4..6], &(version as u16).to_le_bytes());
        assert_eq!(&bytes[6..8], &8u16.to_le_bytes());
        assert_eq!(&bytes[8..12], &42u32.to_le_bytes());
        assert_eq!(Header::decode_version(&bytes, version).unwrap(), header);
        let other = if version == Version::V2 {
            Version::V3
        } else {
            Version::V2
        };
        assert!(Header::decode_version(&bytes, other).is_err());
    }
    assert!(Version::from_u16(1).is_err());
    assert!(Version::from_u16(4).is_err());
}

#[test]
fn source_packet_termination_differs_at_512_but_gadget_burst_is_constant() {
    for length in [0usize, 479, 480, 481, 991, 992, 993, 16352] {
        for version in [Version::V2, Version::V3] {
            let payload = vec![0x73; length];
            let host = wire::frame_version(8, 2, &payload, false, version).unwrap();
            let padded = (32 + length).is_multiple_of(version.packet_multiple());
            assert_eq!(host.len(), 32 + length + usize::from(padded));
            assert_eq!(wire::word(&host, 12), if padded { wire::PADDED } else { 0 });
            let gadget = wire::frame_version(8, 2, &payload, true, version).unwrap();
            assert_eq!(gadget.len() % wire::TX_ALIGN, 0);
            assert_eq!(wire::word(&gadget, 12), 0);
        }
    }
}
