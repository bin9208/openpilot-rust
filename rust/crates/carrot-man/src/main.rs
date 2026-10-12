fn main() {
    if let Err(error) = openpilot_managed_entry::runtime(
        "openpilot.selfdrive.carrot.carrot_man",
        "carrot_man",
        || openpilot_carrot_man::native::run().map_err(openpilot_managed_entry::StepError::raised),
    ) {
        eprintln!("carrot_man: {error}");
        std::process::exit(1);
    }
}
