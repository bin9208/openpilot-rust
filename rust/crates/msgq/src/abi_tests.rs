use crate::{queue_core, vision_wire, VisionLayout, VisionMetadata, VisionStream};

pub(crate) fn layout() -> std::collections::BTreeMap<String, u64> {
    let output = crate::test_process::command(env!("NATIVE_IPC_ABI_PEER"))
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| {
            let (key, value) = line.split_once('=').unwrap();
            (key.to_owned(), value.parse().unwrap())
        })
        .collect()
}

#[test]
fn native_headers_and_serialized_vision_records_match_original_cpp() {
    let layout = layout();
    assert_eq!(layout["msgq_header_t"], queue_core::HEADER_BYTES as u64);
    for (field, expected) in [
        ("num_readers", 0),
        ("write_pointer", 8),
        ("write_uid", 16),
        ("read_pointers", 24),
        ("read_valids", 344),
        ("read_uids", 664),
    ] {
        assert_eq!(layout[&format!("msgq_header_t.{field}")], expected);
    }
    assert_eq!(layout["VisionBuf"], vision_wire::BUFFER_BYTES as u64);
    assert_eq!(layout["VisionIpcPacket"], vision_wire::PACKET_BYTES as u64);
    let output = crate::test_process::command(env!("NATIVE_IPC_ABI_PEER"))
        .arg("wire")
        .output()
        .unwrap();
    assert!(output.status.success());
    let buffer = vision_wire::Buffer {
        layout: VisionLayout {
            width: 8,
            height: 4,
            stride: 16,
            uv_offset: 64,
            len: 96,
        },
        mapped_length: 104,
        server_id: 0x8877665544332211,
        index: 3,
        stream: VisionStream::WideRoad,
    };
    let metadata = VisionMetadata {
        width: 8,
        height: 4,
        stride: 16,
        uv_offset: 64,
        len: 96,
        frame_id: 7,
        timestamp_sof: 7000,
        timestamp_eof: 7100,
        valid: true,
        received: true,
        index: 3,
        fd: 9,
    };
    let mut expected = vision_wire::encode_buffer(buffer, 0x1000, 9)
        .unwrap()
        .to_vec();
    expected.extend_from_slice(&vision_wire::encode_packet(5, 3, metadata).unwrap());
    assert_eq!(output.stdout, expected);
}
