fn main() -> Result<(), Box<dyn std::error::Error>> {
    openpilot_dev_tools::bluetooth_bluez_policy_check::cli()
}
