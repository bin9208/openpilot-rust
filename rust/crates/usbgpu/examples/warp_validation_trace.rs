use openpilot_usbgpu::{
    warp_validation::{self, sampling_boundary_only, IMAGE_BYTES},
    Error,
};
use serde::Deserialize;
use serde_json::json;
use std::{fs, path::Path};

#[derive(Deserialize)]
struct Case {
    name: String,
    camera: [u32; 2],
    frames: String,
    transforms: Vec<u8>,
    differences: Vec<(usize, u8, u8)>,
    expected: bool,
}
fn main() -> Result<(), Error> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err(Error::Contract("expected input and receipt paths"));
    }
    let cases: Vec<Case> = serde_json::from_slice(&fs::read(&args[0])?)?;
    let root = Path::new(&args[0])
        .parent()
        .ok_or(Error::Contract("input parent"))?;
    let mut rows = Vec::new();
    for case in cases {
        let frames = fs::read(root.join(case.frames))?;
        let mut actual = vec![0; IMAGE_BYTES];
        let mut expected = vec![0; IMAGE_BYTES];
        for (index, a, b) in &case.differences {
            actual[*index] = *a;
            expected[*index] = *b;
        }
        let accepted =
            sampling_boundary_only(&actual, &expected, &frames, case.camera, &case.transforms)?;
        if accepted != case.expected {
            return Err(Error::Contract("source sampling predicate mismatch"));
        }
        rows.push(
            json!({"name":case.name,"accepted":accepted,"source":case.expected,"passed":true}),
        );
    }
    let families = ["comma tici", "comma tizi\0", "comma mici", "", "tici2"]
        .map(|value| (value, warp_validation::device_family(value)));
    if families.map(|(_, enabled)| enabled) != [true, true, false, false, false] {
        return Err(Error::Contract("C3 family selection mismatch"));
    }
    let receipt = json!({"scenario":"Native sampling acceptance versus unchanged Python source predicate", "rows":rows,"families":families,"passed":true});
    fs::write(&args[1], serde_json::to_vec_pretty(&receipt)?)?;
    println!("{receipt}");
    Ok(())
}
