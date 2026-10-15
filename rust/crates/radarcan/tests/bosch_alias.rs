use openpilot_radarcan::decoder::hyundai::bosch::alias::Allocator;
use serde_json::Value;
use std::collections::BTreeSet;
#[test]
fn publication_alias_fifo_pressure_grace_and_denial_states_match_source() {
    let bytes =
        zstd::stream::decode_all(include_bytes!("fixtures/bosch-alias-final.json.zst").as_slice())
            .unwrap();
    let rows: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
    let mut allocator = Allocator::default();
    let mut count = 0;
    for (i, row) in rows.iter().enumerate() {
        if row["op"] == "new" {
            allocator = Allocator::default();
        } else {
            count += 1;
            let published: Vec<u32> = serde_json::from_value(row["published"].clone()).unwrap();
            let live: BTreeSet<u32> = serde_json::from_value(row["live"].clone()).unwrap();
            let result = allocator.update(row["time"].as_i64().unwrap(), &published, &live);
            if let Some(error) = row.get("error") {
                assert_eq!(result.err(), error.as_str(), "error/{i}");
            } else {
                assert_eq!(
                    serde_json::to_value(result.unwrap()).unwrap(),
                    row["result"],
                    "result/{i}"
                );
            }
        }
        assert_eq!(
            serde_json::to_value(&allocator).unwrap(),
            row["state"],
            "state/{i}"
        );
        assert_eq!(
            serde_json::to_value(
                allocator
                    .physical_to_alias
                    .keys()
                    .copied()
                    .collect::<Vec<_>>()
            )
            .unwrap(),
            row["order"],
            "release FIFO/{i}"
        );
    }
    assert_eq!(count, 2048);
}
