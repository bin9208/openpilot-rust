//! Actual Rust callsite and OS process/thread identity; no fabricated Python metadata.
use crate::{record::Metadata, Error};

#[derive(Clone, Copy, Debug)]
pub struct Site {
    pub file: &'static str,
    pub line: u32,
    pub module: &'static str,
    pub function: &'static str,
}
impl Site {
    pub fn metadata(self, host: &str) -> Result<Metadata, Error> {
        let thread_name = rustix::thread::name().map_err(std::io::Error::from)?;
        let created = rustix::time::clock_gettime(rustix::time::ClockId::Realtime);
        Ok(Metadata {
            pathname: self.file.into(),
            lineno: self.line,
            module: self.module.into(),
            function: self.function.into(),
            host: host.into(),
            process: std::process::id(),
            thread: rustix::thread::gettid().as_raw_nonzero().get() as u32,
            thread_name: thread_name
                .to_str()
                .map_err(|_| Error::Contract("native thread name is not UTF-8"))?
                .into(),
            created: created.tv_sec as f64 + created.tv_nsec as f64 / 1e9,
        })
    }
}
#[doc(hidden)]
pub fn function_name<T>(_: T) -> &'static str {
    let name = std::any::type_name::<T>();
    name.strip_suffix("::__openpilot_logging_site")
        .unwrap_or(name)
}

#[macro_export]
macro_rules! log_site {
    () => {{
        fn __openpilot_logging_site() {}
        $crate::site::Site {
            file: file!(),
            line: line!(),
            module: module_path!(),
            function: $crate::site::function_name(__openpilot_logging_site),
        }
    }};
}
