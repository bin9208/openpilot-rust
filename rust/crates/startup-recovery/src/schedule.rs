//! Source recovery display checks once after five seconds, then every thirty.
pub struct Schedule {
    next_check: f64,
    was_busy: bool,
}
impl Schedule {
    pub fn new(now: f64) -> Self {
        Self {
            next_check: now + 5.0,
            was_busy: false,
        }
    }
    pub fn poll(&mut self, busy: bool, now: impl FnMut() -> f64) -> bool {
        self.poll_with(busy, now, || Ok::<(), std::convert::Infallible>(()))
            .expect("infallible schedule action")
    }
    pub fn poll_with<E>(
        &mut self,
        busy: bool,
        mut now: impl FnMut() -> f64,
        action: impl FnOnce() -> Result<(), E>,
    ) -> Result<bool, E> {
        if self.was_busy && !busy {
            self.next_check = now() + 30.0;
        }
        self.was_busy = busy;
        if !busy && now() >= self.next_check {
            action()?;
            self.next_check = now() + 30.0;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_action_occurs_before_the_retry_deadline_clock() {
        let rows: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/startup-recovery-schedule-final.json"
        ))
        .unwrap();
        for row in rows.as_array().unwrap() {
            let mut schedule = Schedule::new(f64::from_bits(row["start_bits"].as_u64().unwrap()));
            for step in row["steps"].as_array().unwrap() {
                let events = std::cell::RefCell::new(step["events"].as_array().unwrap().iter());
                let called = schedule
                    .poll_with(
                        matches!(
                            step["status"].as_str().unwrap(),
                            "updating" | "cleaning" | "rebooting" | "rebuild_rebooting"
                        ),
                        || {
                            let event = events.borrow_mut().next().unwrap();
                            assert_eq!(event[0], "time");
                            f64::from_bits(event[1].as_u64().unwrap())
                        },
                        || {
                            let event = events.borrow_mut().next().unwrap();
                            assert_eq!(*event, serde_json::json!(["start", {"automatic":true}]));
                            Ok::<(), ()>(())
                        },
                    )
                    .unwrap();
                assert_eq!(called, step["called"].as_bool().unwrap());
                assert!(events.borrow_mut().next().is_none());
                assert_eq!(
                    schedule.next_check.to_bits(),
                    step["next_bits"].as_u64().unwrap()
                );
            }
        }
    }
    #[test]
    fn original_loop_clock_reads_and_retry_deadlines() {
        let rows: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/startup-recovery-schedule-final.json"
        ))
        .unwrap();
        for row in rows.as_array().unwrap() {
            let mut schedule = Schedule::new(f64::from_bits(row["start_bits"].as_u64().unwrap()));
            for step in row["steps"].as_array().unwrap() {
                let busy = matches!(
                    step["status"].as_str().unwrap(),
                    "updating" | "cleaning" | "rebooting" | "rebuild_rebooting"
                );
                let mut ticks = step["ticks"].as_array().unwrap().iter();
                assert_eq!(
                    schedule.poll(busy, || f64::from_bits(
                        ticks.next().unwrap().as_u64().unwrap()
                    )),
                    step["called"].as_bool().unwrap()
                );
                assert!(ticks.next().is_none());
                assert_eq!(
                    schedule.next_check.to_bits(),
                    step["next_bits"].as_u64().unwrap()
                );
                assert_eq!(schedule.was_busy, step["was_busy"].as_bool().unwrap());
            }
        }
    }
}
