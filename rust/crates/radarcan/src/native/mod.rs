mod carlog;
mod fixture;
mod io;
mod options;
#[expect(
    unsafe_code,
    reason = "native scheduler ABI; injected argument fixture is validated by Miri"
)]
mod platform;
mod run;
pub use fixture::Paths as FixturePaths;
pub use options::{parse, Options};
pub use run::run;
