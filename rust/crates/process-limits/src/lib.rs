//! Narrow post-fork resource-limit boundary for native oracle processes.
use std::{io, os::unix::process::CommandExt, process::Command};

/// Set both core-file limits to zero in the child before exec.
/// The parent and subsequently launched unrelated children retain their limits.
pub fn disable_core_dump(command: &mut Command) {
    // SAFETY: after fork this closure only invokes setrlimit. It does not
    // allocate, acquire locks, or access mutable state from another thread.
    unsafe {
        command.pre_exec(|| {
            rustix::process::setrlimit(
                rustix::process::Resource::Core,
                rustix::process::Rlimit {
                    current: Some(0),
                    maximum: Some(0),
                },
            )
            .map_err(io::Error::from)
        });
    }
}

/// Start the child in a fresh session and process group before exec.
/// The parent's session and all arguments/environment remain unchanged.
pub fn new_session(command: &mut Command) {
    // SAFETY: setsid is async-signal-safe. This post-fork closure only invokes
    // that syscall and converts its errno; it allocates nothing and holds no locks.
    unsafe {
        command.pre_exec(|| {
            rustix::process::setsid()
                .map(|_| ())
                .map_err(io::Error::from)
        });
    }
}

#[cfg(all(feature = "qa-process-title", target_os = "linux", target_env = "gnu"))]
pub mod source_title;
