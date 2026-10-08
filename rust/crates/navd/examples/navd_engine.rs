use openpilot_navd::{
    geometry::Coordinate,
    route::{
        Config, Diagnostic, Endpoint, Instruction, Ports, Position, RequestError, RouteEngine,
    },
    wire, Error,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    io::{self, BufRead},
};

#[derive(Deserialize)]
struct Case {
    parameters: BTreeMap<String, String>,
    steps: Vec<Operation>,
}

#[derive(Deserialize)]
struct Process {
    name: String,
    pid: i32,
    running: bool,
}

#[derive(Deserialize)]
struct Location {
    #[serde(rename = "xPosLat")]
    latitude: f64,
    #[serde(rename = "xPosLon")]
    longitude: f64,
    #[serde(rename = "xPosAngle")]
    bearing: f64,
}

#[derive(Deserialize)]
struct Response {
    #[serde(default)]
    network_error: bool,
    status: Option<u16>,
    raw: Option<String>,
    #[serde(default)]
    body: Value,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Operation {
    Parameter {
        key: String,
        value: Option<String>,
    },
    Response {
        value: Response,
    },
    Update {
        position: Location,
        manager: Option<Vec<Process>>,
    },
    ShouldRecompute,
    Instruction,
    SendRoute,
    Clear,
    Reset,
    Calculate {
        destination: Value,
    },
}

struct Fixture {
    parameters: BTreeMap<String, String>,
    responses: VecDeque<Response>,
    effects: Vec<Value>,
    diagnostics: Vec<&'static str>,
}

impl Ports for Fixture {
    fn parameter(&mut self, name: &str) -> Result<Option<String>, Error> {
        Ok(self.parameters.get(name).cloned())
    }
    fn remove_parameter(&mut self, name: &str) -> Result<(), Error> {
        self.effects.push(json!({"remove":name}));
        self.parameters.remove(name);
        Ok(())
    }
    fn request(&mut self, url: &str) -> Result<Value, RequestError> {
        self.effects.push(json!({"request":url,"timeout":10}));
        let response = self
            .responses
            .pop_front()
            .ok_or_else(|| RequestError::Transport("missing fixture response".into()))?;
        if response.network_error {
            return Err(RequestError::Transport("owned fixture".into()));
        }
        let status = response.status.unwrap_or(200);
        if status != 200 {
            self.diagnostics.push("api_failed");
        }
        if status >= 400 {
            return Err(RequestError::Status {
                status,
                body: response.raw.unwrap_or_else(|| response.body.to_string()),
            });
        }
        Ok(match response.raw {
            Some(text) => serde_json::from_str(&text)?,
            None => response.body,
        })
    }
    fn instruction(&mut self, message: &Instruction) -> Result<(), Error> {
        self.effects
            .push(json!({"event":wire::instruction(message, 0)?}));
        Ok(())
    }
    fn route(&mut self, coordinates: &[Coordinate]) -> Result<(), Error> {
        self.effects
            .push(json!({"event":wire::route(coordinates, 0)?}));
        Ok(())
    }
    fn diagnostic(&mut self, event: Diagnostic) {
        let name = match event {
            Diagnostic::NewDestination { .. } => "new_destination",
            Diagnostic::Calculating { .. } => "calculating",
            Diagnostic::EmptyRoute => "empty_route",
            Diagnostic::RequestFailed(_) => "request_failed",
            Diagnostic::ComputeFailed(_) => "compute_failed",
            Diagnostic::RouteLimited { .. } => "route_limited",
            Diagnostic::DestinationReached => "destination_reached",
            Diagnostic::SpeedLimit(_) => return,
        };
        self.diagnostics.push(name);
    }
}

fn execute(
    engine: &mut RouteEngine,
    fixture: &mut Fixture,
    operation: Operation,
) -> Result<Value, Error> {
    match operation {
        Operation::Parameter { key, value } => match value {
            Some(value) => {
                fixture.parameters.insert(key, value);
            }
            None => {
                fixture.parameters.remove(&key);
            }
        },
        Operation::Response { value } => fixture.responses.push_back(value),
        Operation::Update { position, manager } => {
            if let Some(manager) = manager {
                let pid = manager
                    .into_iter()
                    .find(|process| process.name == "ui" && process.running)
                    .map(|process| process.pid);
                if engine.update_ui_pid(pid) {
                    fixture.diagnostics.push("ui_restart");
                    fixture.effects.push(json!({"timer":5.}));
                }
            }
            engine.update(
                Position {
                    latitude: position.latitude,
                    longitude: position.longitude,
                    bearing: position.bearing,
                },
                fixture,
            )?;
        }
        Operation::ShouldRecompute => return Ok(json!(engine.should_recompute()?)),
        Operation::Instruction => engine.send_instruction(fixture)?,
        Operation::SendRoute => engine.send_route(fixture)?,
        Operation::Clear => engine.clear_route(fixture)?,
        Operation::Reset => engine.reset_recompute_limits(),
        Operation::Calculate { destination } => {
            engine.calculate_route(Endpoint::from_json(&destination)?, fixture)?
        }
    }
    Ok(Value::Null)
}

fn trace(case: Case) -> Result<Vec<Value>, Error> {
    let mut fixture = Fixture {
        parameters: case.parameters,
        responses: VecDeque::new(),
        effects: Vec::new(),
        diagnostics: Vec::new(),
    };
    let mut engine = RouteEngine::new(
        Config {
            host: "https://api.mapbox.com".into(),
            token: Some("fixture-token".into()),
        },
        &mut fixture,
    )?;
    let mut output = Vec::new();
    for operation in case.steps {
        fixture.effects.clear();
        fixture.diagnostics.clear();
        let result = execute(&mut engine, &mut fixture, operation);
        let error = result.is_err();
        if let Err(error) = &result {
            eprintln!("{error}");
        }
        output.push(json!({"state":engine.snapshot(),"parameters":fixture.parameters,"effects":fixture.effects,
            "result":result.unwrap_or(Value::Null),"error":error,"diagnostics":fixture.diagnostics}));
    }
    Ok(output)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let case = serde_json::from_str(&line?)?;
        println!("{}", serde_json::to_string(&trace(case)?)?);
    }
    Ok(())
}
