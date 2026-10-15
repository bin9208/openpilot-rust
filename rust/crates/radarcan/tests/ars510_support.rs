use openpilot_radarcan::decoder::toyota::ars510::support::*;
#[test]
fn pinned_support_frames_targets_properties_crc_and_short_frames_match() {
    let data = zstd::stream::decode_all(
        include_bytes!("fixtures/ars510-support-final.json.zst").as_slice(),
    )
    .unwrap();
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&data).unwrap();
    assert_eq!(rows.len(), 1024);
    for (index, row) in rows.iter().enumerate() {
        let data: Vec<u8> = serde_json::from_value(row["payload"].clone()).unwrap();
        let pair: Vec<Vec<u8>> = serde_json::from_value(row["pair"].clone()).unwrap();
        let target192 = parse_0x192(&data);
        let object680 = parse_0x680(&data);
        let target366 = parse_0x366(&data);
        let actual = serde_json::json!({
            "parse_0x192":target192,"parse_0x192_properties":target192.as_ref().map(|p|[p.d_rel(),p.y_rel()]),
            "parse_0x190":parse_0x190(&data),"parse_0x191":parse_0x191(&data),
            "parse_0x680":object680,"parse_0x680_properties":object680.as_ref().map(|p|[p.stationary(),p.oncoming(),p.seen_moving()]),
            "parse_acc_target_vrel":parse_acc_target_vrel(&data),"parse_acc_target_arel":parse_acc_target_arel(&data),
            "parse_acc_target_position":parse_acc_target_position(&data),"parse_acc_target_range_code":parse_acc_target_range_code(&data),
            "parse_0x239":parse_0x239(&data),"parse_0x23b":parse_0x23b(&data),"parse_0x366":target366,
            "parse_0x366_properties":target366.as_ref().map(|p|serde_json::json!([p.y_rel(),p.v_rel(),p.d_rel()])),
            "parse_acc_target":parse_acc_target(&pair[0],&pair[1]),
            "yaw":parse_toyota_yaw_rate(&data), "ego_speed":parse_toyota_speed_mps(&data),
            "crc8":crc8_0x1d_bits(data.iter().flat_map(|byte|(0..8).map(move |bit|(byte>>bit)&1!=0))),
            "be": ([(0,5),(3,32),(29,11),(39,13),(43,13)].map(|(start,length)|be_field(&data,start,length)))
        });
        assert_eq!(actual, row["expected"], "support/{index}");
    }
}
