fn main() {
    if let Err(error) = openpilot_carrot_man::native::run() {
        eprintln!("carrot_man: {error}");
        std::process::exit(1);
    }
}
