use openpilot_plannerd::solver::{Acados, Field, Kind};
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Read},
    path::PathBuf,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    kind: Kind,
    commands: Vec<Command>,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    Reset,
    Set {
        stage: usize,
        field: Field,
        values: Vec<f64>,
    },
    Solve,
}

#[derive(Serialize)]
struct Solution {
    status: i32,
    x: Vec<Vec<u64>>,
    u: Vec<Vec<u64>>,
    cost: u64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bundle = PathBuf::from(std::env::var("PLANNER_ACADOS")?);
    let output = PathBuf::from(std::env::args_os().nth(1).ok_or("output path required")?);
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let requests: Vec<Request> = serde_json::from_str(&text)?;
    let mut responses = Vec::new();
    for request in requests {
        let mut solver = Acados::load(&bundle, request.kind)?;
        let mut solutions = Vec::new();
        for command in request.commands {
            match command {
                Command::Reset => solver.reset()?,
                Command::Set {
                    stage,
                    field,
                    values,
                } => solver.set(stage, field, &values)?,
                Command::Solve => {
                    let status = solver.solve();
                    let mut x = vec![vec![0.; request.kind.states()]; request.kind.horizon() + 1];
                    let mut u = vec![vec![0.; 1]; request.kind.horizon()];
                    for (stage, values) in x.iter_mut().enumerate() {
                        solver.get(stage, Field::State, values)?;
                    }
                    for (stage, values) in u.iter_mut().enumerate() {
                        solver.get(stage, Field::Control, values)?;
                    }
                    solutions.push(Solution {
                        status,
                        x: x.into_iter()
                            .map(|row| row.into_iter().map(f64::to_bits).collect())
                            .collect(),
                        u: u.into_iter()
                            .map(|row| row.into_iter().map(f64::to_bits).collect())
                            .collect(),
                        cost: solver.cost().to_bits(),
                    });
                }
            }
        }
        responses.push(solutions);
    }
    std::fs::write(output, serde_json::to_vec(&responses)?)?;
    Ok(())
}
