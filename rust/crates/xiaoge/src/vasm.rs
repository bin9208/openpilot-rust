use crate::Error;

/// Final V-ASM classifier: squeeze singleton axes and select occupied class1.
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
    if count != values.len() || !matches!(dimensions.as_slice(), [2] | [3]) {
        return Err(Error::Invalid("unexpected V-ASM model output shape"));
    }
    Ok(f64::from(values[1]))
}
