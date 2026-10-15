#[path = "../src/onroad/driver_preview/policy.rs"]
mod policy;

#[test]
fn pinned_source_presentation_vectors() {
    for (line, row) in include_str!("fixtures/dm-preview.tsv").lines().enumerate() {
        let v: Vec<_> = row.split('\t').collect();
        let number = |s: &str| s.parse::<f64>().unwrap();
        match v[0] {
            "state" => {
                let b: Vec<_> = v[1].bytes().map(|b| b == b'1').collect();
                let actual = policy::state(policy::Input {
                    monitoring_fresh: b[0],
                    camera_unavailable: b[1],
                    driver_fresh: b[2],
                    frame_fresh: b[3],
                    face_detected: b[4],
                    distracted: b[5],
                    lockout: b[6],
                    wheel_policy: b[7],
                });
                assert_eq!(actual, (v[2], v[3]), "line {line}");
            }
            "fresh" => assert_eq!(
                policy::sample_fresh(number(v[1]), number(v[2])),
                v[3] == "1",
                "line {line}"
            ),
            "crop" => {
                let pos: Vec<_> = if v[4] == "-" {
                    vec![]
                } else {
                    v[4].split(',').map(number).collect()
                };
                let actual = policy::face_crop(number(v[1]), number(v[2]), &pos, number(v[3]));
                if v[5] == "-" {
                    assert_eq!(actual, None, "line {line}");
                } else {
                    let expected: Vec<_> = v[5].split(',').map(number).collect();
                    for (a, e) in actual.unwrap().iter().zip(expected) {
                        assert!((a - e).abs() <= 1e-12, "line {line}: {a} != {e}");
                    }
                }
            }
            "rect" => assert_eq!(
                policy::rect(
                    number(v[2]),
                    number(v[3]),
                    number(v[4]),
                    number(v[5]),
                    v[1] == "1"
                ),
                [number(v[6]), number(v[7]), number(v[8]), number(v[9])]
            ),
            _ => panic!("unknown vector"),
        }
    }
}
