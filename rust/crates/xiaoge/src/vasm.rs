use crate::Error;

pub fn confidence(shape: &[i32], values: &[f32]) -> Result<f64, Error> {
    let mut dimensions = Vec::with_capacity(shape.len());
    let mut count = 1usize;
    for &size in shape {
        let size =
            usize::try_from(size).map_err(|_| Error::Invalid("negative blindspot output axis"))?;
        count = count
            .checked_mul(size)
            .ok_or(Error::Invalid("blindspot output size overflow"))?;
        if size != 1 {
            dimensions.push(size);
        }
    }
    if count != values.len() {
        return Err(Error::Invalid("invalid blindspot output tensor shape"));
    }
    let maximum = |rows: Vec<f32>| -> Result<f64, Error> {
        let mut items = rows.into_iter();
        let mut best = items.next().ok_or(Error::Invalid(
            "zero-size array to reduction operation maximum which has no identity",
        ))?;
        for value in items {
            if value.is_nan() || value > best {
                best = value;
            }
            if best.is_nan() {
                break;
            }
        }
        Ok(f64::from(best))
    };
    if let [height, width] = dimensions[..] {
        let transpose = height < width;
        let (rows, columns) = if transpose {
            (width, height)
        } else {
            (height, width)
        };
        let at = |row: usize, column: usize| {
            values[if transpose {
                column * width + row
            } else {
                row * width + column
            }]
        };
        if columns >= 6 {
            let selected: Vec<_> = (0..rows)
                .filter(|&row| at(row, 5).round_ties_even() == 0.0)
                .map(|row| at(row, 4))
                .collect();
            return if selected.is_empty() {
                Ok(0.0)
            } else {
                maximum(selected)
            };
        }
        if columns == 0 {
            return Err(Error::Invalid(
                "index 0 is out of bounds for axis 1 with size 0",
            ));
        }
        return maximum(
            (0..rows)
                .map(|row| at(row, if columns >= 5 { 4 } else { 0 }))
                .collect(),
        );
    }
    if values.is_empty() {
        return Ok(0.0);
    }
    maximum(values.to_vec())
}
