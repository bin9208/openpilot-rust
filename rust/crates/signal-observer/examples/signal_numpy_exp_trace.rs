//! Exact input/output Float32 bits for source math dispatch comparison.
use std::io::{self, BufRead};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let bits = line?.parse::<u32>()?;
        println!(
            "{}",
            openpilot_signal_observer::numpy_exp::exp(f32::from_bits(bits)).to_bits()
        );
    }
    Ok(())
}
