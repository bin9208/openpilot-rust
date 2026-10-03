use std::{
    io::{self, Read},
    path::Path,
};
#[path = "volkswagen_trace/input.rs"]
mod input;
#[path = "volkswagen_trace/parser.rs"]
mod parser;
#[path = "volkswagen_trace/steps.rs"]
mod steps;
#[path = "volkswagen_trace/trace.rs"]
mod trace;
#[path = "volkswagen_trace/io.rs"]
mod trace_io;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = String::new();
    io::stdin().read_to_string(&mut data)?;
    let cases: Vec<input::Case> = serde_json::from_str(&data)?;
    let args: Vec<_> = std::env::args().collect();
    let output = args.get(1).ok_or("output path")?;
    let paths = [
        Path::new(args.get(2).ok_or("DBC root")?),
        Path::new(args.get(3).ok_or("torque assets")?),
        Path::new(args.get(4).ok_or("numerics")?),
    ];
    let rows = cases
        .into_iter()
        .map(|case| trace::run(case, paths))
        .collect::<Result<Vec<_>, _>>()?;
    std::fs::write(output, serde_json::to_vec(&rows)?)?;
    Ok(())
}
