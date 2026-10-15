use openpilot_radarcan::decoder::hyundai::bosch::{
    group_config::{self, Config},
    physical::VisionCue,
    tracking::Track,
};
use serde_json::Value;
#[test]
fn group_configuration_geometry_and_vision_boundaries_match_source() {
    let bytes = zstd::stream::decode_all(
        include_bytes!("fixtures/bosch-group-config-final.json.zst").as_slice(),
    )
    .unwrap();
    let fixture: Value = serde_json::from_slice(&bytes).unwrap();
    for (i, row) in fixture["constructors"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let config: Config = serde_json::from_value(row["input"].clone()).unwrap();
        if let Some(error) = row.get("error") {
            assert_eq!(
                config.validate().err().as_deref(),
                error.as_str(),
                "constructor/{i}"
            );
        } else {
            config.validate().unwrap();
            assert_eq!(
                serde_json::to_value(config).unwrap(),
                row["result"],
                "config/{i}"
            );
        }
    }
    let config = Config::default();
    let rows = fixture["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 4096);
    for (i, row) in rows.iter().enumerate() {
        let a: Track = serde_json::from_value(row["a"].clone()).unwrap();
        let b: Track = serde_json::from_value(row["b"].clone()).unwrap();
        let cues: Vec<VisionCue> = serde_json::from_value(row["cues"].clone()).unwrap();
        let ego = row["ego"].as_f64().unwrap_or(f64::NAN);
        assert_eq!(
            serde_json::to_value(config.geometry(&a, &b, ego)).unwrap(),
            row["geometry"],
            "geometry/{i}"
        );
        assert_eq!(
            group_config::vision_support(&a, &cues),
            row["support"].as_bool().unwrap(),
            "support/{i}"
        );
    }
}
