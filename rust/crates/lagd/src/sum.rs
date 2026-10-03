// NumPy's contiguous f64 pairwise reduction, also used by the existing torqued port.
pub fn pairwise(values: &[f64]) -> f64 {
    if values.len() < 8 {
        return values.iter().fold(-0., |a, b| a + b);
    }
    if values.len() <= 128 {
        let mut sums = [0.; 8];
        sums.copy_from_slice(&values[..8]);
        let mut index = 8;
        while index + 8 <= values.len() {
            for offset in 0..8 {
                sums[offset] += values[index + offset];
            }
            index += 8;
        }
        let total = ((sums[0] + sums[1]) + (sums[2] + sums[3]))
            + ((sums[4] + sums[5]) + (sums[6] + sums[7]));
        return values[index..].iter().fold(total, |a, b| a + b);
    }
    let middle = (values.len() / 2) / 8 * 8;
    pairwise(&values[..middle]) + pairwise(&values[middle..])
}
