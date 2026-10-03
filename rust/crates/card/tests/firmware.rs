use openpilot_card::firmware::{Catalog, Firmware, MatchOptions};

#[test]
fn firmware_match_preserves_exact_candidate_and_logging_exclusion() {
    let catalog = Catalog::load().unwrap();
    let model = catalog
        .models
        .iter()
        .find(|model| model.name == "COMMA_BODY")
        .unwrap();
    let versions: Vec<_> = model
        .firmware
        .iter()
        .map(|expected| Firmware {
            ecu: expected.ecu,
            address: expected.address,
            sub_address: expected.subaddress.unwrap_or(0),
            fw_version: expected.versions[0].clone(),
            brand: model.brand.as_str().to_owned(),
            logging: false,
            ..Firmware::default()
        })
        .collect();
    let matched = catalog
        .match_car(
            &versions,
            "00000000000000000",
            MatchOptions {
                exact: true,
                fuzzy: false,
            },
        )
        .unwrap();
    assert!(matched.exact);
    assert!(matched.candidates.contains(&model.name));
    let logged: Vec<_> = versions
        .into_iter()
        .map(|mut version| {
            version.logging = true;
            version
        })
        .collect();
    let matched = catalog
        .match_car(
            &logged,
            "00000000000000000",
            MatchOptions {
                exact: true,
                fuzzy: false,
            },
        )
        .unwrap();
    assert!(!matched.candidates.contains(&model.name));
}

#[test]
fn manual_selection_is_exact_and_unknown_choices_remain_unselected() {
    let catalog = Catalog::load().unwrap();
    for (name, expected) in &catalog.selected {
        assert_eq!(catalog.selected_platform(name), Some(expected.as_str()));
        assert_eq!(catalog.selected_platform(&format!("{name} ")), None);
    }
}
