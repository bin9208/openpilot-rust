pub struct Case {
    pub name: &'static str,
    pub predictions: Vec<f32>,
    pub prototypes: Vec<f32>,
}

pub fn cases() -> Vec<Case> {
    let mut state = 416u64;
    let mut next = || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        ((state >> 40) as i32 - (1 << 23)) as f32 / (1 << 23) as f32
    };
    [
        "positive",
        "empty",
        "upper-only",
        "below-cutoff",
        "ignored",
        "cancellation",
        "nonfinite",
        "mixed",
    ]
    .into_iter()
    .map(|name| {
        let anchors = if name == "mixed" { 48 } else { 8 };
        let mut predictions = vec![0.0; 42 * anchors];
        for anchor in 0..anchors {
            for (row, value) in [(0, 208.0), (1, 208.0), (2, 416.0), (3, 416.0)] {
                predictions[row * anchors + anchor] = value;
            }
            if name == "below-cutoff" {
                predictions[anchors + anchor] = 80.0;
                predictions[3 * anchors + anchor] = 80.0;
            }
            if name == "mixed" {
                predictions[anchor] = (next() + 1.0) * 210.0;
                predictions[anchors + anchor] = (next() + 1.0) * 210.0;
                predictions[2 * anchors + anchor] = (next() + 1.0) * 100.0;
                predictions[3 * anchors + anchor] = (next() + 1.0) * 100.0;
            }
            let class = if name == "ignored" { 3 } else { anchor % 6 };
            predictions[(4 + class) * anchors + anchor] = 0.9 - anchor as f32 * 0.002;
            for channel in 0..32 {
                predictions[(10 + channel) * anchors + anchor] =
                    if name == "mixed" { next() } else { 1.0 };
            }
        }
        let mut prototypes = vec![0.0; 32 * 104 * 104];
        for channel in 0..32 {
            for row in 0..104 {
                for column in 0..104 {
                    prototypes[channel * 104 * 104 + row * 104 + column] = match name {
                        "positive" | "below-cutoff" | "ignored" => 1.0,
                        "empty" => -1.0,
                        "upper-only" => {
                            if row < 52 {
                                1.0
                            } else {
                                -1.0
                            }
                        }
                        "cancellation" => [1e8, 1.0, -1e8, 1.0][channel % 4],
                        "nonfinite" => {
                            [f32::INFINITY, f32::NEG_INFINITY, f32::NAN][(column + channel) % 3]
                        }
                        "mixed" => next(),
                        _ => unreachable!(),
                    };
                }
            }
        }
        Case {
            name,
            predictions,
            prototypes,
        }
    })
    .collect()
}
