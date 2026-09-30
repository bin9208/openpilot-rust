use openpilot_loggerd::writer::{identifier, route_name};
use openpilot_params::Params;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key = std::env::args().nth(1).ok_or("expected counter key")?;
    let params = Params::for_runtime()?;
    let result = if key == "RouteCount" {
        route_name(&params)?
    } else {
        identifier(&params, &key)?
    };
    println!("{result}");
    Ok(())
}
