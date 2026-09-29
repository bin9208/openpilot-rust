use openpilot_model_runtime::qcom::{Argument, Dispatch, ProgramImage};
use serde_json::Value;
use std::path::PathBuf;

fn fixture(name: &str, suffix: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(format!("tests/fixtures/qcom/{name}.{suffix}")),
    )
    .unwrap()
}

#[test]
fn original_qcom_metadata_arguments_and_packets_match() {
    for name in ["buffer_add", "scalar_add", "image_copy", "constants"] {
        let program = ProgramImage::parse(name, &fixture(name, "bin")).unwrap();
        let oracle: Value = serde_json::from_slice(&fixture(name, "json")).unwrap();
        let metadata = serde_json::to_value(&program).unwrap();
        for (field, value) in metadata.as_object().unwrap() {
            assert_eq!(value, &oracle[field], "{name} {field}");
        }
        for element_bytes in if name == "image_copy" {
            vec![4, 2]
        } else {
            vec![0]
        } {
            let arguments: Vec<_> = [0x678900000, 0x789a00000]
                .into_iter()
                .map(|address| {
                    if element_bytes == 0 {
                        Argument::Buffer { address }
                    } else {
                        Argument::Image {
                            address,
                            width: 64,
                            height: 64,
                            pitch: 64 * 4 * element_bytes,
                            element_bytes,
                        }
                    }
                })
                .collect();
            let scalars = if name == "scalar_add" {
                vec![7]
            } else {
                vec![]
            };
            let bytes = program.arguments(&arguments, &scalars).unwrap();
            let variant = match element_bytes {
                0 => "buffer",
                2 => "f16",
                _ => "f32",
            };
            assert_eq!(
                bytes,
                fixture(&format!("{name}-{variant}"), "args"),
                "{name} {variant}"
            );
        }
        let dispatch = Dispatch {
            program: 0x123400000,
            stack: 0x234500000,
            border: 0x345600000,
            dummy: 0x456700000,
            args: 0x567800000,
            global: [3.0, 5.0, 2.0],
            local: [8, 4, 1],
        };
        assert_eq!(
            serde_json::to_value(program.dispatch(&dispatch).unwrap()).unwrap(),
            oracle["queue"],
            "{name}"
        );
        assert_eq!(
            serde_json::to_value(ProgramImage::memory_barrier(dispatch.dummy)).unwrap(),
            oracle["barrier"]
        );
        let fractional = Dispatch {
            global: [2.5, 3.25, 1.0],
            ..dispatch
        };
        assert_eq!(
            serde_json::to_value(program.dispatch(&fractional).unwrap()).unwrap(),
            oracle["fractional_queue"]
        );
    }
}

#[test]
fn truncated_qcom_metadata_never_panics() {
    let binary = fixture("constants", "bin");
    for length in 0..binary.len() {
        let result = ProgramImage::parse("constants", &binary[..length]);
        assert!(result.is_err(), "accepted truncated binary at {length}");
    }
}

#[test]
fn rejects_malformed_qcom_descriptors() {
    let original = fixture("constants", "bin");
    for (offset, value) in [
        (0xc0, u32::MAX),
        (0x100, u32::MAX),
        (0x110, u32::MAX),
        (0x34, u32::MAX),
        (0xac, u32::MAX),
    ] {
        let mut binary = original.clone();
        binary[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        assert!(
            ProgramImage::parse("constants", &binary).is_err(),
            "offset {offset:#x}"
        );
    }
    let program = ProgramImage::parse("buffer_add", &fixture("buffer_add", "bin")).unwrap();
    assert!(program.arguments(&[], &[]).is_err());
    let mut dispatch = Dispatch {
        program: 0,
        stack: 0,
        border: 0,
        dummy: 0,
        args: 0,
        global: [1.0; 3],
        local: [1; 3],
    };
    dispatch.local[0] = 0;
    assert!(program.dispatch(&dispatch).is_err());
    dispatch.local = [1024; 3];
    assert!(program.dispatch(&dispatch).is_err());
    dispatch.local = [2; 3];
    dispatch.global[0] = f64::from(u32::MAX);
    assert!(program.dispatch(&dispatch).is_err());
    for value in [f64::NAN, f64::INFINITY, -1.0, 0.0] {
        dispatch.global[0] = value;
        assert!(program.dispatch(&dispatch).is_err());
    }
}

#[test]
fn mutated_qcom_sections_remain_bounded() {
    for name in ["buffer_add", "scalar_add", "image_copy", "constants"] {
        let original = fixture(name, "bin");
        assert!(ProgramImage::parse("wrong_kernel", &original).is_err());
        for offset in (0..original.len() - 4).step_by(4) {
            for value in [0_u32, 1, 0x80000000, u32::MAX] {
                let mut binary = original.clone();
                binary[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
                let _ = ProgramImage::parse(name, &binary);
            }
        }
    }
}

#[test]
fn rejects_shader_unit_count_that_overflows_the_packet_field() {
    for units in [1023_u32, 1024] {
        let mut binary = fixture("buffer_add", "bin");
        let offset = binary.len();
        binary.resize(offset + units as usize * 128, 0);
        binary[0xc0..0xc4].copy_from_slice(&(offset as u32).to_le_bytes());
        binary[0x100..0x104].copy_from_slice(&(units * 128).to_le_bytes());
        assert_eq!(
            ProgramImage::parse("buffer_add", &binary).is_ok(),
            units == 1023
        );
    }
}
