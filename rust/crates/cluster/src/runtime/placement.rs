use openpilot_ui_application::scheduling::{Native, Request, Scheduler};
use std::io;
pub(super) struct Placement {
    scheduler: Scheduler,
    native: Native,
}
impl Placement {
    pub fn new(board: bool) -> io::Result<Self> {
        let mut value = Self {
            scheduler: Scheduler::new(board),
            native: Native { root: "/".into() },
        };
        value.scheduler.core = 7;
        value.update(false, true, None, 0.0)?;
        Ok(value)
    }
    pub fn update(
        &mut self,
        onroad: bool,
        force: bool,
        child: Option<u32>,
        now: f64,
    ) -> io::Result<()> {
        self.scheduler.update(
            Request {
                onroad,
                force,
                child_pid: child
                    .map(i32::try_from)
                    .transpose()
                    .map_err(io::Error::other)?,
                now,
            },
            &mut self.native,
        )
    }
}
