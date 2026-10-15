//! Raw Float32 bit inputs preserve NaNs and signed zero in source sort gates.
use openpilot_signal_observer::numpy_sort::scalar_argsort;
use std::io::{self, BufRead, Write};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut output = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let values = line?
            .split_whitespace()
            .map(|v| v.parse::<u32>().map(f32::from_bits))
            .collect::<Result<Vec<_>, _>>()?;
        let order = scalar_argsort(&values);
        for (i, value) in order.iter().enumerate() {
            if i > 0 {
                write!(output, " ")?;
            }
            write!(output, "{value}")?;
        }
        writeln!(output)?;
    }
    Ok(())
}
