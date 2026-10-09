use crate::Error;

pub const IMAGE_BYTES: usize = 393216;

pub fn device_family(model: &str) -> bool {
    matches!(
        model.trim_end_matches('\0').trim().split("comma ").last(),
        Some("tici" | "tizi")
    )
}

pub fn sampling_boundary_only(
    actual: &[u8],
    expected: &[u8],
    frames: &[u8],
    camera: [u32; 2],
    transforms: &[u8],
) -> Result<bool, Error> {
    let [width, height] = camera.map(|value| value as usize);
    let stride = width.next_multiple_of(128);
    let y_height = height.next_multiple_of(32);
    let frame_size = stride * (y_height + (height / 2).next_multiple_of(16));
    if ![[1344, 760], [1928, 1208]].contains(&camera)
        || actual.len() != IMAGE_BYTES
        || expected.len() != IMAGE_BYTES
        || frames.len() != 2 * frame_size
        || transforms.len() != 72
    {
        return Err(Error::Contract("warp validation layout mismatch"));
    }
    let matrices = transforms
        .chunks_exact(4)
        .map(|bytes| {
            f64::from(f32::from_le_bytes(
                bytes.try_into().expect("four byte chunk"),
            ))
        })
        .collect::<Vec<_>>();
    for (index, (&a, &b)) in actual.iter().zip(expected).enumerate() {
        if a == b {
            continue;
        }
        let camera_index = index / (6 * 128 * 256);
        let channel = index / (128 * 256) % 6;
        let row = index / 256 % 128;
        let column = index % 256;
        let uv = channel >= 4;
        let (x, y) = if uv {
            (column, row)
        } else {
            (2 * column + channel / 2, 2 * row + channel % 2)
        };
        let mut matrix: [f64; 9] = matrices[camera_index * 9..camera_index * 9 + 9]
            .try_into()
            .map_err(|_| Error::Contract("warp matrix size"))?;
        if uv {
            for (value, scale) in matrix
                .iter_mut()
                .zip([1., 1., 0.5, 1., 1., 0.5, 2., 2., 1.])
            {
                *value *= scale;
            }
        }
        let projected = std::array::from_fn::<_, 3, _>(|r| {
            matrix[3 * r] * x as f64 + matrix[3 * r + 1] * y as f64 + matrix[3 * r + 2]
        });
        if projected.iter().any(|value| !value.is_finite()) || projected[2].abs() < 0.5 {
            return Ok(false);
        }
        let source = [projected[0] / projected[2], projected[1] / projected[2]];
        if source.iter().any(|value| !value.is_finite()) {
            return Ok(false);
        }
        let mut candidates = [[0usize; 2]; 2];
        let mut boundary = false;
        for axis in 0..2 {
            let value = source[axis];
            let floor = value.floor();
            let near = (value - floor - 0.5).abs() <= 0.00025;
            boundary |= near;
            let limit = if axis == 0 { width } else { height } / if uv { 2 } else { 1 };
            let coords = if near {
                [floor, floor + 1.]
            } else {
                [value.round_ties_even(); 2]
            };
            candidates[axis] = coords.map(|v| v.clamp(0., (limit - 1) as f64) as usize);
        }
        if !boundary {
            return Ok(false);
        }
        let mut actual_found = false;
        let mut expected_found = false;
        for sy in candidates[1] {
            for sx in candidates[0] {
                let offset = if uv {
                    stride * y_height + sy * stride + 2 * sx + channel - 4
                } else {
                    sy * stride + sx
                };
                let value = frames[camera_index * frame_size + offset];
                actual_found |= a == value;
                expected_found |= b == value;
            }
        }
        if !actual_found || !expected_found {
            return Ok(false);
        }
    }
    Ok(true)
}
