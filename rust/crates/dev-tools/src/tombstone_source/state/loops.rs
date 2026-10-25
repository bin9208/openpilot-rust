//! One original queue/sleep boundary per Worker start/cycle request.
//! The source thread performs no work while blocked on its advance queue, so
//! keeping that suspended state directly preserves the deterministic protocol
//! without an unowned background thread or real five-second sleeps.
use super::{files::concat, path, scalar, Worker};
use crate::tombstone_source::{value as v, Error, Result};
use openpilot_log_format::JsonValue as J;
use std::{collections::BTreeSet, fs};

pub(super) struct Loop {
    should_report: bool,
    previous: BTreeSet<(Vec<u32>, i64)>,
}
impl Worker {
    fn loop_iteration(&mut self, state: &mut Loop) -> Result<()> {
        let current = self
            .scan()?
            .into_iter()
            .map(|(path, time)| (v::string(&path), time))
            .collect::<BTreeSet<_>>();
        for (points, _) in current.difference(&state.previous) {
            self.check_control()?;
            let filename = v::text(points.clone());
            if !state.should_report {
                if let Ok(path) = self.actual_path(&filename) {
                    let _ = fs::remove_file(path);
                }
                continue;
            }
            self.log(
                20,
                concat(&[J::text("reporting new tombstone "), filename.clone()]),
                false,
            );
            let result = if points.ends_with(&[46, 99, 114, 97, 115, 104]) {
                self.report(&filename, &v::object(&[]))
            } else {
                self.log(
                    40,
                    concat(&[J::text("unknown crash type: "), filename.clone()]),
                    false,
                );
                Ok(())
            };
            if result.is_err() {
                self.log(
                    40,
                    concat(&[J::text("Error reporting tombstone "), filename]),
                    true,
                );
            }
        }
        // The original updates membership from the scan before report/removal,
        // so a following empty cycle is needed before a same-name file is new.
        state.previous = current;
        Ok(())
    }
    pub(super) fn step(&mut self, request: &J, op: &J) -> Result<J> {
        if op.text_eq("start") {
            let mut apport = v::points(&v::field(request, "apport")?, "TypeError")?;
            apport.push(47);
            self.apport = v::text(apport);
            let root = path(&v::field(request, "root")?)?;
            let stamp = v::field(request, "stamp")?;
            self.root = root;
            self.stamp = stamp;
            let enabled = self.init(&J::text("selfdrive_native"))?;
            self.enabled = enabled.clone();
            self.clear_apport();
            let previous = self
                .scan()?
                .into_iter()
                .map(|(path, time)| (v::string(&path), time))
                .collect();
            let mut state = Loop {
                should_report: v::truth(&enabled),
                previous,
            };
            self.loop_iteration(&mut state)?;
            self.active = Some(state);
            return Ok(self.enabled.clone());
        }
        let mut state = self
            .active
            .take()
            .ok_or_else(|| Error::new("Empty", "source daemon is not awaiting an advance"))?;
        self.loop_iteration(&mut state)?;
        self.active = Some(state);
        Ok(scalar("null"))
    }
}
