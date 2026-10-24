use openpilot_image_codecs::{avif, jpeg2000};
#[test]
fn pinned_original_avif_and_jpeg2000_arrays() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../dev-tools/tests/fixtures/image-array");
    for mode in ["L", "LA", "RGB", "RGBA", "CMYK", "I-16"] {
        for format in ["avif", "jpeg2000"] {
            for variant in if format == "avif" {
                &["default"][..]
            } else {
                &["jp2", "j2k"][..]
            } {
                let name = format!("native-{format}-{mode}-{variant}");
                let bytes = std::fs::read(fixtures.join(format!("{name}.image"))).unwrap();
                let image = if format == "avif" {
                    avif(&bytes, 178_956_970)
                } else {
                    jpeg2000(
                        &bytes,
                        178_956_970,
                        if mode == "I-16" { 16 } else { 8 },
                        false,
                    )
                }
                .unwrap();
                let expected = std::fs::read(
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures")
                        .join(format!("{name}.i64")),
                )
                .unwrap();
                let actual = image
                    .values
                    .iter()
                    .flat_map(|v| v.to_le_bytes())
                    .collect::<Vec<_>>();
                assert_eq!(actual, expected, "{name} every original integer sample");
                assert_eq!((image.width, image.height), (16, 16), "{name}");
                assert_eq!(
                    image.values.len(),
                    16 * 16 * image.components as usize,
                    "{name}"
                );
                assert!(
                    if format == "avif" {
                        avif(&bytes, 255)
                    } else {
                        jpeg2000(&bytes, 255, 8, false)
                    }
                    .is_err(),
                    "{name} pixel bound"
                );
            }
        }
    }
}
#[test]
fn malformed_codec_streams_return_owned_errors() {
    for _ in 0..16 {
        for bytes in [&[][..], &[255, 79, 255, 81][..], b"not an image"] {
            assert!(avif(bytes, 178_956_970).is_err());
            assert!(jpeg2000(bytes, 178_956_970, 8, false).is_err());
        }
    }
}

#[test]
fn original_mode_and_every_integer_sample_including_palette_variants() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for line in include_str!("fixtures/cases.tsv").lines() {
        let row = line.split('\t').collect::<Vec<_>>();
        let name = row[0];
        let bytes = std::fs::read(
            root.join("../dev-tools/tests/fixtures/image-array")
                .join(format!("{name}.image")),
        )
        .unwrap();
        let result = if row[1] == "avif" {
            openpilot_image_codecs::policy::decode_avif(&bytes, 178_956_970)
        } else {
            openpilot_image_codecs::policy::decode_jpeg2000(&bytes, 178_956_970)
        };
        if row[2] == "error" {
            assert!(result.is_err(), "{name} original rejection");
            continue;
        }
        let image = result.unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(image.mode, row[2], "{name} loaded mode");
        assert_eq!(
            (image.width, image.height),
            (row[3].parse().unwrap(), row[4].parse().unwrap()),
            "{name} shape"
        );
        let expected =
            std::fs::read(root.join("tests/fixtures").join(format!("{name}.i64"))).unwrap();
        assert_eq!(
            image
                .values
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>(),
            expected,
            "{name} all integer samples"
        );
    }
}

#[test]
fn original_tiff_compressions_modes_and_integer_samples() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for line in include_str!("fixtures/tiff-cases.tsv").lines() {
        let row = line.split('\t').collect::<Vec<_>>();
        let name = row[0];
        let bytes = std::fs::read(
            root.join("../dev-tools/tests/fixtures/image-array")
                .join(format!("{name}.image")),
        )
        .unwrap();
        let result = openpilot_image_codecs::tiff::decode(&bytes, 178_956_970);
        if row[1] == "error" {
            assert!(result.is_err(), "{name} original rejection");
            continue;
        }
        let image = result.unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(image.mode, row[1], "{name} original mode");
        assert_eq!(
            (image.width, image.height),
            (row[2].parse().unwrap(), row[3].parse().unwrap()),
            "{name} original shape"
        );
        let expected =
            std::fs::read(root.join("tests/fixtures").join(format!("{name}.i64"))).unwrap();
        assert_eq!(
            image
                .values
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>(),
            expected,
            "{name} every original integer sample"
        );
    }
}
