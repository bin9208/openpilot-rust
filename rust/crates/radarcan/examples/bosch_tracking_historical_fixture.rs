//! Explicit historical environment comparison; the ordinary regression uses the
//! hash-verified original source in the same environment.
#[path = "../tests/support/bosch_tracking_state.rs"]
mod support;

fn main() {
    support::check(&support::frozen_fixture());
}
