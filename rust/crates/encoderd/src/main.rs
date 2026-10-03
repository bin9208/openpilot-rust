use openpilot_encoderd::{
    config::Mode,
    native::{platform, runtime},
};
use openpilot_logging::{log_site, record::Level};
fn main() {
    platform::initialize();
    let result = (|| {
        let segment_length = platform::segment_length()?;
        let recording = platform::recording()?;
        let argument = std::env::args().nth(1);
        if let Some(mode) = Mode::parse(argument.as_deref()) {
            runtime::run(mode, recording, segment_length)?;
        } else {
            platform::schedule(Mode::Main)?;
            platform::emit(
                log_site!(),
                Level::Error,
                format!(
                    "Argument '{}' is not supported",
                    argument.unwrap_or_default()
                ),
            );
        }
        Ok(())
    })();
    if let Err(error) = result {
        runtime::fatal(error);
    }
    platform::close();
}
