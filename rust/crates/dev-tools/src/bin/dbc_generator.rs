fn main() {
    let json_errors = std::env::args_os()
        .nth(1)
        .is_some_and(|arg| arg == "--json-errors");
    if let Err(error) = openpilot_dev_tools::dbc_generator::generator_cli() {
        if json_errors {
            eprint!("{}", error.json_line());
        } else {
            eprintln!("{error}");
        }
        std::process::exit(if error.kind() == "ArgumentError" {
            2
        } else {
            1
        });
    }
}
