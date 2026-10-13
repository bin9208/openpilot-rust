use openpilot_cluster::h264::sps::{self, PatchCache};
use serde::{Deserialize, Serialize};
#[derive(Deserialize)]
struct Case {
    data: Vec<u8>,
    width: i64,
    height: i64,
    fps: i64,
}
#[derive(Serialize)]
struct Output {
    constraints: (Vec<u8>, bool),
    crop: (Vec<u8>, bool, String),
    vui: (Vec<u8>, bool, String),
    nals: Vec<(usize, usize, usize)>,
    starts: Vec<(usize, usize)>,
    units: Vec<Vec<u8>>,
    first: Option<Vec<u8>>,
    prepared: Vec<Vec<u8>>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, input] = args.as_slice() else {
        return Err("cluster_sps_fixture CASES".into());
    };
    let cases: Vec<Case> = serde_json::from_slice(&std::fs::read(input)?)?;
    let mut outputs = Vec::new();
    for case in cases {
        let mut cache = PatchCache::default();
        let prepared = [true, true, false]
            .into_iter()
            .map(|enabled| {
                cache.prepare(
                    case.data.clone(),
                    case.width,
                    case.height,
                    case.fps,
                    enabled,
                )
            })
            .collect();
        outputs.push(Output {
            constraints: sps::patch_constraints(&case.data),
            crop: sps::patch_crop(&case.data, case.width, case.height),
            vui: sps::patch_vui(&case.data, case.fps),
            nals: sps::nals(&case.data),
            starts: sps::start_codes(&case.data),
            units: sps::byte_stream_units(&case.data),
            first: sps::first_unit(&case.data, 7),
            prepared,
        });
    }
    println!("{}", serde_json::to_string(&outputs)?);
    Ok(())
}
