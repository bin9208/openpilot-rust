use std::io::{self, Write};
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args == ["--help"] || args == ["-h"] {
        println!("Usage: list-service-ports");
    } else if !args.is_empty() {
        eprintln!("list-service-ports takes no arguments");
        std::process::exit(1);
    } else if let Err(error) = io::stdout()
        .lock()
        .write_all(openpilot_service_tools::catalog_ports().as_bytes())
    {
        eprintln!("Service ports: {error}");
        std::process::exit(1);
    }
}
