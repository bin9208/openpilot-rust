use openpilot_usbgpu::{amd_metadata::Catalog, discovery::Discovery};
#[test]
fn native_layout_round_trip_and_truncation_are_checked() {
    let catalog = Catalog::bundled().unwrap();
    let name = "struct_ip_discovery_header";
    let mut bytes = vec![0; catalog.layout(name).unwrap().size];
    catalog.write(name, &["num_dies"], &mut bytes, 2).unwrap();
    catalog
        .write(name, &["base_addr_64_bit"], &mut bytes, 1)
        .unwrap();
    assert_eq!(catalog.read(name, &["num_dies"], &bytes).unwrap(), 2);
    assert_eq!(
        catalog.read(name, &["base_addr_64_bit"], &bytes).unwrap(),
        1
    );
    assert!(catalog
        .write(name, &["base_addr_64_bit"], &mut bytes, 2)
        .is_err());
    for size in 0..60 {
        assert!(Discovery::parse(&catalog, &vec![0; size]).is_err());
    }
    assert!(catalog.register_module("gc", [10, 0, 0]).is_err());
    assert!(catalog.register_module("gc", [12, 0, 0]).is_ok());
}
