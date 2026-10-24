#[path = "../src/compiler_launch.rs"]
mod compiler_launch;

use compiler_launch::{launch_dimensions, Dimension};

const BINARY: &[u8] = include_bytes!("fixtures/qcom/buffer_add.bin");
const SOURCE: &str = "__kernel void run(__global float *out) { out[get_global_id(0)] = 1.0f; }";

fn dims(values: &[i128]) -> Vec<Dimension> {
    values.iter().copied().map(Dimension::Integer).collect()
}

#[test]
fn covers_the_original_work_item_domain_without_padding() {
    for (items, groups, local) in [
        ([128, 1, 1], [2, 1, 1], [64, 1, 1]),
        ([6, 12, 8], [3, 3, 1], [2, 4, 8]),
        ([3, 5, 7], [3, 5, 7], [1, 1, 1]),
        ([1, 2, 128], [1, 1, 4], [1, 2, 32]),
        ([1, 1, 1], [1, 1, 1], [1, 1, 1]),
        (
            [u32::MAX as i128, 2, 1],
            [u32::MAX as i128, 1, 1],
            [1, 2, 1],
        ),
        (
            [u32::MAX as i128 - 1, 1, 1],
            [i32::MAX as i128, 1, 1],
            [2, 1, 1],
        ),
    ] {
        let result = launch_dimensions(&dims(&items), None, SOURCE, BINARY).unwrap();
        assert_eq!(result, (dims(&groups), dims(&local)));
        for axis in 0..3 {
            assert_eq!(groups[axis] * local[axis], items[axis]);
        }
        assert!(local.iter().product::<i128>() <= 64);
        if items.iter().product::<i128>() < 4096 {
            let mut visited = std::collections::BTreeSet::new();
            for gx in 0..groups[0] {
                for gy in 0..groups[1] {
                    for gz in 0..groups[2] {
                        for lx in 0..local[0] {
                            for ly in 0..local[1] {
                                for lz in 0..local[2] {
                                    assert!(visited.insert([
                                        gx * local[0] + lx,
                                        gy * local[1] + ly,
                                        gz * local[2] + lz,
                                    ]));
                                }
                            }
                        }
                    }
                }
            }
            assert_eq!(visited.len() as i128, items.iter().product::<i128>());
            assert!(visited
                .iter()
                .all(|point| (0..3).all(|axis| point[axis] < items[axis])));
        }
    }
}

#[test]
fn rejects_noninteger_unbounded_or_nonthree_dimensional_inferred_domains() {
    let mut cases = vec![dims(&[1, 1]), dims(&[1, 1, 1, 1])];
    for value in [
        Dimension::Integer(0),
        Dimension::Integer(-1),
        Dimension::Integer(u32::MAX as i128 + 1),
        Dimension::Boolean(true),
        Dimension::Float(1.0),
        Dimension::Float(f64::INFINITY),
        Dimension::Float(f64::NAN),
    ] {
        cases.push(vec![value, Dimension::Integer(1), Dimension::Integer(1)]);
    }
    for items in cases {
        assert!(launch_dimensions(&items, None, SOURCE, BINARY)
            .unwrap_err()
            .contains("launch dimensions"));
    }
}

#[test]
fn preserves_single_thread_groups_for_each_group_sensitive_token() {
    for builtin in [
        "get_local_id",
        "get_group_id",
        "get_local_size",
        "get_num_groups",
        "barrier",
        "work_group_barrier",
        "__local",
    ] {
        let items = dims(&[128, 2, 1]);
        let source = format!("{SOURCE}\n{builtin} (0);");
        // This branch must not inspect binary metadata, including an absent binary.
        assert_eq!(
            launch_dimensions(&items, None, &source, &[]).unwrap(),
            (items, dims(&[1, 1, 1]))
        );
    }
}

#[test]
fn matches_word_boundaries_rather_than_substrings() {
    for source in [
        "my_barrier(0);",
        "barrier_suffix(0);",
        "get_local_id2(0);",
        "__locals value;",
    ] {
        assert_eq!(
            launch_dimensions(&dims(&[128, 1, 1]), None, source, BINARY).unwrap(),
            (dims(&[2, 1, 1]), dims(&[64, 1, 1]))
        );
    }
}

#[test]
fn combining_marks_are_python_regex_word_boundaries() {
    // Python re treats these marks as non-word characters even when Rust's
    // Alphabetic property includes them. Both expose a genuine barrier token.
    for source in ["\u{0345}barrier(0);", "barrier\u{05b0}(0);"] {
        assert_eq!(
            launch_dimensions(&dims(&[128, 1, 1]), None, source, BINARY).unwrap(),
            (dims(&[128, 1, 1]), dims(&[1, 1, 1]))
        );
    }
}

#[test]
fn explicit_geometry_survives_without_reinterpretation_or_binary_inspection() {
    for (global, local) in [
        (
            vec![
                Dimension::Float(3.5),
                Dimension::Integer(2),
                Dimension::Integer(1),
            ],
            dims(&[16, 1, 1]),
        ),
        (dims(&[64, 2, 1]), dims(&[1, 1, 1])),
    ] {
        assert_eq!(
            launch_dimensions(&global, Some(&local), "barrier(0);", &[]).unwrap(),
            (global, local)
        );
    }
}

fn registers(full: u32, half: u32) -> Vec<u8> {
    let mut binary = BINARY.to_vec();
    let offset = u32::from_le_bytes(binary[0x34..0x38].try_into().unwrap()) as usize + 0x14;
    binary[offset..offset + 4].copy_from_slice(&full.to_le_bytes());
    binary[offset + 4..offset + 8].copy_from_slice(&half.to_le_bytes());
    binary
}

#[test]
fn accepts_the_maximum_supported_register_pressure() {
    assert_eq!(
        launch_dimensions(&dims(&[128, 1, 1]), None, SOURCE, &registers(63, 63)).unwrap(),
        (dims(&[2, 1, 1]), dims(&[64, 1, 1]))
    );
}

#[test]
fn rejects_unsupported_register_counts() {
    for (full, half) in [(64, 0), (0, 64), (u32::MAX, 0)] {
        assert!(
            launch_dimensions(&dims(&[128, 1, 1]), None, SOURCE, &registers(full, half))
                .unwrap_err()
                .contains("register")
        );
    }
}

#[test]
fn rejects_truncated_or_out_of_bounds_register_metadata() {
    let mut invalid_pointer = vec![0; 0x34];
    invalid_pointer.extend_from_slice(&u32::MAX.to_le_bytes());
    for binary in [vec![], vec![0; 0x38], invalid_pointer] {
        assert!(
            launch_dimensions(&dims(&[128, 1, 1]), None, SOURCE, &binary)
                .unwrap_err()
                .contains("register")
        );
    }
    let mut binary = BINARY.to_vec();
    binary[0x34..0x38].copy_from_slice(&0x37u32.to_le_bytes());
    assert!(
        launch_dimensions(&dims(&[128, 1, 1]), None, SOURCE, &binary)
            .unwrap_err()
            .contains("register")
    );
}
