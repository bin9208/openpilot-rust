use openpilot_usbgpu::amd_metadata::Catalog;
#[test]
fn gfx12_scratch_bitfield_fits_declared_four_byte_record() {
    let catalog = Catalog::bundled().unwrap();
    let mut bytes = [0xff; 4];
    catalog
        .write(
            "union_COMPUTE_TMPRING_SIZE_GFX12_bitfields",
            &["WAVESIZE"],
            &mut bytes,
            0x23456,
        )
        .unwrap();
    assert_eq!(
        u32::from_le_bytes(bytes),
        (!(0x3ffffu32 << 12)) | (0x23456 << 12)
    );
    assert_eq!(
        catalog
            .read(
                "union_COMPUTE_TMPRING_SIZE_GFX12_bitfields",
                &["WAVESIZE"],
                &bytes
            )
            .unwrap(),
        0x23456
    );
}
