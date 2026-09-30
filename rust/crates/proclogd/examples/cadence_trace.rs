use openpilot_proclogd::cadence::Cadence;
use std::{
    error::Error,
    io::{self, BufRead},
    time::Duration,
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut cadence = Cadence::default();
    for line in io::stdin().lock().lines() {
        let now = Duration::from_nanos(line?.parse()?);
        let deadline = cadence.deadline(now);
        println!("{}", deadline.saturating_sub(now).as_nanos());
    }
    Ok(())
}
