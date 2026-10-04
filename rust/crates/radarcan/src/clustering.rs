use crate::{numerics::Numerics, scalar, Error};

pub struct Correlation {
    pub labels: Vec<i64>,
    pub dot: Vec<f64>,
    pub distance: Vec<f64>,
}

pub fn correlate(
    previous: &[[f64; 3]],
    current: &[[f64; 3]],
    max_distance: f64,
    numerics: &Numerics,
) -> Result<Correlation, Error> {
    if current.is_empty() || previous.is_empty() {
        return Ok(Correlation {
            labels: vec![-1; current.len()],
            dot: Vec::new(),
            distance: Vec::new(),
        });
    }
    let threshold = scalar::square(max_distance)?;
    let dot = numerics.dot_points(current, previous)?;
    let norm = |point: &[f64; 3]| (point[0] * point[0] + point[1] * point[1]) + point[2] * point[2];
    let previous_norm = previous.iter().map(norm).collect::<Vec<_>>();
    let current_norm = current.iter().map(norm).collect::<Vec<_>>();
    let mut distance = Vec::with_capacity(dot.len());
    let mut labels = Vec::with_capacity(current.len());
    for (row, current) in current_norm.iter().enumerate() {
        let mut closest = 0;
        let mut closest_distance = f64::INFINITY;
        for (column, previous) in previous_norm.iter().enumerate() {
            let value = (current + previous) - 2. * dot[row * previous_norm.len() + column];
            let value = if value.is_nan() || value > 0. {
                value
            } else {
                0.
            };
            distance.push(value);
            if column == 0
                || (!closest_distance.is_nan() && (value.is_nan() || value < closest_distance))
            {
                closest = column;
                closest_distance = value;
            }
        }
        labels.push(if closest_distance < threshold {
            i64::try_from(closest).map_err(|_| Error::IntegerOverflow)?
        } else {
            -1
        });
    }
    Ok(Correlation {
        labels,
        dot,
        distance,
    })
}
