use openpilot_ublox::framing::{checksum, Framer};
fn frame(id: u8, payload: &[u8]) -> Vec<u8> {
    let length = u16::try_from(payload.len()).unwrap().to_le_bytes();
    let mut result = vec![0xb5, 0x62, 1, id, length[0], length[1]];
    result.extend_from_slice(payload);
    let check = checksum(&result[2..]);
    result.extend_from_slice(&check);
    result
}
#[test]
fn noise_followed_by_split_preamble_retains_complete_packet() {
    let packet = frame(7, &[]);
    let mut framer = Framer::default();
    assert!(framer.add_data(1., &[0, 0xb5]).is_empty());
    assert_eq!(
        framer.buffer,
        [0xb5],
        "trailing first sync byte must survive noise"
    );
    assert_eq!(framer.add_data(2., &packet[1..]), [packet]);
    assert!(framer.buffer.is_empty());
    assert_eq!(framer.last_log_time, 2.);
}
#[test]
fn every_chunk_boundary_and_noise_prefix_preserve_ordered_packets() {
    let packets = [frame(7, &[1, 2, 3]), frame(8, &[0xb5, 0x62, 0x7e])];
    for noise in [vec![], vec![0], vec![0x62, 0, 0xb5, 0xb5]] {
        let bytes = noise
            .into_iter()
            .chain(packets.iter().flatten().copied())
            .collect::<Vec<_>>();
        for split in 0..=bytes.len() {
            let mut framer = Framer::default();
            let mut observed = framer.add_data(1., &bytes[..split]);
            observed.extend(framer.add_data(2., &bytes[split..]));
            assert_eq!(observed, packets, "split={split}, input={bytes:?}");
            assert!(framer.buffer.is_empty());
        }
        let mut framer = Framer::default();
        let observed = bytes
            .iter()
            .flat_map(|byte| framer.add_data(3., &[*byte]))
            .collect::<Vec<_>>();
        assert_eq!(observed, packets);
    }
}
#[test]
fn corrupt_checksum_resynchronizes_to_the_next_valid_packet() {
    let good = frame(9, &[3, 4, 5]);
    let mut bad = frame(8, &[1, 2]);
    *bad.last_mut().unwrap() ^= 1;
    let bytes = bad
        .into_iter()
        .chain([0, 0xb5])
        .chain(good[1..].iter().copied())
        .collect::<Vec<_>>();
    for split in 0..=bytes.len() {
        let mut framer = Framer::default();
        let mut observed = framer.add_data(1., &bytes[..split]);
        observed.extend(framer.add_data(2., &bytes[split..]));
        assert_eq!(observed, std::slice::from_ref(&good), "split={split}");
        assert!(framer.buffer.is_empty());
    }
}
#[test]
fn garbage_keeps_only_a_final_first_sync_byte_and_reset_discards_it() {
    let packet = frame(7, &[]);
    let mut framer = Framer::default();
    assert!(framer.add_data(1., &[1, 2, 3, 0xb5]).is_empty());
    assert_eq!(framer.buffer, [0xb5]);
    assert!(framer.add_data(2., &[]).is_empty());
    assert_eq!(framer.buffer, [0xb5]);
    assert_eq!(framer.last_log_time, 2.);
    framer.reset();
    assert!(framer.buffer.is_empty());
    assert!(framer.add_data(3., &packet[1..]).is_empty());
    assert_eq!(framer.add_data(4., &packet), [packet]);
    assert!(framer.buffer.is_empty());
    assert!(framer.add_data(5., &[1, 2, 3, 0x62]).is_empty());
    assert!(framer.buffer.is_empty());
}
#[test]
fn retained_sync_byte_can_be_repeated_or_completed_without_accepting_bad_checksum() {
    let packet = frame(7, &[]);
    let mut framer = Framer::default();
    for _ in 0..100 {
        assert!(framer.add_data(1., &[0xb5]).is_empty());
        assert_eq!(framer.buffer, [0xb5]);
    }
    assert_eq!(
        framer.add_data(2., &packet[1..]),
        std::slice::from_ref(&packet)
    );
    let mut corrupt = packet.clone();
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(framer.add_data(3., &corrupt).is_empty());
    assert_eq!(framer.add_data(4., &packet), [packet]);
}
