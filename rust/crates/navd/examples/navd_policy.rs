use openpilot_navd::{
    geometry::{self, Coordinate},
    instructions, Error,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{self, BufRead};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Request {
    Distance {
        a: Coordinate,
        b: Coordinate,
    },
    Minimum {
        a: Coordinate,
        b: Coordinate,
        position: Coordinate,
    },
    Along {
        geometry: Vec<Coordinate>,
        position: Coordinate,
    },
    Limit {
        points: Vec<u32>,
        maximum: usize,
    },
    Banner {
        banners: Value,
        distance: f64,
    },
    Direction {
        value: String,
    },
}

fn run(request: Request) -> Result<Value, Error> {
    Ok(match request {
        Request::Distance { a, b } => json!(a.distance_to(b)?),
        Request::Minimum { a, b, position } => json!(geometry::minimum_distance(a, b, position)?),
        Request::Along { geometry, position } => {
            json!(geometry::distance_along_geometry(&geometry, position)?)
        }
        Request::Limit { points, maximum } => {
            json!(geometry::limit_route_points(&points, maximum)?)
        }
        Request::Banner { banners, distance } => {
            serde_json::to_value(instructions::parse_banner_instructions(&banners, distance)?)?
        }
        Request::Direction { value } => {
            serde_json::to_value(instructions::string_to_direction(&value))?
        }
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let request = serde_json::from_str(&line?)?;
        let value = match run(request) {
            Ok(value) => json!({"ok":true,"value":value}),
            Err(error) => json!({"ok":false,"error":error.to_string()}),
        };
        println!("{value}");
    }
    Ok(())
}
