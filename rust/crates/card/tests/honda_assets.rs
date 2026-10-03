use openpilot_card::brands::honda::{self, Error, Honda, ParamsInput, Setup};
use openpilot_params::Params;

const MESSAGES: &str = "BO_ 1 STEER_STATUS: 8 EPS\n SG_ STEER_STATUS : 0|4@1+ (1,0) [0|15] \"\" EPS\nBO_ 2 GEARBOX: 8 TCM\n SG_ GEAR_SHIFTER : 0|4@1+ (1,0) [0|15] \"\" TCM\n";
const STEERING: &str = "VAL_ 1 STEER_STATUS 0 \"NORMAL\";\n";
const GEARS: &str = "VAL_ 2 GEAR_SHIFTER 0 \"P\";\n";

fn construct(dbc: Option<&str>) -> Result<Honda, Error> {
    construct_for(
        "HONDA_CIVIC",
        "honda_civic_touring_2016_can_generated",
        &[],
        dbc,
    )
}

fn construct_for(
    candidate: &str,
    name: &str,
    fingerprints: &[(u8, Vec<(u32, usize)>)],
    dbc: Option<&str>,
) -> Result<Honda, Error> {
    let directory = tempfile::tempdir().unwrap();
    if let Some(dbc) = dbc {
        std::fs::write(directory.path().join(format!("{name}.dbc")), dbc).unwrap();
    }
    let settings = Params::open(directory.path(), "d").unwrap();
    let message = honda::parameters(ParamsInput {
        candidate,
        fingerprints,
        firmware: &[],
        alpha_long: false,
        settings: &settings,
    })
    .unwrap();
    Honda::new(Setup {
        params_bytes: &capnp::serialize::write_message_to_words(&message),
        dbc_root: directory.path(),
        settings,
        fingerprints,
        now_ns: 2_000_000_000,
    })
}

#[test]
fn invalid_pt_definitions_fail_before_missing_bsm_body_dbc() {
    let fingerprints = [(0, vec![(0x12f8bfa7, 8)])];
    let result = construct_for(
        "HONDA_CRV_5G",
        "honda_crv_ex_2017_can_generated",
        &fingerprints,
        Some(MESSAGES),
    );
    assert!(
        matches!(result, Err(Error::Signal(name)) if name == "GEARBOX.GEAR_SHIFTER definitions")
    );
}
#[test]
fn missing_dbc_is_a_typed_fatal_startup_error() {
    assert!(matches!(
        construct(None),
        Err(Error::Can(openpilot_can::Error::Io(_)))
    ));
}
#[test]
fn missing_gear_definitions_fail_before_steering_definitions() {
    for values in [MESSAGES.to_owned(), format!("{MESSAGES}{STEERING}")] {
        assert!(
            matches!(construct(Some(&values)), Err(Error::Signal(name)) if name == "GEARBOX.GEAR_SHIFTER definitions")
        );
    }
}
#[test]
fn missing_steering_definitions_fail_at_construction() {
    let values = format!("{MESSAGES}{GEARS}");
    assert!(
        matches!(construct(Some(&values)), Err(Error::Signal(name)) if name == "STEER_STATUS.STEER_STATUS definitions")
    );
}
