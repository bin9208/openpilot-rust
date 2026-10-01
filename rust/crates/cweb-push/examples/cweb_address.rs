fn main() -> Result<(), Box<dyn std::error::Error>> {
    let names: Vec<String> = std::env::args().skip(1).collect();
    let values: Vec<_> = names
        .iter()
        .map(|name| openpilot_cweb_push::address::interface_ipv4(name))
        .collect();
    println!("{}", serde_json::to_string(&values)?);
    Ok(())
}
