fn main() {
    for candidate in ["C.UTF-8", "C", "owned-missing-locale", "invalid\0locale"] {
        match openpilot_startup_ui::try_process_locale(candidate) {
            Ok(available) => println!("{candidate:?}: {available}"),
            Err(error) => println!("{candidate:?}: {error}"),
        }
    }
}
