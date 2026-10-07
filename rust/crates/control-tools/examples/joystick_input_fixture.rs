use openpilot_control_tools::{
    joystick_input::{self, Entry, Options},
    Error,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let input = args
        .next()
        .ok_or(Error::Contract("owned fixture input path required"))?
        .into();
    let entry = match args.next().as_deref() {
        None => Entry::GamepadCli,
        Some("--managed") => Entry::Managed,
        Some(_) => return Err(Error::Contract("unexpected fixture argument").into()),
    };
    if args.next().is_some() {
        return Err(Error::Contract("one owned input path required").into());
    }
    joystick_input::run(Options {
        entry,
        input: Some(input),
        frames: None,
    })?;
    Ok(())
}
