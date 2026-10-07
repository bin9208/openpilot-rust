use super::*;

fn descriptor() -> [u8; 112] {
    let mut bytes = [0; 112];
    bytes[0..8].copy_from_slice(&96_u64.to_le_bytes());
    bytes[8..16].copy_from_slice(&104_u64.to_le_bytes());
    bytes[16..32].fill(0xff);
    bytes[32..36].copy_from_slice(&i32::MAX.to_le_bytes());
    bytes[40..48].copy_from_slice(&8_u64.to_le_bytes());
    bytes[48..56].copy_from_slice(&4_u64.to_le_bytes());
    bytes[56..64].copy_from_slice(&16_u64.to_le_bytes());
    bytes[64..72].copy_from_slice(&64_u64.to_le_bytes());
    bytes[72..88].fill(0xff);
    bytes[88..96].copy_from_slice(&0x8877665544332211_u64.to_le_bytes());
    bytes[104..108].copy_from_slice(&2_i32.to_le_bytes());
    bytes[108..112].fill(0xff);
    bytes
}

#[test]
fn buffer_decode_uses_wire_scalars_without_importing_foreign_pointers() {
    let payload = descriptor();

    let buffers = decode_buffers(&payload, 1, VisionStream::WideRoad).unwrap();

    assert_eq!(
        buffers,
        [Buffer {
            layout: VisionLayout {
                width: 8,
                height: 4,
                stride: 16,
                uv_offset: 64,
                len: 96
            },
            mapped_length: 104,
            server_id: 0x8877665544332211,
            index: 0,
            stream: VisionStream::WideRoad,
        }]
    );
}

#[test]
fn imported_buffer_accepts_the_original_unaligned_trailer_layout() {
    let mut payload = descriptor();
    payload[0..8].copy_from_slice(&98_u64.to_le_bytes());
    payload[8..16].copy_from_slice(&106_u64.to_le_bytes());

    let buffers = decode_buffers(&payload, 1, VisionStream::WideRoad).unwrap();

    assert_eq!(buffers[0].layout.len, 98);
    assert_eq!(buffers[0].mapped_length, 106);
}

#[test]
fn buffer_encode_keeps_original_field_offsets_and_clears_private_handle() {
    let buffer = Buffer {
        layout: VisionLayout {
            width: 8,
            height: 4,
            stride: 16,
            uv_offset: 64,
            len: 96,
        },
        mapped_length: 104,
        server_id: 0x8877665544332211,
        index: 0,
        stream: VisionStream::WideRoad,
    };
    let mut expected = descriptor();
    expected[16..24].copy_from_slice(&0x1000_u64.to_le_bytes());
    expected[24..32].copy_from_slice(&0x1060_u64.to_le_bytes());
    expected[32..36].copy_from_slice(&9_i32.to_le_bytes());
    expected[72..80].copy_from_slice(&0x1000_u64.to_le_bytes());
    expected[80..88].copy_from_slice(&0x1040_u64.to_le_bytes());
    expected[108..112].fill(0);

    let encoded = encode_buffer(buffer, 0x1000, 9).unwrap();

    assert_eq!(encoded, expected);
}

#[test]
fn invalid_buffer_counts_indices_and_layouts_are_rejected() {
    for (offset, value) in [
        (8, 103_u64),
        (40, 7),
        (48, u64::MAX - 1),
        (56, 7),
        (64, 63),
        (96, 1),
    ] {
        let mut payload = descriptor();
        payload[offset..offset + 8].copy_from_slice(&value.to_le_bytes());

        let result = decode_buffers(&payload, 1, VisionStream::WideRoad);

        assert!(result.is_err(), "offset={offset} value={value}");
    }
    assert!(decode_buffers(&descriptor(), 0, VisionStream::WideRoad).is_err());
    assert!(decode_buffers(&descriptor(), 2, VisionStream::WideRoad).is_err());
    assert!(decode_buffers(&descriptor(), usize::MAX, VisionStream::WideRoad).is_err());
    assert!(decode_buffers(&descriptor(), 1, VisionStream::Driver).is_err());
}

#[test]
fn packet_decode_ignores_abi_padding_but_rejects_invalid_bool() {
    let mut payload = [0xa5; 48];
    payload[0..8].copy_from_slice(&5_u64.to_le_bytes());
    payload[8..16].copy_from_slice(&3_u64.to_le_bytes());
    payload[16..20].copy_from_slice(&7_u32.to_le_bytes());
    payload[24..32].copy_from_slice(&7000_u64.to_le_bytes());
    payload[32..40].copy_from_slice(&7100_u64.to_le_bytes());
    payload[40] = 1;

    let packet = decode_packet(&payload).unwrap();

    assert_eq!(
        packet,
        Packet {
            server_id: 5,
            index: 3,
            frame_id: 7,
            timestamp_sof: 7000,
            timestamp_eof: 7100,
            valid: true
        }
    );
    payload[40] = 2;
    assert!(decode_packet(&payload).is_err());
    assert!(decode_packet(&payload[..47]).is_err());
}

#[test]
fn frame_publication_uses_buffer_identity_and_frame_fields_only() {
    let metadata = VisionMetadata {
        width: 99,
        height: 99,
        stride: 99,
        uv_offset: 99,
        len: 99,
        frame_id: 7,
        timestamp_sof: 7000,
        timestamp_eof: 7100,
        valid: true,
        received: false,
        index: 99,
        fd: -1,
    };
    let mut expected = [0; 48];
    expected[0..8].copy_from_slice(&5_u64.to_le_bytes());
    expected[8..16].copy_from_slice(&3_u64.to_le_bytes());
    expected[16..20].copy_from_slice(&7_u32.to_le_bytes());
    expected[24..32].copy_from_slice(&7000_u64.to_le_bytes());
    expected[32..40].copy_from_slice(&7100_u64.to_le_bytes());
    expected[40] = 1;

    let encoded = encode_packet(5, 3, metadata).unwrap();

    assert_eq!(encoded, expected);
}

#[test]
fn stream_discovery_preserves_sorted_set_semantics() {
    let payload = [3, 0, 0, 0, 1, 0, 0, 0, 3, 0, 0, 0];

    let streams = decode_streams(&payload).unwrap();

    assert_eq!(streams, [VisionStream::Driver, VisionStream::Map]);
    assert!(decode_streams(&[]).unwrap().is_empty());
    assert!(decode_streams(&[4, 0, 0, 0]).is_err());
    assert!(decode_streams(&[0]).is_err());
}
