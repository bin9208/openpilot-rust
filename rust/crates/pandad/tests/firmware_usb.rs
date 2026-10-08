#![cfg(feature = "native-skip-miri")]
use openpilot_pandad::firmware::{usb::descriptor_mcu, Mcu};

#[test]
fn dfu_h7_identification_counts_the_unprogrammable_provisioning_sector() {
    assert_eq!(
        descriptor_mcu("@Internal Flash /0x08000000/08*128Kg").unwrap(),
        Mcu::H7
    );
    assert_eq!(Mcu::H7.sectors().len(), 7);
    assert!(descriptor_mcu("@Internal Flash /0x08000000/07*128Kg").is_err());
}

#[test]
fn dfu_f4_identification_sums_mixed_sector_groups() {
    assert_eq!(
        descriptor_mcu("@Internal Flash /0x08000000/04*016Kg,01*064Kg,011*128Kg").unwrap(),
        Mcu::F4
    );
    assert!(descriptor_mcu("@Internal Flash /0x08000000/invalid").is_err());
}
