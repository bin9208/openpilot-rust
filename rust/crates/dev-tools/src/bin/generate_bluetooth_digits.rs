use std::io::Write;

fn main() {
    let result = openpilot_dev_tools::bluetooth_digits_generate::generate()
        .and_then(|output| Ok(std::io::stdout().lock().write_all(output.as_bytes())?));
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
