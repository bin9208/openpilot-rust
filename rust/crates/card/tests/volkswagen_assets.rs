use openpilot_card::brands::volkswagen::{self, Error, ParamsInput, Setup, Volkswagen};
use openpilot_params::Params;
use std::path::PathBuf;

struct Boundary {
    candidate: &'static str,
    dbc: &'static str,
    gear: (&'static str, &'static str),
    hca: (&'static str, &'static str),
    fingerprints: Vec<(u8, Vec<(u32, usize)>)>,
}
fn boundaries() -> Vec<Boundary> {
    vec![
        Boundary {
            candidate: "VOLKSWAGEN_PASSAT_NMS",
            dbc: "vw_pq",
            gear: ("Getriebe_1", "Waehlhebelposition__Getriebe_1_"),
            hca: ("Lenkhilfe_2", "LH2_Sta_HCA"),
            fingerprints: vec![(0, vec![(0x440, 8)])],
        },
        Boundary {
            candidate: "VOLKSWAGEN_GOLF_MK7",
            dbc: "vw_mqb",
            gear: ("Gateway_73", "GE_Fahrstufe"),
            hca: ("LH_EPS_03", "EPS_HCA_Status"),
            fingerprints: vec![(0, vec![(0xad, 8)])],
        },
        Boundary {
            candidate: "VOLKSWAGEN_GOLF_MK7",
            dbc: "vw_mqb",
            gear: ("Motor_EV_01", "MO_Waehlpos"),
            hca: ("LH_EPS_03", "EPS_HCA_Status"),
            fingerprints: vec![(0, vec![(0x187, 8)])],
        },
        Boundary {
            candidate: "VOLKSWAGEN_ID4_MK1",
            dbc: "vw_meb",
            gear: ("Getriebe_11", "GE_Fahrstufe"),
            hca: ("QFK_01", "LatCon_HCA_Status"),
            fingerprints: vec![(0, vec![(0x3dc, 8)])],
        },
        Boundary {
            candidate: "VOLKSWAGEN_ID4_MK2",
            dbc: "vw_meb_2024",
            gear: ("Getriebe_11", "GE_Fahrstufe"),
            hca: ("QFK_01", "LatCon_HCA_Status"),
            fingerprints: vec![(0, vec![(0x3dc, 8)])],
        },
    ]
}
fn original(boundary: &Boundary) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../opendbc_repo/opendbc/dbc");
    std::fs::read_to_string(root.join(format!("{}.dbc", boundary.dbc))).unwrap()
}
fn without(source: &str, signal: &str) -> String {
    source
        .lines()
        .filter(|line| !line.starts_with("VAL_ ") || line.split_whitespace().nth(2) != Some(signal))
        .map(|line| format!("{line}\n"))
        .collect()
}
fn construct(boundary: &Boundary, source: Option<&str>) -> Result<Volkswagen, Error> {
    let directory = tempfile::tempdir().unwrap();
    if let Some(source) = source {
        std::fs::write(
            directory.path().join(format!("{}.dbc", boundary.dbc)),
            source,
        )
        .unwrap();
    }
    let settings = Params::open(directory.path(), "d").unwrap();
    let cp = volkswagen::parameters(ParamsInput {
        candidate: boundary.candidate,
        fingerprints: &boundary.fingerprints,
        firmware: &[],
        alpha_long: false,
        settings: &settings,
    })
    .unwrap();
    Volkswagen::new(Setup {
        params_bytes: &capnp::serialize::write_message_to_words(&cp),
        dbc_root: directory.path(),
        settings,
        fingerprints: &boundary.fingerprints,
        now_ns: 2_000_000_000,
    })
}
#[test]
fn missing_dbc_is_typed_fatal_for_all_reachable_families() {
    for boundary in boundaries() {
        assert!(matches!(
            construct(&boundary, None),
            Err(Error::Can(openpilot_can::Error::Io(_)))
        ));
    }
}
#[test]
fn gear_definition_failure_precedes_hca_definition_failure() {
    for boundary in boundaries() {
        let source = without(
            &without(&original(&boundary), boundary.gear.1),
            boundary.hca.1,
        );
        let expected = format!("{}.{} definitions", boundary.gear.0, boundary.gear.1);
        assert!(
            matches!(construct(&boundary,Some(&source)),Err(Error::Signal(name)) if name==expected)
        );
    }
}
#[test]
fn hca_definition_failure_occurs_at_construction() {
    for boundary in boundaries() {
        let source = without(&original(&boundary), boundary.hca.1);
        let expected = format!("{}.{} definitions", boundary.hca.0, boundary.hca.1);
        assert!(
            matches!(construct(&boundary,Some(&source)),Err(Error::Signal(name)) if name==expected)
        );
    }
}
#[test]
fn alt_gear_still_requires_original_getriebe_gear_definitions() {
    for boundary in boundaries()
        .into_iter()
        .filter(|b| b.dbc.starts_with("vw_meb"))
    {
        let source = without(&original(&boundary), boundary.gear.1);
        assert!(
            matches!(construct(&boundary,Some(&source)),Err(Error::Signal(name)) if name=="Getriebe_11.GE_Fahrstufe definitions")
        );
    }
}
