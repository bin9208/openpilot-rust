//! Integration tests for `Predicate::Updated` (issue #346).
//!
//! The updater daemon must stay runnable both onroad and offroad whenever the
//! user's `SoftwareMenu` preference is enabled. `SoftwareMenu=false` is the
//! user's explicit opt-out and must keep the updater off. Therefore
//! `Predicate::Updated` must:
//!
//!   * evaluate to the `SoftwareMenu` value regardless of `State::started`,
//!     i.e. across ignition on/off transitions, and
//!   * consult the parameter store exactly once and never touch GPS paths.
//!
//! Before the fix, `Updated` was `!started && SoftwareMenu`, so the
//! onroad-enabled row (`started=true, SoftwareMenu=true`) returned `false`.

use openpilot_manager_catalog::{Error, GpsPaths, Parameters, Predicate, State};

/// Parameter-store fake that permits only `get_bool("SoftwareMenu")`.
///
/// It records how many times the preference is read and returns the supplied
/// value. Any other access panics, so an implementation that reads extra keys,
/// writes, or falls back to a default is called out by name.
struct SoftwareMenuParams {
    software_menu: bool,
    reads: usize,
}

impl Parameters for SoftwareMenuParams {
    fn get_bool(&mut self, key: &str) -> Result<bool, Error> {
        assert_eq!(key, "SoftwareMenu", "unexpected parameter read: {key:?}");
        self.reads += 1;
        Ok(self.software_menu)
    }

    fn get_int(&mut self, _key: &str) -> Result<i32, Error> {
        panic!("Updated must not read integer parameters");
    }

    fn get_bool_default(&mut self, _key: &str) -> Result<bool, Error> {
        panic!("Updated must not fall back to a default parameter");
    }

    fn put_bool(&mut self, _key: &str, _value: bool) -> Result<(), Error> {
        panic!("Updated must not write parameters");
    }
}

/// GPS path fake: `Updated` must never consult the filesystem.
struct NoGpsPaths;

impl GpsPaths for NoGpsPaths {
    fn exists(&mut self, _path: &str) -> bool {
        panic!("Updated must not consult GPS paths");
    }
}

/// `Updated` must equal the `SoftwareMenu` preference for every
/// `started` x `SoftwareMenu` combination, with exactly one read per row.
#[test]
fn updated_follows_software_menu_onroad_and_offroad() {
    // (started, SoftwareMenu) matrix: offroad/onroad x opt-out/allowed.
    for (started, software_menu) in [(false, false), (false, true), (true, false), (true, true)] {
        let mut params = SoftwareMenuParams {
            software_menu,
            reads: 0,
        };
        let mut paths = NoGpsPaths;

        let enabled = Predicate::Updated
            .evaluate(
                State {
                    started,
                    not_car: false,
                },
                &mut params,
                &mut paths,
            )
            .unwrap_or_else(|err| {
                panic!(
                    "Updated evaluate failed (started={started}, SoftwareMenu={software_menu}): {err:?}"
                )
            });

        let context = format!("started={started}, SoftwareMenu={software_menu}");
        assert_eq!(
            enabled,
            software_menu,
            "{context}: updater must be runnable onroad and offroad when enabled, and stay off when the user opts out"
        );
        assert_eq!(
            params.reads, 1,
            "{context}: Updated must read SoftwareMenu exactly once"
        );
    }
}
